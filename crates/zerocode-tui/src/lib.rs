use anyhow::Result;
use crossterm::event::{self, Event};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{execute, terminal};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Terminal;
use std::io::{self, Stdout};
use std::path::{Path, PathBuf};
use std::collections::HashSet;
use std::time::Duration;
use zerocode_core::buffer::TextBuffer;
use zerocode_core::command::EditorCommand;
use zerocode_core::keymap::{KeyCode, KeyEvent, Keymap};
use zerocode_core::selection::Cursor;

#[derive(Debug)]
struct CommandLine {
    prompt: String,
    input: String,
    kind: CommandKind,
}

#[derive(Debug, Clone, Copy)]
enum CommandKind {
    OpenFile,
    SaveAs,
    GoToLine,
    Search,
    FileSearch,
}

#[derive(Debug)]
enum Mode {
    Normal,
    Insert,
    Command(CommandLine),
}

#[derive(Debug)]
enum Focus {
    Editor,
    Sidebar,
}

#[derive(Debug, Clone)]
struct FileEntry {
    name: String,
    path: PathBuf,
    is_dir: bool,
    depth: usize,
    expanded: bool,
}

enum CommandAction {
    None,
    Cancel,
    Execute { kind: CommandKind, input: String },
}

#[derive(Debug)]
pub struct EditorView {
    pub buffer: TextBuffer,
    keymap: Keymap,
    running: bool,
    cursor: Cursor,
    mode: Mode,
    status: String,
    last_search: Option<String>,
    focus: Focus,
    file_entries: Vec<FileEntry>,
    view_entries: Vec<FileEntry>,
    file_selected: usize,
    cwd: PathBuf,
    show_sidebar: bool,
    expanded_dirs: HashSet<PathBuf>,
    file_query: Option<String>,
}

impl EditorView {
    pub fn new() -> Self {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let expanded_dirs = HashSet::new();
        let file_entries = build_tree_entries(&cwd, &expanded_dirs);
        let view_entries = file_entries.clone();
        Self {
            buffer: TextBuffer::default(),
            keymap: Keymap::default_bindings(),
            running: true,
            cursor: Cursor::default(),
            mode: Mode::Normal,
            status: String::new(),
            last_search: None,
            focus: Focus::Editor,
            file_entries,
            view_entries,
            file_selected: 0,
            cwd,
            show_sidebar: true,
            expanded_dirs,
            file_query: None,
        }
    }

    pub fn set_status(&mut self, status: impl Into<String>) {
        self.status = status.into();
    }

    pub fn run(&mut self) -> Result<()> {
        let (mut terminal, _guard) = setup_terminal()?;
        while self.running {
            terminal.draw(|frame| {
                let size = frame.size();
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Min(1), Constraint::Length(1)])
                    .split(size);

                let editor_text = render_editor_text(&self.buffer);
                let columns = if self.show_sidebar {
                    Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([Constraint::Length(30), Constraint::Min(1)])
                        .split(chunks[0])
                } else {
                    Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([Constraint::Min(1)])
                        .split(chunks[0])
                };

                if self.show_sidebar {
                    let sidebar_title = if let Some(query) = self.file_query.as_ref() {
                        if query.is_empty() {
                            format!("Files: {}", self.cwd.display())
                        } else {
                            format!("Files: {} (filter: {})", self.cwd.display(), query)
                        }
                    } else {
                        format!("Files: {}", self.cwd.display())
                    };
                    let sidebar_text = render_sidebar(
                        &self.view_entries,
                        self.file_selected,
                        self.buffer.path.as_ref(),
                    );
                    let sidebar = Paragraph::new(sidebar_text)
                        .block(Block::default().title(sidebar_title).borders(Borders::ALL))
                        .wrap(Wrap { trim: false });
                    frame.render_widget(sidebar, columns[0]);
                }

                let editor_index = if self.show_sidebar { 1 } else { 0 };
                let editor = Paragraph::new(editor_text)
                    .block(Block::default().title("ZeroCode").borders(Borders::ALL))
                    .wrap(Wrap { trim: false });
                frame.render_widget(editor, columns[editor_index]);

                let status_line = match &self.mode {
                    Mode::Command(cmd) => format!("{} {}", cmd.prompt, cmd.input),
                    Mode::Insert => self.status_line("INSERT"),
                    Mode::Normal => self.status_line("NORMAL"),
                };

                let status = Paragraph::new(Line::from(status_line))
                    .style(Style::default().fg(Color::DarkGray))
                    .block(Block::default().borders(Borders::TOP));
                frame.render_widget(status, chunks[1]);

                if matches!(self.mode, Mode::Insert | Mode::Normal)
                    && matches!(self.focus, Focus::Editor)
                {
                    let (x, y) = cursor_screen_position(columns[editor_index], self.cursor);
                    frame.set_cursor(x, y);
                }
            })?;

            if event::poll(Duration::from_millis(16))? {
                if let Event::Key(key) = event::read()? {
                    if let Some(core_key) = translate_key_event(key) {
                        self.handle_key(core_key)?;
                    }
                }
            }
        }

        Ok(())
    }

    fn status_line(&self, mode: &str) -> String {
        let path = self
            .buffer
            .path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "[no file]".to_string());
        let help = " ^G Goto  ^F Find  ^S Save  ^P Open  ^Z Undo  ^Y Redo  ^Q Quit  ^Left Files  ^B Toggle  ^L File  ^E Expand  ^W Collapse  Alt: Alt+P/S/F/G/Z/Y or o s / g f b";
        if self.status.is_empty() {
            format!("{} | {} |{}", mode, path, help)
        } else {
            format!("{} | {} | {} |{}", mode, path, self.status, help)
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        let current_mode = std::mem::replace(&mut self.mode, Mode::Normal);
        match current_mode {
            Mode::Command(mut cmd) => {
                let action = Self::handle_command_input(&mut cmd, key);
                match action {
                    CommandAction::None => {
                        if matches!(cmd.kind, CommandKind::FileSearch) {
                            self.file_query = Some(cmd.input.clone());
                            self.refresh_view_entries();
                        }
                        self.mode = Mode::Command(cmd);
                    }
                    CommandAction::Cancel => {
                        if matches!(cmd.kind, CommandKind::FileSearch) {
                            self.file_query = None;
                            self.refresh_view_entries();
                        }
                        self.mode = Mode::Normal;
                    }
                    CommandAction::Execute { kind, input } => {
                        if matches!(kind, CommandKind::FileSearch) {
                            self.file_query = Some(input.clone());
                            self.refresh_view_entries();
                        }
                        self.mode = Mode::Normal;
                        self.execute_command(kind, input)?;
                    }
                }
            }
            Mode::Insert => {
                self.mode = Mode::Insert;
                self.handle_insert(key)?;
            }
            Mode::Normal => {
                self.mode = Mode::Normal;
                self.handle_normal(key)?;
            }
        }
        Ok(())
    }

    fn handle_normal(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Ctrl('b') => {
                self.show_sidebar = !self.show_sidebar;
                if !self.show_sidebar {
                    self.focus = Focus::Editor;
                }
                return Ok(());
            }
            KeyCode::Char('b') => {
                self.show_sidebar = !self.show_sidebar;
                if !self.show_sidebar {
                    self.focus = Focus::Editor;
                }
                return Ok(());
            }
            KeyCode::Ctrl('l') => {
                self.focus = Focus::Sidebar;
                self.prompt_command(CommandKind::FileSearch, "Find file:");
                return Ok(());
            }
            KeyCode::Char('f') => {
                self.focus = Focus::Sidebar;
                self.prompt_command(CommandKind::FileSearch, "Find file:");
                return Ok(());
            }
            KeyCode::Ctrl('e') => {
                self.expanded_dirs = collect_dirs(&self.cwd);
                self.refresh_tree();
                return Ok(());
            }
            KeyCode::Char('e') => {
                self.expanded_dirs = collect_dirs(&self.cwd);
                self.refresh_tree();
                return Ok(());
            }
            KeyCode::Ctrl('w') => {
                self.expanded_dirs.clear();
                self.refresh_tree();
                return Ok(());
            }
            KeyCode::Char('w') => {
                self.expanded_dirs.clear();
                self.refresh_tree();
                return Ok(());
            }
            KeyCode::CtrlLeft => {
                if self.show_sidebar {
                    self.focus = Focus::Sidebar;
                }
                return Ok(());
            }
            KeyCode::Tab => {
                if self.show_sidebar {
                    self.focus = Focus::Sidebar;
                }
                return Ok(());
            }
            KeyCode::CtrlRight | KeyCode::Esc => {
                self.focus = Focus::Editor;
                return Ok(());
            }
            _ => {}
        }

        if matches!(self.focus, Focus::Sidebar) {
            return self.handle_sidebar_key(key);
        }

        if let Some(command) = self.keymap.resolve(key) {
            return self.dispatch(command);
        }

        match key.code {
            KeyCode::Char('i') => self.mode = Mode::Insert,
            KeyCode::Char('h') | KeyCode::Left => self.move_left(),
            KeyCode::Char('j') | KeyCode::Down => self.move_down(),
            KeyCode::Char('k') | KeyCode::Up => self.move_up(),
            KeyCode::Char('l') | KeyCode::Right => self.move_right(),
            _ => {}
        }
        Ok(())
    }

    fn handle_insert(&mut self, key: KeyEvent) -> Result<()> {
        if matches!(self.focus, Focus::Sidebar) {
            return self.handle_sidebar_key(key);
        }

        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Enter => self.insert_char('\n'),
            KeyCode::Backspace => self.backspace(),
            KeyCode::Left => self.move_left(),
            KeyCode::Right => self.move_right(),
            KeyCode::Up => self.move_up(),
            KeyCode::Down => self.move_down(),
            KeyCode::Char(ch) => self.insert_char(ch),
            _ => {}
        }
        Ok(())
    }

    fn handle_command_input(cmd: &mut CommandLine, key: KeyEvent) -> CommandAction {
        match key.code {
            KeyCode::Esc => CommandAction::Cancel,
            KeyCode::Enter => CommandAction::Execute {
                kind: cmd.kind,
                input: cmd.input.trim().to_string(),
            },
            KeyCode::Backspace => {
                cmd.input.pop();
                CommandAction::None
            }
            KeyCode::Char(ch) => {
                cmd.input.push(ch);
                CommandAction::None
            }
            _ => CommandAction::None,
        }
    }

    fn dispatch(&mut self, command: EditorCommand) -> Result<()> {
        match command {
            EditorCommand::Quit => self.running = false,
            EditorCommand::OpenFile => self.prompt_command(CommandKind::OpenFile, "Open:"),
            EditorCommand::SaveFile => {
                if self.buffer.path.is_some() {
                    match self.buffer.save() {
                        Ok(()) => self.status = "saved".to_string(),
                        Err(err) => self.status = format!("save failed: {err}"),
                    }
                } else {
                    self.prompt_command(CommandKind::SaveAs, "Save as:");
                }
            }
            EditorCommand::Undo => {
                if !self.buffer.undo() {
                    self.status = "nothing to undo".to_string();
                }
            }
            EditorCommand::Redo => {
                if !self.buffer.redo() {
                    self.status = "nothing to redo".to_string();
                }
            }
            EditorCommand::Search => self.prompt_command(CommandKind::Search, "Search:"),
            EditorCommand::GoToLine => self.prompt_command(CommandKind::GoToLine, "Go to line:"),
        }
        self.clamp_cursor();
        Ok(())
    }

    fn prompt_command(&mut self, kind: CommandKind, prompt: &str) {
        self.mode = Mode::Command(CommandLine {
            prompt: prompt.to_string(),
            input: String::new(),
            kind,
        });
    }

    fn execute_command(&mut self, kind: CommandKind, input: String) -> Result<()> {
        self.status.clear();
        match kind {
            CommandKind::OpenFile => {
                let buffer = TextBuffer::open(&input);
                match buffer {
                    Ok(mut buffer) => {
                        buffer.clear_history();
                        self.buffer = buffer;
                        self.cursor = Cursor::default();
                        self.status = "opened".to_string();
                    }
                    Err(err) => self.status = format!("open failed: {err}"),
                }
            }
            CommandKind::SaveAs => match self.buffer.save_as(&input) {
                Ok(()) => self.status = "saved".to_string(),
                Err(err) => self.status = format!("save failed: {err}"),
            },
            CommandKind::GoToLine => {
                if let Ok(line) = input.parse::<usize>() {
                    if line > 0 {
                        self.cursor.line = line - 1;
                        self.cursor.column = 0;
                        self.clamp_cursor();
                    }
                }
            }
            CommandKind::Search => {
                if input.is_empty() {
                    return Ok(());
                }
                self.last_search = Some(input.clone());
                if let Some(idx) = self.buffer.text.find(&input) {
                    self.cursor = cursor_from_index(&self.buffer.text, idx);
                } else {
                    self.status = "not found".to_string();
                }
            }
            CommandKind::FileSearch => {
                if input.is_empty() {
                    return Ok(());
                }
                let needle = input.to_lowercase();
                if let Some((idx, _)) = self
                    .file_entries
                    .iter()
                    .enumerate()
                    .find(|(_, entry)| entry.name.to_lowercase().contains(&needle))
                {
                    self.file_selected = idx;
                    self.focus = Focus::Sidebar;
                } else {
                    self.status = "file not found".to_string();
                }
            }
        }
        Ok(())
    }

    fn insert_char(&mut self, ch: char) {
        let idx = byte_index_from_cursor(&self.buffer.text, self.cursor);
        self.buffer.insert_char(idx, ch);
        if ch == '\n' {
            self.cursor.line += 1;
            self.cursor.column = 0;
        } else {
            self.cursor.column += 1;
        }
    }

    fn backspace(&mut self) {
        let idx = byte_index_from_cursor(&self.buffer.text, self.cursor);
        if idx == 0 {
            return;
        }
        let prev_idx = prev_char_boundary(&self.buffer.text, idx);
        if self.buffer.remove_char(prev_idx).is_some() {
            if self.cursor.column > 0 {
                self.cursor.column -= 1;
            } else if self.cursor.line > 0 {
                self.cursor.line -= 1;
                self.cursor.column = line_length(&self.buffer.text, self.cursor.line);
            }
        }
    }

    fn move_left(&mut self) {
        if self.cursor.column > 0 {
            self.cursor.column -= 1;
        } else if self.cursor.line > 0 {
            self.cursor.line -= 1;
            self.cursor.column = line_length(&self.buffer.text, self.cursor.line);
        }
    }

    fn move_right(&mut self) {
        let line_len = line_length(&self.buffer.text, self.cursor.line);
        if self.cursor.column < line_len {
            self.cursor.column += 1;
        } else if self.cursor.line + 1 < line_count(&self.buffer.text) {
            self.cursor.line += 1;
            self.cursor.column = 0;
        }
    }

    fn move_up(&mut self) {
        if self.cursor.line > 0 {
            self.cursor.line -= 1;
            self.cursor.column = self.cursor.column.min(line_length(&self.buffer.text, self.cursor.line));
        }
    }

    fn move_down(&mut self) {
        if self.cursor.line + 1 < line_count(&self.buffer.text) {
            self.cursor.line += 1;
            self.cursor.column = self.cursor.column.min(line_length(&self.buffer.text, self.cursor.line));
        }
    }

    fn clamp_cursor(&mut self) {
        let max_line = line_count(&self.buffer.text).saturating_sub(1);
        self.cursor.line = self.cursor.line.min(max_line);
        let max_col = line_length(&self.buffer.text, self.cursor.line);
        self.cursor.column = self.cursor.column.min(max_col);
    }

    fn handle_sidebar_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Ctrl('l') => {
                self.prompt_command(CommandKind::FileSearch, "Find file:");
            }
            KeyCode::Ctrl('e') => {
                self.expanded_dirs = collect_dirs(&self.cwd);
                self.refresh_tree();
            }
            KeyCode::Ctrl('w') => {
                self.expanded_dirs.clear();
                self.refresh_tree();
            }
            KeyCode::Up => {
                if self.file_selected > 0 {
                    self.file_selected -= 1;
                }
            }
            KeyCode::Down => {
                if self.file_selected + 1 < self.view_entries.len() {
                    self.file_selected += 1;
                }
            }
            KeyCode::Enter => {
                if let Some(entry) = self.view_entries.get(self.file_selected).cloned() {
                    if entry.is_dir {
                        if entry.name == ".." {
                            self.cwd = entry.path.clone();
                            self.expanded_dirs.clear();
                            self.refresh_tree();
                        } else {
                            if self.expanded_dirs.contains(&entry.path) {
                                self.expanded_dirs.remove(&entry.path);
                            } else {
                                self.expanded_dirs.insert(entry.path.clone());
                            }
                            self.refresh_tree();
                        }
                    } else {
                        match TextBuffer::open(&entry.path) {
                            Ok(buffer) => {
                                self.buffer = buffer;
                                self.cursor = Cursor::default();
                                self.focus = Focus::Editor;
                                self.status = "opened".to_string();
                            }
                            Err(err) => self.status = format!("open failed: {err}"),
                        }
                    }
                }
            }
            KeyCode::Esc | KeyCode::CtrlRight => {
                self.focus = Focus::Editor;
            }
            _ => {}
        }
        Ok(())
    }

    fn refresh_tree(&mut self) {
        self.file_entries = build_tree_entries(&self.cwd, &self.expanded_dirs);
        self.refresh_view_entries();
    }

    fn refresh_view_entries(&mut self) {
        if let Some(query) = self.file_query.as_ref() {
            if query.is_empty() {
                self.view_entries = self.file_entries.clone();
            } else {
                self.view_entries = build_tree_entries_filtered(&self.cwd, query);
            }
        } else {
            self.view_entries = self.file_entries.clone();
        }

        if self.file_selected >= self.view_entries.len() {
            self.file_selected = self.view_entries.len().saturating_sub(1);
        }
    }
}

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let mut stdout = io::stdout();
        let _ = execute!(
            stdout,
            LeaveAlternateScreen,
            terminal::EnableLineWrap,
            crossterm::cursor::Show
        );
    }
}

fn setup_terminal() -> Result<(Terminal<CrosstermBackend<Stdout>>, TerminalGuard)> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, terminal::DisableLineWrap)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok((terminal, TerminalGuard))
}

fn translate_key_event(key: event::KeyEvent) -> Option<KeyEvent> {
    use crossterm::event::KeyModifiers;
    let code = match key.code {
        crossterm::event::KeyCode::Left if key.modifiers.contains(KeyModifiers::CONTROL) => {
            KeyCode::CtrlLeft
        }
        crossterm::event::KeyCode::Right if key.modifiers.contains(KeyModifiers::CONTROL) => {
            KeyCode::CtrlRight
        }
        crossterm::event::KeyCode::Char(ch) if key.modifiers.contains(KeyModifiers::ALT) => {
            KeyCode::Alt(ch)
        }
        crossterm::event::KeyCode::Char(ch) if key.modifiers.contains(KeyModifiers::CONTROL) => {
            KeyCode::Ctrl(ch)
        }
        crossterm::event::KeyCode::Char(ch) => KeyCode::Char(ch),
        crossterm::event::KeyCode::Esc => KeyCode::Esc,
        crossterm::event::KeyCode::Enter => KeyCode::Enter,
        crossterm::event::KeyCode::Backspace => KeyCode::Backspace,
        crossterm::event::KeyCode::Tab => KeyCode::Tab,
        crossterm::event::KeyCode::Up => KeyCode::Up,
        crossterm::event::KeyCode::Down => KeyCode::Down,
        crossterm::event::KeyCode::Left => KeyCode::Left,
        crossterm::event::KeyCode::Right => KeyCode::Right,
        _ => return None,
    };
    Some(KeyEvent { code })
}

fn line_count(text: &str) -> usize {
    text.lines().count().max(1)
}

fn line_length(text: &str, line: usize) -> usize {
    text.lines().nth(line).map(|l| l.chars().count()).unwrap_or(0)
}

fn byte_index_from_cursor(text: &str, cursor: Cursor) -> usize {
    let mut current_line = 0;
    let mut index = 0;
    for line in text.split_inclusive('\n') {
        if current_line == cursor.line {
            let mut col = 0usize;
            for (byte_idx, ch) in line.char_indices() {
                if ch == '\n' {
                    return index + byte_idx.min(line.len());
                }
                if col == cursor.column {
                    return index + byte_idx;
                }
                col += 1;
            }
            return index + line.len();
        }
        index += line.len();
        current_line += 1;
    }
    index
}

fn prev_char_boundary(text: &str, idx: usize) -> usize {
    let mut prev = 0;
    for (i, _) in text.char_indices() {
        if i >= idx {
            break;
        }
        prev = i;
    }
    prev
}

fn cursor_from_index(text: &str, idx: usize) -> Cursor {
    let mut remaining = idx;
    let mut line = 0;
    for line_text in text.split_inclusive('\n') {
        if remaining <= line_text.len() {
            let mut col = 0usize;
            for (byte_idx, ch) in line_text.char_indices() {
                if byte_idx >= remaining {
                    break;
                }
                if ch != '\n' {
                    col += 1;
                }
            }
            return Cursor { line, column: col };
        }
        remaining = remaining.saturating_sub(line_text.len());
        line += 1;
    }
    Cursor { line, column: 0 }
}

fn cursor_screen_position(area: ratatui::layout::Rect, cursor: Cursor) -> (u16, u16) {
    let x = area.x + 1 + cursor.column as u16;
    let y = area.y + 1 + cursor.line as u16;
    let max_x = area.x + area.width.saturating_sub(1);
    let max_y = area.y + area.height.saturating_sub(1);
    (x.min(max_x), y.min(max_y))
}

fn startup_help() -> String {
    let lines = [
        "Selamat datang di ZeroCode!",
        "",
        "Perintah utama:",
        "  q           Keluar",
        "  i           Masuk Insert mode",
        "  Esc         Kembali ke Normal mode",
        "  Ctrl+P      Buka file (input path)",
        "  Ctrl+S      Simpan file",
        "  Ctrl+Z      Undo",
        "  Ctrl+Y      Redo",
        "  Ctrl+F      Search",
        "  Ctrl+G      Go to line",
        "  Ctrl+B      Toggle panel kiri",
        "  Ctrl+L      Cari file di panel kiri",
        "  Ctrl+E      Expand semua folder",
        "  Ctrl+W      Collapse semua folder",
        "  Alternatif jika Ctrl diblok: Alt+P/S/F/G/Z/Y atau o open, s save, / search, g goto, f files, b toggle",
        "",
        "Navigasi (Normal mode):",
        "  Panah / h j k l",
        "",
        "Panel file:",
        "  Ctrl+Left fokus, Tab fokus, Enter buka/expand, Esc kembali",
        "  Item '..' untuk naik satu level folder",
    ];
    lines.join("\n")
}

fn render_editor_text(buffer: &TextBuffer) -> Text<'static> {
    if buffer.text.is_empty() {
        return Text::from(startup_help());
    }

    let ext = buffer
        .path
        .as_ref()
        .and_then(|path| path.extension())
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_lowercase();

    let lang = match ext.as_str() {
        "rs" => Language::Rust,
        "js" | "jsx" | "ts" | "tsx" => Language::JavaScript,
        "json" => Language::Json,
        "py" => Language::Python,
        "go" => Language::Go,
        _ => Language::Plain,
    };

    highlight_text(&buffer.text, lang)
}

#[derive(Clone, Copy)]
enum Language {
    Plain,
    Rust,
    JavaScript,
    Json,
    Python,
    Go,
}

fn highlight_text(text: &str, lang: Language) -> Text<'static> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut state = HighlightState { in_string: None };
    for line in text.lines() {
        let spans = highlight_line(line, lang, &mut state);
        lines.push(Line::from(spans));
    }
    if text.ends_with('\n') {
        lines.push(Line::from(""));
    }
    Text::from(lines)
}

struct HighlightState {
    in_string: Option<char>,
}

fn highlight_line(line: &str, lang: Language, state: &mut HighlightState) -> Vec<Span<'static>> {
    let (keywords, comment_prefix, string_delims) = match lang {
        Language::Rust => (rust_keywords(), Some("//"), &['"', '\''][..]),
        Language::JavaScript => (js_keywords(), Some("//"), &['"', '\'', '`'][..]),
        Language::Json => (&[][..], None, &['"'][..]),
        Language::Python => (py_keywords(), Some("#"), &['"', '\''][..]),
        Language::Go => (go_keywords(), Some("//"), &['"', '\''][..]),
        Language::Plain => (&[][..], None, &[][..]),
    };

    let mut spans = Vec::new();
    let mut current = String::new();
    let mut string_buf = String::new();
    let mut in_string = state.in_string;
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        if let Some(delim) = in_string {
            string_buf.push(ch);
            if ch == delim {
                spans.push(Span::styled(
                    string_buf.clone(),
                    Style::default().fg(Color::Yellow),
                ));
                string_buf.clear();
                in_string = None;
            }
            continue;
        }

        if let Some(prefix) = comment_prefix {
            if ch == prefix.chars().next().unwrap_or('/') && comment_matches(prefix, &chars) {
                let consume = prefix.chars().count().saturating_sub(1);
                for _ in 0..consume {
                    let _ = chars.next();
                }
                flush_token(&mut spans, &mut current, keywords);
                let remaining: String = chars.collect();
                let mut rest = String::new();
                rest.push_str(prefix);
                rest.push_str(&remaining);
                spans.push(Span::styled(rest, Style::default().fg(Color::Green)));
                return spans;
            }
        }

        if string_delims.contains(&ch) {
            flush_token(&mut spans, &mut current, keywords);
            string_buf.push(ch);
            in_string = Some(ch);
            continue;
        }

        let is_word = ch.is_alphanumeric() || ch == '_';
        if is_word {
            current.push(ch);
        } else {
            flush_token(&mut spans, &mut current, keywords);
            if is_operator(ch) {
                spans.push(Span::styled(ch.to_string(), Style::default().fg(Color::Cyan)));
            } else {
                spans.push(Span::raw(ch.to_string()));
            }
        }
    }

    if !current.is_empty() {
        flush_token(&mut spans, &mut current, keywords);
    }

    if in_string.is_some() && !string_buf.is_empty() {
        spans.push(Span::styled(
            string_buf.clone(),
            Style::default().fg(Color::Yellow),
        ));
    }

    state.in_string = in_string;
    spans
}

fn flush_token(spans: &mut Vec<Span<'static>>, token: &mut String, keywords: &[&str]) {
    if token.is_empty() {
        return;
    }
    let style = if is_number(token) {
        Style::default().fg(Color::Magenta)
    } else if keywords.iter().any(|kw| kw == token) {
        Style::default().fg(Color::Blue)
    } else {
        Style::default()
    };
    spans.push(Span::styled(token.clone(), style));
    token.clear();
}

fn is_number(token: &str) -> bool {
    token.chars().all(|ch| ch.is_ascii_digit())
}

fn is_operator(ch: char) -> bool {
    matches!(ch, '+' | '-' | '*' | '/' | '=' | '<' | '>' | '!' | '&' | '|' | '%' | '^' | ':' | '?' )
}

fn comment_matches(prefix: &str, chars: &std::iter::Peekable<std::str::Chars<'_>>) -> bool {
    let mut lookahead = chars.clone();
    for expected in prefix.chars().skip(1) {
        if let Some(next) = lookahead.next() {
            if next != expected {
                return false;
            }
        } else {
            return false;
        }
    }
    true
}

fn rust_keywords() -> &'static [&'static str] {
    &[
        "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
        "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move",
        "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait",
        "true", "type", "unsafe", "use", "where", "while",
    ]
}

fn js_keywords() -> &'static [&'static str] {
    &[
        "break", "case", "catch", "class", "const", "continue", "debugger", "default", "delete",
        "do", "else", "export", "extends", "false", "finally", "for", "function", "if", "import",
        "in", "instanceof", "let", "new", "null", "return", "super", "switch", "this", "throw",
        "true", "try", "typeof", "var", "void", "while", "yield",
    ]
}

fn py_keywords() -> &'static [&'static str] {
    &[
        "and", "as", "assert", "break", "class", "continue", "def", "del", "elif", "else", "except",
        "False", "finally", "for", "from", "global", "if", "import", "in", "is", "lambda", "None",
        "nonlocal", "not", "or", "pass", "raise", "return", "True", "try", "while", "with", "yield",
    ]
}

fn go_keywords() -> &'static [&'static str] {
    &[
        "break", "case", "chan", "const", "continue", "default", "defer", "else", "fallthrough",
        "for", "func", "go", "goto", "if", "import", "interface", "map", "package", "range",
        "return", "select", "struct", "switch", "type", "var",
    ]
}

fn build_tree_entries(root: &Path, expanded_dirs: &HashSet<PathBuf>) -> Vec<FileEntry> {
    let mut entries = Vec::new();
    if let Some(parent) = root.parent() {
        entries.push(FileEntry {
            name: "..".to_string(),
            path: parent.to_path_buf(),
            is_dir: true,
            depth: 0,
            expanded: false,
        });
    }
    build_tree_recursive(root, expanded_dirs, 0, &mut entries);
    entries
}

fn build_tree_entries_filtered(root: &Path, query: &str) -> Vec<FileEntry> {
    let mut entries = Vec::new();
    if let Some(parent) = root.parent() {
        entries.push(FileEntry {
            name: "..".to_string(),
            path: parent.to_path_buf(),
            is_dir: true,
            depth: 0,
            expanded: false,
        });
    }
    let needle = query.to_lowercase();
    build_tree_recursive_filtered(root, &needle, 0, &mut entries);
    entries
}

fn collect_dirs(root: &Path) -> HashSet<PathBuf> {
    let mut dirs = HashSet::new();
    collect_dirs_recursive(root, &mut dirs);
    dirs
}

fn collect_dirs_recursive(path: &Path, dirs: &mut HashSet<PathBuf>) {
    if let Ok(read_dir) = std::fs::read_dir(path) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if is_dir {
                dirs.insert(path.clone());
                collect_dirs_recursive(&path, dirs);
            }
        }
    }
}

fn build_tree_recursive(
    path: &Path,
    expanded_dirs: &HashSet<PathBuf>,
    depth: usize,
    entries: &mut Vec<FileEntry>,
) {
    let mut children = Vec::new();
    if let Ok(read_dir) = std::fs::read_dir(path) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let expanded = is_dir && expanded_dirs.contains(&path);
            children.push(FileEntry {
                name,
                path,
                is_dir,
                depth,
                expanded,
            });
        }
    }

    children.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    for child in children {
        let path = child.path.clone();
        let is_dir = child.is_dir;
        let expanded = child.expanded;
        entries.push(child);
        if is_dir && expanded {
            build_tree_recursive(&path, expanded_dirs, depth + 1, entries);
        }
    }
}

fn build_tree_recursive_filtered(
    path: &Path,
    query: &str,
    depth: usize,
    entries: &mut Vec<FileEntry>,
) -> bool {
    let mut children = Vec::new();
    if let Ok(read_dir) = std::fs::read_dir(path) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            children.push(FileEntry {
                name,
                path,
                is_dir,
                depth,
                expanded: false,
            });
        }
    }

    children.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    let mut any_match = false;
    for child in children {
        let name_match = child.name.to_lowercase().contains(query);
        if child.is_dir {
            let mut temp = Vec::new();
            let child_match = build_tree_recursive_filtered(&child.path, query, depth + 1, &mut temp);
            if name_match || child_match {
                entries.push(FileEntry {
                    expanded: child_match,
                    ..child.clone()
                });
                entries.extend(temp);
                any_match = true;
            }
        } else if name_match {
            entries.push(child);
            any_match = true;
        }
    }
    any_match
}

fn render_sidebar(
    entries: &[FileEntry],
    selected: usize,
    active_path: Option<&PathBuf>,
) -> Text<'static> {
    if entries.is_empty() {
        return Text::from("No files");
    }
    let mut lines = Vec::new();
    for (idx, entry) in entries.iter().enumerate() {
        let marker = if idx == selected { ">" } else { " " };
        let suffix = if entry.is_dir { "/" } else { "" };
        let indent = "  ".repeat(entry.depth);
        let icon = if entry.name == ".." {
            "[..]"
        } else if entry.is_dir {
            if entry.expanded { "[-]" } else { "[+]" }
        } else {
            file_icon(&entry.path)
        };
        let text = format!("{} {}{} {}{}", marker, indent, icon, entry.name, suffix);

        let mut style = Style::default();
        if Some(&entry.path) == active_path {
            style = style.fg(Color::Green);
        }
        if idx == selected {
            style = style.add_modifier(Modifier::BOLD).bg(Color::DarkGray);
        }

        lines.push(Line::from(Span::styled(text, style)));
    }
    Text::from(lines)
}

fn file_icon(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "rs" => "[R]",
        "js" | "jsx" => "[JS]",
        "ts" | "tsx" => "[TS]",
        "json" => "[JSON]",
        "py" => "[PY]",
        "go" => "[GO]",
        "md" => "[MD]",
        "toml" => "[TOML]",
        "yaml" | "yml" => "[YML]",
        "c" => "[C]",
        "cc" | "cpp" | "cxx" => "[CPP]",
        "h" | "hpp" => "[H]",
        "sh" => "[SH]",
        "txt" => "[TXT]",
        _ => "[F]",
    }
}
