use std::io;
use std::env;
use std::fs;
use std::path::PathBuf;
use ratatui::{
    layout::Position,
    style::{Color, Style, Stylize},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Paragraph},
    DefaultTerminal, Frame,
};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

#[derive(Debug, Default, PartialEq)]
enum Mode {
    #[default]
    Normal,
    SaveAs,
    Open,
}

#[derive(Debug, Default)]
pub struct App {
    lines: Vec<String>,
    row: usize,
    col: usize,
    scroll: usize,
    path: Option<PathBuf>,
    modified: bool,
    exit: bool,
    mode: Mode,
    input: String,
    status: Option<String>,
}

impl App {
    fn new(path: Option<PathBuf>) -> Self {
        let mut app = Self { lines: vec![String::new()], ..Default::default() };
        if let Some(p) = path {
            app.open(p);
            app.status = None;
        }
        app
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        while !self.exit {
            terminal.draw(|frame| self.draw(frame))?;
            self.handle_events()?;
        }
        Ok(())
    }

    fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let name = self.path.as_ref().map_or("[No Name]".to_string(), |p| p.display().to_string());
        let title = format!(" {}{} ", name, if self.modified { " *" } else { "" });
        let bottom = match self.mode {
            Mode::SaveAs => format!(" Save as: {}\u{2588}  (Enter confirm, Esc cancel) ", self.input),
            Mode::Open => format!(" Open: {}\u{2588}  (Enter confirm, Esc cancel) ", self.input),
            Mode::Normal => match &self.status {
                Some(s) => format!(" {s} "),
                None => " ^S save  ^O open  Esc quit ".to_string(),
            },
        };
        let block = Block::bordered()
            .title(Line::from(title.bold()).centered())
            .title_bottom(Line::from(bottom.italic().dim()).centered())
            .border_set(border::THICK);

        let inner_h = area.height.saturating_sub(2) as usize;
        if self.row < self.scroll {
            self.scroll = self.row;
        } else if inner_h > 0 && self.row >= self.scroll + inner_h {
            self.scroll = self.row + 1 - inner_h;
        }

        let text: Vec<Line> = self.lines.iter().map(|l| highlight_line(l)).collect();
        let para = Paragraph::new(text).block(block).scroll((self.scroll as u16, 0));
        frame.render_widget(para, area);

        let cursor_x = area.x + 1 + self.col as u16;
        let cursor_y = area.y + 1 + (self.row - self.scroll) as u16;
        frame.set_cursor_position(Position::new(cursor_x, cursor_y));
    }

    fn handle_events(&mut self) -> io::Result<()> {
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                self.handle_key_event(key);
            }
        }
        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent) {
        match self.mode {
            Mode::SaveAs | Mode::Open => self.handle_input_key(key),
            Mode::Normal => self.handle_normal_key(key),
        }
    }

    fn handle_input_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Enter => {
                let input = std::mem::take(&mut self.input);
                if !input.is_empty() {
                    match self.mode {
                        Mode::SaveAs => {
                            self.path = Some(PathBuf::from(input));
                            self.save();
                        }
                        Mode::Open => self.open(PathBuf::from(input)),
                        Mode::Normal => {}
                    }
                }
                self.mode = Mode::Normal;
            }
            KeyCode::Char(c) => self.input.push(c),
            KeyCode::Backspace => {
                self.input.pop();
            }
            _ => {}
        }
    }

    fn handle_normal_key(&mut self, key: KeyEvent) {
        self.status = None;
        match key.code {
            KeyCode::Esc => self.exit = true,
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.path.is_some() {
                    self.save();
                } else {
                    self.input.clear();
                    self.mode = Mode::SaveAs;
                }
            }
            KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.clear();
                self.mode = Mode::Open;
            }
            KeyCode::Char(c) => {
                self.lines[self.row].insert(self.col, c);
                self.col += 1;
                self.modified = true;
            }
            KeyCode::Enter => {
                let rest = self.lines[self.row].split_off(self.col);
                self.lines.insert(self.row + 1, rest);
                self.row += 1;
                self.col = 0;
                self.modified = true;
            }
            KeyCode::Backspace => {
                if self.col > 0 {
                    self.col -= 1;
                    self.lines[self.row].remove(self.col);
                } else if self.row > 0 {
                    let cur = self.lines.remove(self.row);
                    self.row -= 1;
                    self.col = self.lines[self.row].len();
                    self.lines[self.row].push_str(&cur);
                }
                self.modified = true;
            }
            KeyCode::Delete => {
                if self.col < self.lines[self.row].len() {
                    self.lines[self.row].remove(self.col);
                    self.modified = true;
                } else if self.row + 1 < self.lines.len() {
                    let next = self.lines.remove(self.row + 1);
                    self.lines[self.row].push_str(&next);
                    self.modified = true;
                }
            }
            KeyCode::Left => {
                if self.col > 0 {
                    self.col -= 1;
                } else if self.row > 0 {
                    self.row -= 1;
                    self.col = self.lines[self.row].len();
                }
            }
            KeyCode::Right => {
                if self.col < self.lines[self.row].len() {
                    self.col += 1;
                } else if self.row + 1 < self.lines.len() {
                    self.row += 1;
                    self.col = 0;
                }
            }
            KeyCode::Up if self.row > 0 => {
                self.row -= 1;
                self.col = self.col.min(self.lines[self.row].len());
            }
            KeyCode::Down if self.row + 1 < self.lines.len() => {
                self.row += 1;
                self.col = self.col.min(self.lines[self.row].len());
            }
            KeyCode::Home => self.col = 0,
            KeyCode::End => self.col = self.lines[self.row].len(),
            _ => {}
        }
    }

    fn save(&mut self) {
        if let Some(p) = &self.path {
            match fs::write(p, self.lines.join("\n")) {
                Ok(()) => {
                    self.modified = false;
                    self.status = Some("Saved".to_string());
                }
                Err(e) => self.status = Some(format!("Save failed: {e}")),
            }
        }
    }

    fn open(&mut self, path: PathBuf) {
        match fs::read_to_string(&path) {
            Ok(s) => {
                self.lines = s.lines().map(str::to_string).collect();
                if self.lines.is_empty() {
                    self.lines.push(String::new());
                }
                self.row = 0;
                self.col = 0;
                self.scroll = 0;
                self.modified = false;
                self.status = Some("Opened".to_string());
                self.path = Some(path);
            }
            Err(e) => self.status = Some(format!("Open failed: {e}")),
        }
    }
}

/// Very small markdown highlighter: headings get a bold colored line,
/// and `**bold**`, `*italic*`, and `` `code` `` toggle inline styles.
fn highlight_line(line: &str) -> Line<'static> {
    let hashes = line.chars().take_while(|&c| c == '#').count();
    if (1..=6).contains(&hashes) && line.as_bytes().get(hashes) == Some(&b' ') {
        return Line::from(Span::styled(line.to_string(), Style::new().bold().fg(Color::Cyan)));
    }

    let mut spans = Vec::new();
    let mut buf = String::new();
    let (mut bold, mut italic, mut code) = (false, false, false);
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' if chars.get(i + 1) == Some(&'*') => {
                flush(&mut buf, &mut spans, bold, italic, code);
                spans.push(Span::styled("**", Style::new().dim()));
                bold = !bold;
                i += 2;
            }
            '*' => {
                flush(&mut buf, &mut spans, bold, italic, code);
                spans.push(Span::styled("*", Style::new().dim()));
                italic = !italic;
                i += 1;
            }
            '`' => {
                flush(&mut buf, &mut spans, bold, italic, code);
                spans.push(Span::styled("`", Style::new().dim()));
                code = !code;
                i += 1;
            }
            c => {
                buf.push(c);
                i += 1;
            }
        }
    }
    flush(&mut buf, &mut spans, bold, italic, code);
    Line::from(spans)
}

fn flush(buf: &mut String, spans: &mut Vec<Span<'static>>, bold: bool, italic: bool, code: bool) {
    if buf.is_empty() {
        return;
    }
    let mut style = Style::new();
    if bold {
        style = style.bold();
    }
    if italic {
        style = style.italic();
    }
    if code {
        style = style.fg(Color::Yellow).bg(Color::DarkGray);
    }
    spans.push(Span::styled(std::mem::take(buf), style));
}

fn main() -> io::Result<()> {
    let path = env::args().nth(1).map(PathBuf::from);
    ratatui::run(|terminal| App::new(path).run(terminal))
}
