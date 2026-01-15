#[derive(Debug, Clone, Copy, Default)]
pub struct Cursor {
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Selection {
    pub start: Cursor,
    pub end: Cursor,
}
