use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct RollingOutputBuffer {
    capacity: usize,
    lines: VecDeque<String>,
    current_line: String,
}

impl RollingOutputBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            lines: VecDeque::with_capacity(capacity),
            current_line: String::new(),
        }
    }

    pub fn append(&mut self, text: &str) {
        for ch in text.chars() {
            if ch == '\n' {
                if self.lines.len() >= self.capacity {
                    self.lines.pop_front();
                }
                self.lines.push_back(std::mem::take(&mut self.current_line));
            } else if ch == '\r' {
                // Return to start of current line
                self.current_line.clear();
            } else {
                self.current_line.push(ch);
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
