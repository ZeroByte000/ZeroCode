use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct TextBuffer {
    pub path: Option<PathBuf>,
    pub text: String,
    undo_stack: Vec<String>,
    redo_stack: Vec<String>,
}

impl TextBuffer {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            path: None,
            text: text.into(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn set_path(&mut self, path: PathBuf) {
        self.path = Some(path);
    }

    pub fn open(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        let text = fs::read_to_string(path)?;
        Ok(Self {
            path: Some(path.to_path_buf()),
            text,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        })
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let path = self
            .path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("buffer has no path"))?;
        fs::write(path, &self.text)?;
        Ok(())
    }

    pub fn save_as(&mut self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        let path = path.as_ref().to_path_buf();
        fs::write(&path, &self.text)?;
        self.path = Some(path);
        Ok(())
    }

    pub fn insert_char(&mut self, idx: usize, ch: char) {
        self.push_undo();
        self.text.insert(idx, ch);
        self.redo_stack.clear();
    }

    pub fn remove_char(&mut self, idx: usize) -> Option<char> {
        if idx >= self.text.len() {
            return None;
        }
        self.push_undo();
        let ch = self.text.remove(idx);
        self.redo_stack.clear();
        Some(ch)
    }

    pub fn undo(&mut self) -> bool {
        match self.undo_stack.pop() {
            Some(previous) => {
                self.redo_stack.push(self.text.clone());
                self.text = previous;
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self) -> bool {
        match self.redo_stack.pop() {
            Some(next) => {
                self.undo_stack.push(self.text.clone());
                self.text = next;
                true
            }
            None => false,
        }
    }

    pub fn clear_history(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    fn push_undo(&mut self) {
        self.undo_stack.push(self.text.clone());
    }
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self::new("")
    }
}
