#[derive(Debug, Clone, Copy)]
pub enum EditorCommand {
    OpenFile,
    SaveFile,
    Quit,
    Undo,
    Redo,
    Search,
    GoToLine,
}
