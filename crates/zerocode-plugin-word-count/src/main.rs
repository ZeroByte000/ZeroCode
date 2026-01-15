use anyhow::Result;
use serde_json::Value;
use std::io::{self, BufReader, Write};
use zerocode_plugin_protocol::{
    read_message, write_message, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse,
};

fn main() -> Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = stdout.lock();

    loop {
        let payload = match read_message(&mut reader)? {
            Some(payload) => payload,
            None => break,
        };
        let value: Value = match serde_json::from_slice(&payload) {
            Ok(value) => value,
            Err(_) => continue,
        };

        if let Some(request) = parse_request(&value) {
            handle_request(&mut writer, request)?;
            continue;
        }

        if let Some(notification) = parse_notification(&value) {
            handle_notification(&mut writer, notification)?;
        }
    }

    Ok(())
}

fn parse_request(value: &Value) -> Option<JsonRpcRequest> {
    if value.get("id").is_some() && value.get("method").is_some() {
        serde_json::from_value(value.clone()).ok()
    } else {
        None
    }
}

fn parse_notification(value: &Value) -> Option<JsonRpcNotification> {
    if value.get("id").is_none() && value.get("method").is_some() {
        serde_json::from_value(value.clone()).ok()
    } else {
        None
    }
}

fn handle_request(writer: &mut impl Write, request: JsonRpcRequest) -> Result<()> {
    if request.method == "initialize" {
        let response = JsonRpcResponse::ok(
            request.id,
            serde_json::json!({
                "name": "word-count",
                "capabilities": {
                    "events": ["document_opened"]
                }
            }),
        );
        let payload = serde_json::to_vec(&response)?;
        write_message(writer, &payload)?;
    }
    Ok(())
}

fn handle_notification(writer: &mut impl Write, notification: JsonRpcNotification) -> Result<()> {
    if notification.method != "on_event" {
        return Ok(());
    }

    let event = notification.params.get("name").and_then(Value::as_str);
    if event != Some("document_opened") {
        return Ok(());
    }

    let text = notification
        .params
        .get("payload")
        .and_then(|payload| payload.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("");

    let count = text.split_whitespace().count();
    let log = JsonRpcNotification::new(
        "log",
        serde_json::json!({
            "plugin": "word-count",
            "message": format!("Words: {}", count),
        }),
    );
    let payload = serde_json::to_vec(&log)?;
    write_message(writer, &payload)?;
    Ok(())
}
