use std::collections::VecDeque;

/// Size of the virtual terminal used to render full-screen TUIs. It only needs to be at least
/// as large as the real terminal: TUIs position the cursor absolutely, so extra room stays blank.
const SCREEN_ROWS: u16 = 100;
const SCREEN_COLS: u16 = 300;

pub struct RollingOutputBuffer {
    capacity: usize,
    /// Full-screen TUIs (Claude Code, etc.) draw on the alternate screen with cursor movement
    /// instead of newlines, which a line buffer cannot follow. This emulator renders them.
    screen: vt100::Parser,
    lines: VecDeque<String>,
    current_line: String,
    /// A '\r' was the last character seen. It is either half of a "\r\n" line ending
    /// (possibly split across chunks) or a carriage return that redraws the line.
    pending_cr: bool,
}

impl RollingOutputBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            screen: vt100::Parser::new(SCREEN_ROWS, SCREEN_COLS, 0),
            lines: VecDeque::with_capacity(capacity),
            current_line: String::new(),
            pending_cr: false,
        }
    }

    pub fn append(&mut self, text: &str) {
        self.screen.process(text.as_bytes());
        for ch in text.chars() {
            match ch {
                '\n' => {
                    self.pending_cr = false;
                    if self.lines.len() >= self.capacity {
                        self.lines.pop_front();
                    }
                    self.lines.push_back(std::mem::take(&mut self.current_line));
                }
                '\r' => self.pending_cr = true,
                _ => {
                    // A lone carriage return: the program is redrawing this line.
                    if std::mem::take(&mut self.pending_cr) {
                        self.current_line.clear();
                    }
                    self.current_line.push(ch);
                }
            }
        }
    }

    pub fn get_lines(&self) -> Vec<String> {
        let mut result: Vec<String> = self.lines.iter().cloned().collect();
        if !self.current_line.is_empty() {
            result.push(self.current_line.clone());
        }
        result
    }

    pub fn get_tail(&self, n: usize) -> Vec<String> {
        let all = self.get_lines();
        if all.len() <= n {
            all
        } else {
            all[all.len() - n..].to_vec()
        }
    }

    /// The last `n` lines as the user sees them: the rendered screen for full-screen TUIs,
    /// otherwise the line-oriented output.
    pub fn visible_tail(&self, n: usize) -> Vec<String> {
        let screen = self.screen.screen();
        if !screen.alternate_screen() {
            return self.get_tail(n);
        }
        let rows: Vec<String> = screen
            .rows(0, SCREEN_COLS)
            .map(|r| r.trim_end().to_string())
            .collect();
        let end = rows
            .iter()
            .rposition(|r| !r.is_empty())
            .map_or(0, |i| i + 1);
        rows[end.saturating_sub(n)..end].to_vec()
    }

    /// The terminal title last set by the program (OSC 0/2), empty if none.
    pub fn title(&self) -> &str {
        self.screen.screen().title()
    }

    pub fn get_current_line(&self) -> &str {
        &self.current_line
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_lines_and_capacity() {
        let mut buf = RollingOutputBuffer::new(3);
        buf.append("line 1\nline 2\nline 3\nline 4\n");
        let lines = buf.get_lines();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines, vec!["line 2", "line 3", "line 4"]);
    }

    #[test]
    fn test_crlf_line_endings_keep_text() {
        let mut buf = RollingOutputBuffer::new(5);
        // PTYs emit \r\n, and a read can split the pair across chunks.
        buf.append("1) Apply now\r");
        buf.append("\n2) Dry run\r\nChoice: ");
        assert_eq!(
            buf.get_lines(),
            vec!["1) Apply now", "2) Dry run", "Choice: "]
        );
    }

    #[test]
    fn test_lone_carriage_return_redraws_line() {
        let mut buf = RollingOutputBuffer::new(5);
        buf.append("Progress 10%\rProgress 90%\rDone\n");
        assert_eq!(buf.get_lines(), vec!["Done"]);
    }

    #[test]
    fn test_alternate_screen_is_rendered() {
        let mut buf = RollingOutputBuffer::new(5);
        // Claude Code style: alternate screen, rows reached with "\r" + cursor-down, no "\n".
        buf.append(
            "\x1b[?1049h\x1b[2J\x1b[H\r\x1b[1BHeader\r\x1b[1B\u{2500}\u{2500}\r\x1b[1B\u{276f} hi",
        );
        assert_eq!(
            buf.visible_tail(5),
            vec!["", "Header", "\u{2500}\u{2500}", "\u{276f} hi"]
        );
        assert_eq!(buf.visible_tail(2), vec!["\u{2500}\u{2500}", "\u{276f} hi"]);
    }

    #[test]
    fn test_visible_tail_without_alternate_screen_uses_lines() {
        let mut buf = RollingOutputBuffer::new(5);
        buf.append("a\nb\nprompt> ");
        assert_eq!(buf.visible_tail(2), vec!["b", "prompt> "]);
    }

    #[test]
    fn test_title_tracks_latest_osc_title() {
        let mut buf = RollingOutputBuffer::new(5);
        assert_eq!(buf.title(), "");
        buf.append("\x1b]0;\u{25d0} Fix bug\x07work");
        assert_eq!(buf.title(), "\u{25d0} Fix bug");
        // ST-terminated OSC 2, and other OSC sequences (progress, hyperlinks) leave it alone.
        buf.append("\x1b]2;\u{2733} Fix bug\x1b\\\x1b]9;4;0;\x07\x1b]8;;file:///a\x07");
        assert_eq!(buf.title(), "\u{2733} Fix bug");
    }

    #[test]
    fn test_buffer_partial_line() {
        let mut buf = RollingOutputBuffer::new(5);
        buf.append("Hello world\nWaiting: ");
        let lines = buf.get_lines();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "Hello world");
        assert_eq!(lines[1], "Waiting: ");
        assert_eq!(buf.get_current_line(), "Waiting: ");
    }
}
