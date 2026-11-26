//! Terminal UI utilities: responsive banner, text wrapping, viewport management,
//! and interactive line reading with resize detection.

use std::io::{self, IsTerminal, Write};

use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

/// Terminal dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalSize {
    pub cols: u16,
    pub rows: u16,
}

impl Default for TerminalSize {
    fn default() -> Self {
        Self { cols: 80, rows: 24 }
    }
}

/// Check whether stdin and stdout are both connected to a TTY.
pub fn is_interactive_tty() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

/// Query the current terminal size, clamping to sensible minimums.
pub fn current_terminal_size() -> TerminalSize {
    crossterm::terminal::size()
        .map(|(cols, rows)| TerminalSize {
            cols: cols.max(20),
            rows: rows.max(6),
        })
        .unwrap_or_default()
}

/// RAII guard that enables raw mode on construction and disables it on drop.
pub struct RawModeGuard;

impl RawModeGuard {
    pub fn enter() -> io::Result<Self> {
        crossterm::terminal::enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
    }
}

/// Render a responsive Unicode box banner sized to the terminal width.
pub fn render_banner(title: &str, width: u16) -> String {
    let w = width.clamp(24, 120);
    let inner = w - 2;

    let title_chars: Vec<char> = title.chars().collect();
    let display_title = if title_chars.len() > inner as usize {
        let truncated: String = title_chars[..inner as usize - 1].iter().collect();
        format!("{}…", truncated)
    } else {
        title.to_string()
    };

    let display_title = format!("{:<width$}", display_title, width = inner as usize);
    let top = format!("╭{}╮", "─".repeat(inner as usize));
    let middle = format!("│{}│", display_title);
    let bottom = format!("╰{}╯", "─".repeat(inner as usize));

    format!(
        "\x1b[1;36m{}\x1b[0m\n\x1b[1;36m{}\x1b[0m\n\x1b[1;36m{}\x1b[0m",
        top, middle, bottom
    )
}

/// Wrap text to a given width, splitting on whitespace when possible.
pub fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = Vec::new();

    for line in text.split('\n') {
        if line.is_empty() {
            lines.push(String::new());
            continue;
        }

        let mut current = String::new();
        for word in line.split(' ') {
            if word.is_empty() {
                continue;
            }

            if current.is_empty() {
                if word.chars().count() <= width {
                    current = word.to_string();
                } else {
                    let mut chars = word.chars().peekable();
                    while chars.peek().is_some() {
                        let chunk: String = chars.by_ref().take(width).collect();
                        lines.push(chunk);
                    }
                }
            } else if current.chars().count() + 1 + word.chars().count() <= width {
                current.push(' ');
                current.push_str(word);
            } else {
                lines.push(current);
                current = String::new();
                if word.chars().count() <= width {
                    current = word.to_string();
                } else {
                    let mut chars = word.chars().peekable();
                    while chars.peek().is_some() {
                        let chunk: String = chars.by_ref().take(width).collect();
                        lines.push(chunk);
                    }
                }
            }
        }

        if !current.is_empty() {
            lines.push(current);
        }
    }

    lines
}

/// Format a diff preview to fit within the given width.
pub fn format_diff_preview_for_width(diff: &str, width: u16) -> String {
    let width = width.max(20) as usize;
    let cont_width = width.saturating_sub(2).max(1);
    let mut result = String::new();

    for line in diff.lines() {
        if line.chars().count() <= width {
            result.push_str(line);
            result.push('\n');
            continue;
        }
        let wrapped = wrap_text(line, cont_width);
        for (i, wline) in wrapped.iter().enumerate() {
            if i > 0 {
                result.push_str("  ");
            }
            result.push_str(wline);
            result.push('\n');
        }
    }

    result
}

/// Viewport state for the interactive TUI.
pub struct TuiViewport {
    pub size: TerminalSize,
    pub title: String,
    pub transcript: Vec<String>,
    pub status_bar: String,
    pub prompt_label: String,
    pub input_buffer: String,
    pub cursor_chars: usize,
    pub popup_items: Vec<String>,
}

impl TuiViewport {
    pub fn new(title: impl Into<String>, size: TerminalSize) -> Self {
        Self {
            size,
            title: title.into(),
            transcript: Vec::new(),
            status_bar: String::new(),
            prompt_label: "hypercode> ".to_string(),
            input_buffer: String::new(),
            cursor_chars: 0,
            popup_items: Vec::new(),
        }
    }

    pub fn on_resize(&mut self, cols: u16, rows: u16) {
        self.size = TerminalSize {
            cols: cols.max(20),
            rows: rows.max(6),
        };
    }

    pub fn push_transcript(&mut self, line: impl Into<String>) {
        self.transcript.push(line.into());
    }

    pub fn clear_transcript(&mut self) {
        self.transcript.clear();
    }

    /// Compute the wrapped (row, col) position of the cursor within the prompt line.
    pub fn cursor_row_col(&self) -> (u16, u16) {
        let prompt_len = self.prompt_label.chars().count();
        let target = prompt_len + self.cursor_chars.min(self.input_buffer.chars().count());
        let width = (self.size.cols as usize).max(1);
        let row = target / width;
        let col = target % width;
        (row as u16, col as u16)
    }

    /// Render the full viewport frame.
    pub fn render_frame(&self) -> String {
        let mut out = String::new();

        out.push_str(&render_banner(&self.title, self.size.cols));
        out.push('\n');

        let status_lines = if self.status_bar.is_empty() {
            Vec::new()
        } else {
            wrap_text(&self.status_bar, self.size.cols as usize)
        };
        let reserved = 4 + self.popup_items.len() + status_lines.len();
        let available_rows = (self.size.rows as usize).saturating_sub(reserved);
        let wrapped_transcript: Vec<String> = self
            .transcript
            .iter()
            .flat_map(|line| wrap_text(line, self.size.cols as usize))
            .collect();

        let visible = wrapped_transcript
            .iter()
            .rev()
            .take(available_rows)
            .collect::<Vec<_>>();
        for line in visible.iter().rev() {
            out.push_str(line);
            out.push('\n');
        }

        for item in &self.popup_items {
            out.push_str(item);
            out.push('\n');
        }

        for line in &status_lines {
            out.push_str(line);
            out.push('\n');
        }

        let prompt = format!("{}{}", self.prompt_label, self.input_buffer);
        let wrapped_prompt = wrap_text(&prompt, self.size.cols as usize);
        for (i, line) in wrapped_prompt.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(line);
        }

        out
    }
}

/// Read an interactive line with resize detection and responsive rerendering.
pub fn read_interactive_line(viewport: &mut TuiViewport) -> io::Result<Option<String>> {
    let _guard = RawModeGuard::enter()?;

    let redraw = |vp: &TuiViewport| {
        let frame = vp.render_frame().replace('\n', "\r\n");
        let _ = write!(io::stdout(), "\x1b[2J\x1b[H{}", frame);
        let _ = io::stdout().flush();
    };

    redraw(viewport);

    loop {
        let event = event::read()?;

        match event {
            Event::Resize(cols, rows) => {
                viewport.on_resize(cols, rows);
                redraw(viewport);
            }
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Enter => {
                    let line = viewport.input_buffer.clone();
                    viewport.input_buffer.clear();
                    viewport.cursor_chars = 0;
                    let _ = write!(io::stdout(), "\r\n");
                    let _ = io::stdout().flush();
                    return Ok(Some(line));
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    viewport.input_buffer.clear();
                    viewport.cursor_chars = 0;
                    redraw(viewport);
                }
                KeyCode::Char('d')
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && viewport.input_buffer.is_empty() =>
                {
                    let _ = write!(io::stdout(), "\r\n");
                    let _ = io::stdout().flush();
                    return Ok(None);
                }
                KeyCode::Backspace => {
                    if viewport.cursor_chars > 0 {
                        let mut chars: Vec<char> = viewport.input_buffer.chars().collect();
                        if viewport.cursor_chars <= chars.len() {
                            chars.remove(viewport.cursor_chars - 1);
                            viewport.input_buffer = chars.into_iter().collect();
                            viewport.cursor_chars -= 1;
                        }
                    }
                    redraw(viewport);
                }
                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                    let mut chars: Vec<char> = viewport.input_buffer.chars().collect();
                    let pos = viewport.cursor_chars.min(chars.len());
                    chars.insert(pos, c);
                    viewport.input_buffer = chars.into_iter().collect();
                    viewport.cursor_chars = pos + 1;
                    redraw(viewport);
                }
                _ => {}
            },
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strip_ansi(s: &str) -> String {
        let mut result = String::new();
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next == 'm' || next == 'H' || next == 'J' {
                        break;
                    }
                }
            } else {
                result.push(c);
            }
        }
        result
    }

    #[test]
    fn test_render_banner_adapts_to_widths() {
        for width in [24, 40, 80, 120] {
            let banner = render_banner("Test Title", width);
            let lines: Vec<&str> = banner.lines().collect();
            assert_eq!(
                lines.len(),
                3,
                "banner should have 3 lines at width {}",
                width
            );

            let clean_lines: Vec<String> = lines.iter().map(|l| strip_ansi(l)).collect();
            let widths: Vec<usize> = clean_lines.iter().map(|l| l.chars().count()).collect();
            assert!(
                widths.iter().all(|&w| w == widths[0]),
                "banner lines should have equal width at width {}: {:?}",
                width,
                widths
            );
            assert!(
                widths[0] <= width.into(),
                "banner width {} should not exceed terminal width {}",
                widths[0],
                width
            );
        }
    }

    #[test]
    fn test_wrap_text_handles_words_long_tokens_and_utf8() {
        let wrapped = wrap_text("hello world foo bar", 10);
        assert_eq!(wrapped, vec!["hello", "world foo", "bar"]);

        let long = wrap_text("abcdefghijklmnopqrstuvwxyz", 5);
        assert_eq!(long, vec!["abcde", "fghij", "klmno", "pqrst", "uvwxy", "z"]);

        let emoji = wrap_text("🚀🌍🔥", 2);
        assert_eq!(emoji, vec!["🚀🌍", "🔥"]);

        let cjk = wrap_text("你好世界", 2);
        assert_eq!(cjk, vec!["你好", "世界"]);

        let empty = wrap_text("line1\n\nline2", 10);
        assert_eq!(empty, vec!["line1", "", "line2"]);
    }

    #[test]
    fn test_format_diff_preview_for_width() {
        let diff = "  - old long line that should wrap\n  + new short line";

        let narrow = format_diff_preview_for_width(diff, 20);
        let narrow_lines: Vec<&str> = narrow.lines().collect();
        for line in &narrow_lines {
            assert!(
                line.chars().count() <= 20,
                "line too long: '{}' ({} chars)",
                line,
                line.chars().count()
            );
        }

        let wide = format_diff_preview_for_width(diff, 80);
        assert!(wide.contains("old long line that should wrap"));
    }

    #[test]
    fn test_viewport_resize_recalculates_layout_and_cursor() {
        let mut vp = TuiViewport::new("Test", TerminalSize { cols: 80, rows: 24 });
        vp.push_transcript("Hello world");
        vp.input_buffer = "some input".to_string();
        vp.cursor_chars = 4;

        let frame1 = vp.render_frame();
        assert!(frame1.contains("Test"));

        vp.on_resize(30, 12);
        let frame2 = vp.render_frame();
        assert!(frame2.contains("Test"));

        let (row, col) = vp.cursor_row_col();
        assert!(row < 12);
        assert!(col < 30);

        vp.on_resize(100, 30);
        let frame3 = vp.render_frame();
        assert!(frame3.contains("Test"));
    }
}
