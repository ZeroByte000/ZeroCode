use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone)]
pub struct LspCommand {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Debug, Default)]
pub struct LspClient {
    pub server: Option<LspCommand>,
}

impl LspClient {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_server(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            server: Some(LspCommand {
                program: program.into(),
                args,
            }),
        }
    }

    pub fn connect(&self) -> anyhow::Result<LspConnection> {
        let server = self
            .server
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("LSP server not configured"))?;
        LspConnection::spawn(&server.program, &server.args)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    pub params: Value,
}

impl JsonRpcRequest {
    pub fn new(id: u64, method: impl Into<String>, params: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.into(),
            params,
        }
    }
}

pub struct LspConnection {
    _child: Child,
    outgoing: Sender<Value>,
    incoming: Receiver<Value>,
}

impl LspConnection {
    pub fn spawn(program: &str, args: &[String]) -> anyhow::Result<Self> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

        let stdin = child.stdin.take().ok_or_else(|| anyhow::anyhow!("LSP stdin unavailable"))?;
        let stdout = child.stdout.take().ok_or_else(|| anyhow::anyhow!("LSP stdout unavailable"))?;

        let (out_tx, out_rx) = mpsc::channel::<Value>();
        let (in_tx, in_rx) = mpsc::channel::<Value>();

        spawn_writer(stdin, out_rx);
        spawn_reader(stdout, in_tx);

        Ok(Self {
            _child: child,
            outgoing: out_tx,
            incoming: in_rx,
        })
    }

    pub fn send(&self, message: Value) -> anyhow::Result<()> {
        self.outgoing.send(message)?;
        Ok(())
    }

    pub fn send_request(&self, id: u64, method: &str, params: Value) -> anyhow::Result<()> {
        let request = JsonRpcRequest::new(id, method, params);
        self.send(serde_json::to_value(request)?)?;
        Ok(())
    }

    pub fn send_notification(&self, method: &str, params: Value) -> anyhow::Result<()> {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        self.send(notification)?;
        Ok(())
    }

    pub fn recv(&self) -> Option<Value> {
        self.incoming.recv().ok()
    }
}

pub struct LspSession {
    connection: LspConnection,
    next_id: AtomicU64,
}

impl LspSession {
    pub fn new(connection: LspConnection) -> Self {
        Self {
            connection,
            next_id: AtomicU64::new(1),
        }
    }

    pub fn initialize(&self, root_uri: Option<&str>) -> anyhow::Result<u64> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let params = serde_json::json!({
            "processId": std::process::id(),
            "rootUri": root_uri,
            "capabilities": {},
        });
        self.connection.send_request(id, "initialize", params)?;
        Ok(id)
    }

    pub fn initialized(&self) -> anyhow::Result<()> {
        self.connection
            .send_notification("initialized", serde_json::json!({}))?;
        Ok(())
    }

    pub fn did_open(&self, uri: &str, language_id: &str, version: i64, text: &str) -> anyhow::Result<()> {
        let params = serde_json::json!({
            "textDocument": {
                "uri": uri,
                "languageId": language_id,
                "version": version,
                "text": text,
            }
        });
        self.connection
            .send_notification("textDocument/didOpen", params)?;
        Ok(())
    }

    pub fn did_change(&self, uri: &str, version: i64, text: &str) -> anyhow::Result<()> {
        let params = serde_json::json!({
            "textDocument": {
                "uri": uri,
                "version": version,
            },
            "contentChanges": [
                { "text": text }
            ]
        });
        self.connection
            .send_notification("textDocument/didChange", params)?;
        Ok(())
    }

    pub fn did_save(&self, uri: &str) -> anyhow::Result<()> {
        let params = serde_json::json!({
            "textDocument": { "uri": uri }
        });
        self.connection
            .send_notification("textDocument/didSave", params)?;
        Ok(())
    }

    pub fn recv(&self) -> Option<Value> {
        self.connection.recv()
    }
}

fn spawn_writer(mut stdin: ChildStdin, rx: Receiver<Value>) {
    thread::spawn(move || {
        for message in rx {
            if let Ok(payload) = serde_json::to_vec(&message) {
                let _ = write_message(&mut stdin, &payload);
            }
        }
    });
}

fn spawn_reader(stdout: ChildStdout, tx: Sender<Value>) {
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            match read_message(&mut reader) {
                Ok(Some(payload)) => {
                    if let Ok(value) = serde_json::from_slice::<Value>(&payload) {
                        let _ = tx.send(value);
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }
    });
}

fn read_message(reader: &mut impl BufRead) -> anyhow::Result<Option<Vec<u8>>> {
    let mut content_length = None;
    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            return Ok(None);
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length:") {
            content_length = value.trim().parse::<usize>().ok();
        }
    }

    let length = match content_length {
        Some(length) => length,
        None => return Ok(None),
    };

    let mut buf = vec![0u8; length];
    reader.read_exact(&mut buf)?;
    Ok(Some(buf))
}

fn write_message(writer: &mut impl Write, payload: &[u8]) -> anyhow::Result<()> {
    write!(writer, "Content-Length: {}\r\n\r\n", payload.len())?;
    writer.write_all(payload)?;
    writer.flush()?;
    Ok(())
}
