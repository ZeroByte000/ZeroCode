use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct Workspace {
    pub root: Option<PathBuf>,
}

impl Workspace {
    pub fn new(root: Option<PathBuf>) -> Self {
        Self { root }
    }
}
