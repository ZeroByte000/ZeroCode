# ZeroCode Roadmap (LSP + Plugin)

## LSP Roadmap
1) **LSP client scaffold**
   - Define client trait and message loop (JSON-RPC 2.0).
   - Model server capabilities and editor requests.

2) **Process management**
   - Spawn language servers per workspace (stdio transport).
   - Manage lifecycle: initialize, shutdown, restart.

3) **Document sync**
   - Track open documents and versions.
   - Send textDocument/didOpen, didChange, didSave.

4) **Diagnostics pipeline**
   - Receive and store diagnostics.
   - Render diagnostics in gutter and status.

5) **Completion + hover**
   - Implement textDocument/completion and hover.
   - Add popup UI in TUI.

6) **Navigation**
   - Go-to-definition, references, rename.

7) **Performance**
   - Debounce change events.
   - Queue requests per server to avoid overload.

## Plugin Roadmap
1) **Plugin protocol**
   - JSON-RPC over stdio for local processes.
   - Minimal API: register commands, handle events.

2) **Plugin host**
   - Load from `plugins.toml`.
   - Start/stop plugins, track health.

3) **Events + hooks**
   - on_open, on_save, on_key, on_diagnostic.
   - Provide editor context snapshot (read-only).

4) **Commands + keybinding**
   - Plugins register commands.
   - Bind commands to keys via config.

5) **Permissions**
   - Simple allowlist: filesystem, network, shell.
   - Show prompt on first use.

6) **Plugin SDK**
   - Provide Rust crate for plugin authors.
   - Example plugin: word count, formatter wrapper.
