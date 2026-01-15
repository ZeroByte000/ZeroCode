use anyhow::Result;
use semver::Version;
use serde_json::{json, Value};
use std::path::Path;
use std::time::Duration;
use zerocode_core::config::AppConfig;
use zerocode_core::plugin::PluginManager;
use zerocode_lsp::LspClient;
use zerocode_tui::EditorView;

fn main() -> Result<()> {
    if let Some(arg) = std::env::args().nth(1) {
        if arg == "--version" || arg == "-v" {
            println!("zerocode {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
    }

    let config_path = AppConfig::default_path();
    let config = AppConfig::load_optional(&config_path);
    let update_message = if config.update_check {
        check_update().unwrap_or(None)
    } else {
        None
    };

    let _client = LspClient::new();
    let _plugins = if Path::new("plugins.toml").exists() {
        let manager = PluginManager::load_from("plugins.toml")?;
        let running_plugins = match manager.spawn_all() {
            Ok(running) => running,
            Err(err) => {
                eprintln!("Plugin spawn failed: {err}");
                Vec::new()
            }
        };
        for plugin in &running_plugins {
            let _ = plugin.initialize();
            let _ = plugin.send_event(
                "document_opened",
                json!({ "text": "ZeroCode sample buffer" }),
            );
        }
        manager
    } else {
        PluginManager::new()
    };
    let mut view = EditorView::new();
    if let Some(message) = update_message {
        view.set_status(message);
    }
    view.run()?;
    Ok(())
}

fn check_update() -> Result<Option<String>> {
    let current = Version::parse(env!("CARGO_PKG_VERSION"))?;
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(2))
        .timeout_read(Duration::from_secs(2))
        .build();
    let response = agent
        .get("https://api.github.com/repos/ZeroByte000/ZeroCode/releases/latest")
        .set("User-Agent", "zerocode")
        .call();

    let response = match response {
        Ok(response) => response,
        Err(_) => return Ok(None),
    };

    let body = response.into_string().unwrap_or_default();
    let value: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    let tag = value
        .get("tag_name")
        .and_then(Value::as_str)
        .unwrap_or("");
    let latest = tag.strip_prefix('v').unwrap_or(tag);
    let latest = match Version::parse(latest) {
        Ok(version) => version,
        Err(_) => return Ok(None),
    };

    if latest > current {
        Ok(Some(format!(
            "Update tersedia: v{} (lihat Releases)",
            latest
        )))
    } else {
        Ok(None)
    }
}
