use anyhow::Result;
use std::path::Path;
use zerocode_core::plugin::PluginManager;
use zerocode_lsp::LspClient;
use zerocode_tui::EditorView;
use serde_json::json;

fn main() -> Result<()> {
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
    view.run()?;
    Ok(())
}
