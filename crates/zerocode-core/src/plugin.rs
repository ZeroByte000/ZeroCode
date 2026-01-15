use serde::{Deserialize, Serialize};
use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use zerocode_plugin_protocol::{read_message, write_message, JsonRpcNotification, JsonRpcRequest};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PluginEntry {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct PluginConfig {
    #[serde(default)]
    pub plugins: Vec<PluginEntry>,
}

#[derive(Debug, Default)]
pub struct PluginManager {
    pub config_path: Option<PathBuf>,
    pub config: PluginConfig,
}

pub struct PluginConnection {
    pub name: String,
    _child: Child,
    outgoing: Sender<serde_json::Value>,
    incoming: Receiver<serde_json::Value>,
}

impl PluginManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load_from(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let content = fs::read_to_string(&path)?;
        let config: PluginConfig = toml::from_str(&content)?;
        Ok(Self {
            config_path: Some(path),
            config,
        })
    }

    pub fn spawn_all(&self) -> anyhow::Result<Vec<PluginConnection>> {
        let mut processes = Vec::new();
        for plugin in &self.config.plugins {
            let mut cmd = Command::new(&plugin.command);
            cmd.args(&plugin.args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit());
            let mut child = cmd.spawn()?;

            let stdin = child
                .stdin
                .take()
                .ok_or_else(|| anyhow::anyhow!("plugin stdin unavailable"))?;
            let stdout = child
                .stdout
                .take()
                .ok_or_else(|| anyhow::anyhow!("plugin stdout unavailable"))?;

            let (out_tx, out_rx) = mpsc::channel::<serde_json::Value>();
            let (in_tx, in_rx) = mpsc::channel::<serde_json::Value>();

            spawn_writer(stdin, out_rx);
            spawn_reader(stdout, in_tx);

            processes.push(PluginConnection {
                name: plugin.name.clone(),
                _child: child,
                outgoing: out_tx,
                incoming: in_rx,
            });
        }
        Ok(processes)
    }
}

impl PluginConnection {
    pub fn initialize(&self) -> anyhow::Result<()> {
        let params = serde_json::json!({
            "capabilities": {},
        });
        let request = JsonRpcRequest::new(1, "initialize", params);
        self.outgoing.send(serde_json::to_value(request)?)?;
        Ok(())
    }

    pub fn send_event(&self, name: &str, params: serde_json::Value) -> anyhow::Result<()> {
        let notification = JsonRpcNotification::new("on_event", serde_json::json!({
            "name": name,
            "payload": params,
        }));
        self.outgoing.send(serde_json::to_value(notification)?)?;
        Ok(())
    }

    pub fn recv(&self) -> Option<serde_json::Value> {
        self.incoming.recv().ok()
    }
}

fn spawn_writer(mut stdin: ChildStdin, rx: Receiver<serde_json::Value>) {
    thread::spawn(move || {
        for message in rx {
            if let Ok(payload) = serde_json::to_vec(&message) {
                let _ = write_message(&mut stdin, &payload);
            }
        }
    });
}

fn spawn_reader(stdout: ChildStdout, tx: Sender<serde_json::Value>) {
    thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            match read_message(&mut reader) {
                Ok(Some(payload)) => {
                    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&payload) {
                        let _ = tx.send(value);
                    }
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }
    });
}
