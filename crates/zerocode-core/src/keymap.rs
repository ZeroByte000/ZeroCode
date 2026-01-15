use crate::command::EditorCommand;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    Char(char),
    Ctrl(char),
    Alt(char),
    CtrlLeft,
    CtrlRight,
    Esc,
    Enter,
    Backspace,
    Tab,
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyEvent {
    pub code: KeyCode,
}

impl KeyEvent {
    pub fn char(ch: char) -> Self {
        Self {
            code: KeyCode::Char(ch),
        }
    }

    pub fn ctrl(ch: char) -> Self {
        Self {
            code: KeyCode::Ctrl(ch),
        }
    }

    pub fn alt(ch: char) -> Self {
        Self {
            code: KeyCode::Alt(ch),
        }
    }
}

#[derive(Debug, Default)]
pub struct Keymap {
    map: HashMap<KeyEvent, EditorCommand>,
}

impl Keymap {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    pub fn default_bindings() -> Self {
        let mut keymap = Self::new();
        keymap.bind(KeyEvent::ctrl('p'), EditorCommand::OpenFile);
        keymap.bind(KeyEvent::ctrl('s'), EditorCommand::SaveFile);
        keymap.bind(KeyEvent::ctrl('z'), EditorCommand::Undo);
        keymap.bind(KeyEvent::ctrl('y'), EditorCommand::Redo);
        keymap.bind(KeyEvent::ctrl('f'), EditorCommand::Search);
        keymap.bind(KeyEvent::ctrl('g'), EditorCommand::GoToLine);
        keymap.bind(KeyEvent::alt('p'), EditorCommand::OpenFile);
        keymap.bind(KeyEvent::alt('s'), EditorCommand::SaveFile);
        keymap.bind(KeyEvent::alt('z'), EditorCommand::Undo);
        keymap.bind(KeyEvent::alt('y'), EditorCommand::Redo);
        keymap.bind(KeyEvent::alt('f'), EditorCommand::Search);
        keymap.bind(KeyEvent::alt('g'), EditorCommand::GoToLine);
        keymap.bind(KeyEvent::char('o'), EditorCommand::OpenFile);
        keymap.bind(KeyEvent::char('s'), EditorCommand::SaveFile);
        keymap.bind(KeyEvent::char('u'), EditorCommand::Undo);
        keymap.bind(KeyEvent::char('r'), EditorCommand::Redo);
        keymap.bind(KeyEvent::char('/'), EditorCommand::Search);
        keymap.bind(KeyEvent::char('g'), EditorCommand::GoToLine);
        keymap.bind(KeyEvent::char('q'), EditorCommand::Quit);
        keymap
    }

    pub fn bind(&mut self, key: KeyEvent, command: EditorCommand) {
        self.map.insert(key, command);
    }

    pub fn resolve(&self, key: KeyEvent) -> Option<EditorCommand> {
        self.map.get(&key).copied()
    }
}
