use crate::models::{PendingPrompt, PromptChoice, PromptType};
use regex::Regex;

/// Glyphs that TUIs draw in front of the highlighted menu entry.
const CURSOR_GLYPHS: &[char] = &['❯', '›', '▸', '➤', '▶', '→'];
const ARROW_UP: &str = "\x1b[A";
const ARROW_DOWN: &str = "\x1b[B";

pub struct PromptDetector {
    confirm_regexes: Vec<Regex>,
    choice_regexes: Vec<Regex>,
    text_prompt_regexes: Vec<Regex>,
    numbered_option: Regex,
    paren_letter_choice: Regex,
    bracket_choices: Regex,
    numeric_range: Regex,
    secret: Regex,
    acknowledge: Regex,
    menu_hint: Regex,
}

impl Default for PromptDetector {
    fn default() -> Self {
        Self::new()
    }
}

fn compile(patterns: &[&str]) -> Vec<Regex> {
    patterns
        .iter()
        .map(|p| Regex::new(p).expect("Valid regex"))
        .collect()
}

/// A parsed "1. Label" / "b) Label" menu line.
struct OptionLine {
    key: String,
    label: String,
    has_cursor: bool,
}

impl PromptDetector {
    pub fn new() -> Self {
        Self {
            confirm_regexes: compile(&[
                r"(?i)\[y/n\]",
                r"(?i)\(y/n\)",
                r"(?i)\[yes/no\]",
                r"(?i)\(yes/no\)",
                r"(?i)approve\?",
                r"(?i)allow this\?",
                r"(?i)proceed\?",
                r"(?i)continue\?\s*(\[.*?\])?",
                r"(?i)do you want to [^?\n]+\?",
                r"(?i)are you sure[^?\n]+\?",
                r"(?i)run this command\?",
            ]),
            choice_regexes: compile(&[
                r"(?i)select an option",
                r"(?i)choose [1-9]",
                r"\[[0-9]-[0-9]\]:",
                r"\([0-9]-[0-9]\):",
            ]),
            text_prompt_regexes: compile(&[
                r"(?i)what should i [^?\n]+",
                r"(?i)enter your choice:",
                r"(?i)provide input:",
                r"(?i)input:\s*$",
                r"(?i)>\s*$",
                r"(?i)\?\s*>\s*$",
                r"(?i)prompt:\s*$",
                r"(?i)query:\s*$",
            ]),
            // Optional cursor, then "1." / "2)" / "(3)" / "[a]" / "b:" followed by a label.
            numbered_option: Regex::new(
                r"^(?P<cur>[❯›▸➤▶→>*]\s*)?[\[(]?(?P<key>\d{1,2}|[A-Za-z])[.)\]:]\s+(?P<label>\S.*)$",
            )
            .expect("Valid regex"),
            // "(Y)es/(N)o/(A)ll"
            paren_letter_choice: Regex::new(r"\(([A-Za-z])\)([A-Za-z'][A-Za-z' ]*)")
                .expect("Valid regex"),
            // "[y/n/a]", "(yes/no/always)", "[r|s|q]"
            bracket_choices: Regex::new(
                r"[\[(]\s*([A-Za-z0-9]{1,12}(?:\s*[/|,]\s*[A-Za-z0-9]{1,12})+)\s*[\])]",
            )
            .expect("Valid regex"),
            numeric_range: Regex::new(r"[\[(](\d)\s*-\s*(\d)[\])]").expect("Valid regex"),
            secret: Regex::new(
                r"(?i)(password|passphrase|passcode|api[ _-]?key|secret|token)[^:\n]*:\s*$",
            )
            .expect("Valid regex"),
            acknowledge: Regex::new(r"(?i)press (enter|return|any key)|hit (enter|return)")
                .expect("Valid regex"),
            menu_hint: Regex::new(
                r"(?i)^\(|to (select|navigate|confirm|cancel|move)|arrow keys|\besc\b|↑|↓",
            )
            .expect("Valid regex"),
        }
    }

    /// Strips ANSI escape sequences and cleans control characters
    pub fn clean_text(raw: &[u8]) -> String {
        let stripped = strip_ansi_escapes::strip(raw);
        String::from_utf8_lossy(&stripped).to_string()
    }

    /// Analyzes the recent output tail to see if an interactive prompt is waiting for input
    pub fn detect_prompt(&self, session_id: &str, recent_text: &str) -> Option<PendingPrompt> {
        let cleaned = Self::clean_text(recent_text.as_bytes());
        // Drop box-drawing frames and blank lines so framed TUI menus read like plain ones.
        let trimmed_lines: Vec<&str> = cleaned
            .lines()
            .map(|l| {
                l.trim_matches(|c: char| {
                    c.is_whitespace() || ('\u{2500}'..='\u{257F}').contains(&c) || c == '|'
                })
            })
            .collect();
        let lines: Vec<&str> = trimmed_lines
            .iter()
            .copied()
            .filter(|l| !l.is_empty())
            .collect();
        let last_line = *lines.last()?;

        // 1. Multi-line menus (numbered/lettered lists, arrow-key lists)
        if let Some(p) = self.detect_keyed_menu(session_id, &lines, &trimmed_lines) {
            return Some(p);
        }
        if let Some(p) = self.detect_arrow_menu(session_id, &lines, &trimmed_lines) {
            return Some(p);
        }

        // 2. Choices written inline on the prompt line
        if let Some(p) = self.detect_inline_choices(session_id, last_line) {
            return Some(p);
        }

        // 3. Yes/No confirmations
        if self.confirm_regexes.iter().any(|re| re.is_match(last_line)) {
            return Some(Self::confirm(session_id, last_line));
        }

        // 4. Secrets
        if self.secret.is_match(last_line) {
            return Some(PendingPrompt::new(
                session_id,
                last_line,
                PromptType::Secret,
                vec![],
            ));
        }

        // 5. "Press Enter to continue"
        if self.acknowledge.is_match(last_line) {
            return Some(PendingPrompt::new(
                session_id,
                last_line,
                PromptType::Acknowledge,
                vec![PromptChoice::new("Continue", "")],
            ));
        }

        // 6. A choice prompt whose options we could not parse: "Pick one [1-4]:"
        if self.choice_regexes.iter().any(|re| re.is_match(last_line)) {
            let choices = self
                .numeric_range
                .captures(last_line)
                .and_then(|c| Some((c[1].parse::<u32>().ok()?, c[2].parse::<u32>().ok()?)))
                .filter(|(a, b)| a < b)
                .map(|(a, b)| {
                    (a..=b)
                        .map(|n| PromptChoice::new(n.to_string(), n.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            return Some(PendingPrompt::new(
                session_id,
                last_line,
                PromptType::Select,
                choices,
            ));
        }

        // 7. Freeform text input
        if self
            .text_prompt_regexes
            .iter()
            .any(|re| re.is_match(last_line))
        {
            return Some(PendingPrompt::new(
                session_id,
                last_line,
                PromptType::Text,
                vec![],
            ));
        }

        // 8. Trailing question mark or prompt symbol heuristic
        if Self::looks_like_prompt_line(last_line) && last_line.len() > 3 {
            let lower = last_line.to_lowercase();
            let is_confirm = lower.contains("proceed")
                || lower.contains("approve")
                || lower.contains("are you sure");
            return Some(if is_confirm {
                Self::confirm(session_id, last_line)
            } else {
                PendingPrompt::new(session_id, last_line, PromptType::Text, vec![])
            });
        }

        None
    }

    fn confirm(session_id: &str, text: &str) -> PendingPrompt {
        PendingPrompt::new(
            session_id,
            text,
            PromptType::Confirm,
            vec![PromptChoice::new("Yes", "y"), PromptChoice::new("No", "n")],
        )
    }

    fn looks_like_prompt_line(line: &str) -> bool {
        let t = line.trim_end();
        t.ends_with('?') || t.ends_with(':') || t.ends_with('>')
    }

    fn parse_option_line(&self, line: &str) -> Option<OptionLine> {
        let caps = self.numbered_option.captures(line)?;
        Some(OptionLine {
            key: caps["key"].to_string(),
            label: caps["label"].trim().to_string(),
            has_cursor: caps.name("cur").is_some(),
        })
    }

    /// Keys must count up one at a time (1,2,3 / a,b,c) to rule out ordinary prose.
    fn keys_are_sequential(options: &[OptionLine]) -> bool {
        let ordinal = |k: &str| -> Option<u32> {
            k.parse::<u32>().ok().or_else(|| {
                let c = k.chars().next()?;
                Some(c.to_ascii_lowercase() as u32)
            })
        };
        let all_digits = options.iter().all(|o| o.key.parse::<u32>().is_ok());
        let all_letters = options.iter().all(|o| o.key.parse::<u32>().is_err());
        if !(all_digits || all_letters) {
            return false;
        }
        options.windows(2).all(|w| {
            matches!((ordinal(&w[0].key), ordinal(&w[1].key)), (Some(a), Some(b)) if b == a + 1)
        })
    }

    /// Menus whose entries carry a key the user types:
    ///
    /// ```text
    /// Do you want to make this edit?
    /// ❯ 1. Yes
    ///   2. Yes, allow all edits this session
    ///   3. No
    /// ```
    fn detect_keyed_menu(
        &self,
        session_id: &str,
        lines: &[&str],
        trimmed_lines: &[&str],
    ) -> Option<PendingPrompt> {
        // Up to three lines may follow the options: an input prompt and/or key hints.
        let mut end = lines.len();
        while end > 0 && lines.len() - end < 3 && self.parse_option_line(lines[end - 1]).is_none() {
            end -= 1;
        }
        let trailers = &lines[end..];

        let mut start = end;
        while start > 0 && self.parse_option_line(lines[start - 1]).is_some() {
            start -= 1;
        }
        let options: Vec<OptionLine> = lines[start..end]
            .iter()
            .filter_map(|l| self.parse_option_line(l))
            .collect();
        if options.len() < 2 || !Self::keys_are_sequential(&options) {
            return None;
        }

        // A bare numbered list is usually just output. Require evidence that input is awaited:
        // a highlighted entry, or an input prompt line right after the list.
        let has_cursor = options.iter().any(|o| o.has_cursor);
        let trailing_prompt = trailers
            .iter()
            .rev()
            .find(|l| Self::looks_like_prompt_line(l))
            .copied();
        if !has_cursor && (trailing_prompt.is_none() || trailers.len() > 2) {
            return None;
        }

        let first_opt = lines[start];
        let first_opt_idx = trimmed_lines
            .iter()
            .rposition(|&l| l == first_opt)
            .unwrap_or(0);

        let mut preamble: Vec<&str> = Vec::new();
        let mut started = false;
        for i in (0..first_opt_idx).rev() {
            let line = trimmed_lines[i];
            if line.is_empty() {
                if started {
                    break;
                }
                continue;
            }
            started = true;
            preamble.push(line);
            if preamble.len() >= 10 {
                break;
            }
        }
        preamble.reverse();

        let question = if !preamble.is_empty() {
            preamble.join("\n")
        } else {
            trailing_prompt.unwrap_or("Select an option").to_string()
        };

        let choices = options
            .into_iter()
            .map(|o| PromptChoice::new(o.label, o.key))
            .collect();
        let mut prompt = PendingPrompt::new(session_id, question, PromptType::Select, choices);
        // A highlighted entry means a keypress-driven TUI: the key alone selects.
        prompt.press_enter = !has_cursor;
        Some(prompt)
    }

    /// Menus navigated with the arrow keys, with no key to type:
    ///
    /// ```text
    /// ? Which model should I use?
    /// ❯ Fast
    ///   Balanced
    ///   Thorough
    /// ```
    ///
    /// Each choice's value is the arrow presses needed to reach it, followed by Enter.
    fn detect_arrow_menu(
        &self,
        session_id: &str,
        lines: &[&str],
        trimmed_lines: &[&str],
    ) -> Option<PendingPrompt> {
        let is_cursor_line = |l: &str| l.starts_with(CURSOR_GLYPHS);
        let window_start = lines.len().saturating_sub(12);
        let cursor_idx = (window_start..lines.len())
            .rev()
            .find(|&i| is_cursor_line(lines[i]))?;

        // The question is the nearest line above the cursor that reads like one.
        let question_idx = (window_start..cursor_idx).rev().find(|&i| {
            let l = lines[i];
            l.starts_with('?') || l.contains('?') || l.trim_end().ends_with(':')
        })?;

        let entries: Vec<(usize, &str)> = (question_idx + 1..lines.len())
            .filter(|&i| i == cursor_idx || !self.menu_hint.is_match(lines[i]))
            .map(|i| (i, lines[i]))
            .collect();
        if entries.len() < 2 {
            return None;
        }
        let cursor_pos = entries.iter().position(|(i, _)| *i == cursor_idx)?;

        let choices = entries
            .iter()
            .enumerate()
            .map(|(pos, (_, text))| {
                let label = text
                    .trim_start_matches(|c: char| CURSOR_GLYPHS.contains(&c) || c.is_whitespace())
                    .trim_start_matches(['◯', '◉', '○', '●'])
                    .trim();
                let moves = if pos >= cursor_pos {
                    ARROW_DOWN.repeat(pos - cursor_pos)
                } else {
                    ARROW_UP.repeat(cursor_pos - pos)
                };
                PromptChoice::new(label, format!("{moves}\r"))
            })
            .collect();

        let q_line = lines[question_idx].trim_start_matches(['?', ' ']);
        let q_idx = trimmed_lines
            .iter()
            .rposition(|&l| l == lines[question_idx])
            .unwrap_or(0);
        let mut preamble: Vec<&str> = Vec::new();
        for i in (0..q_idx).rev() {
            let line = trimmed_lines[i];
            if line.is_empty() {
                break;
            }
            preamble.push(line.trim_start_matches(['?', ' ']));
            if preamble.len() >= 10 {
                break;
            }
        }
        preamble.reverse();
        preamble.push(q_line);
        let question = preamble.join("\n");

        let mut prompt = PendingPrompt::new(session_id, question, PromptType::Select, choices);
        prompt.press_enter = false;
        Some(prompt)
    }

    /// Choices listed on the prompt line itself: `(Y)es/(N)o/(A)ll`, `[y/n/a/q]`.
    fn detect_inline_choices(&self, session_id: &str, line: &str) -> Option<PendingPrompt> {
        let lettered: Vec<PromptChoice> = self
            .paren_letter_choice
            .captures_iter(line)
            .map(|c| {
                PromptChoice::new(format!("{}{}", &c[1], c[2].trim_end()), c[1].to_lowercase())
            })
            .collect();
        if lettered.len() >= 2 {
            return Some(Self::choices_prompt(session_id, line, lettered));
        }

        let caps = self.bracket_choices.captures(line)?;
        // Only trust a bracket group that sits at the end of a prompt-looking line.
        let after = line[caps.get(0)?.end()..].trim();
        if !(after.is_empty() || after == ":" || after == "?" || after == ">") {
            return None;
        }
        let choices: Vec<PromptChoice> = caps[1]
            .split(['/', '|', ','])
            .map(str::trim)
            .map(|t| {
                let label = match t.to_lowercase().as_str() {
                    "y" | "yes" => "Yes".to_string(),
                    "n" | "no" => "No".to_string(),
                    _ => t.to_string(),
                };
                PromptChoice::new(label, t.to_lowercase())
            })
            .collect();
        Some(Self::choices_prompt(session_id, line, choices))
    }

    /// Builds a Confirm prompt when the choices are exactly yes/no, otherwise a Select.
    fn choices_prompt(session_id: &str, text: &str, choices: Vec<PromptChoice>) -> PendingPrompt {
        let is_yes_no = choices.len() == 2
            && matches!(choices[0].value.as_str(), "y" | "yes")
            && matches!(choices[1].value.as_str(), "n" | "no");
        if is_yes_no {
            Self::confirm(session_id, text)
        } else {
            PendingPrompt::new(session_id, text, PromptType::Select, choices)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(p: &PendingPrompt) -> Vec<&str> {
        p.choices.iter().map(|c| c.label.as_str()).collect()
    }

    #[test]
    fn test_detect_confirm_prompts() {
        let detector = PromptDetector::new();

        let samples = vec![
            "Do you want to run bash command: rm -rf tmp? [y/N]",
            "Approve file modification to src/main.rs? (Y/n)",
            "Do you want to continue? [y/N]:",
            "Are you sure you want to proceed?",
            "Run this command? (yes/no)",
        ];

        for sample in samples {
            let prompt = detector.detect_prompt("sess-1", sample);
            assert!(
                prompt.is_some(),
                "Failed to detect confirmation for: {}",
                sample
            );
            let p = prompt.unwrap();
            assert_eq!(p.prompt_type, PromptType::Confirm, "sample: {}", sample);
            assert_eq!(p.options, vec!["y", "n"]);
        }
    }

    #[test]
    fn test_detect_text_prompts() {
        let detector = PromptDetector::new();

        let prompt = detector
            .detect_prompt("sess-1", "Task completed. Press Enter to continue...")
            .expect("Should detect enter prompt");
        assert_eq!(prompt.prompt_type, PromptType::Acknowledge);
        assert_eq!(prompt.options, vec![""]);

        let prompt2 = detector
            .detect_prompt("sess-2", "What should I search for next? > ")
            .expect("Should detect search prompt");
        assert_eq!(prompt2.prompt_type, PromptType::Text);
    }

    #[test]
    fn test_ansi_stripping() {
        let detector = PromptDetector::new();
        let raw = "\x1b[32mAgent:\x1b[0m Do you want to run this command? \x1b[1m[y/N]\x1b[0m ";
        let prompt = detector.detect_prompt("sess-1", raw);
        assert!(prompt.is_some());
        assert_eq!(prompt.unwrap().prompt_type, PromptType::Confirm);
    }

    #[test]
    fn test_numbered_menu_with_input_line() {
        let detector = PromptDetector::new();
        let text = "Analysis done.\nHow should I apply the migration?\n  1) Apply now\n  2) Dry run first\n  3) Skip\nEnter your choice [1-3]: ";
        let p = detector.detect_prompt("s", text).expect("menu");
        assert_eq!(p.prompt_type, PromptType::Select);
        assert_eq!(
            p.prompt_text,
            "Analysis done.\nHow should I apply the migration?"
        );
        assert_eq!(labels(&p), vec!["Apply now", "Dry run first", "Skip"]);
        assert_eq!(p.options, vec!["1", "2", "3"]);
        assert!(p.press_enter);
    }

    #[test]
    fn test_framed_tui_menu_with_cursor() {
        let detector = PromptDetector::new();
        let text = "╭──────────────────────────────╮\n│ Do you want to make this edit? │\n│ ❯ 1. Yes                       │\n│   2. Yes, allow all edits      │\n│   3. No, tell me what to do    │\n╰──────────────────────────────╯\n  Esc to cancel";
        let p = detector.detect_prompt("s", text).expect("menu");
        assert_eq!(p.prompt_type, PromptType::Select);
        assert_eq!(p.prompt_text, "Do you want to make this edit?");
        assert_eq!(
            labels(&p),
            vec!["Yes", "Yes, allow all edits", "No, tell me what to do"]
        );
        assert!(!p.press_enter, "keypress menus must not get an Enter");
    }

    #[test]
    fn test_menu_from_pty_buffer() {
        // End-to-end with the rolling buffer the daemon uses, fed raw PTY output.
        let mut buf = crate::buffer::RollingOutputBuffer::new(30);
        buf.append("\x1b[1;35m[MenuAgent]\x1b[0m Planning database migration...\r\nHow should I apply the migration?\r\n  1) Apply now\r\n  2) Dry run first\r\n  3) Skip this step\r\nEnter your choice [1-3]: ");
        let tail = buf.get_tail(30).join("\n");
        let p = PromptDetector::new().detect_prompt("s", &tail).expect("menu");
        assert_eq!(
            p.prompt_text,
            "[MenuAgent] Planning database migration...\nHow should I apply the migration?"
        );
        assert_eq!(labels(&p), vec!["Apply now", "Dry run first", "Skip this step"]);
    }

    #[test]
    fn test_menu_stops_at_blank_line() {
        let detector = PromptDetector::new();
        let text = "Build finished in 2.3s\n\n[MenuAgent] Planning database migration...\nHow should I apply the migration?\n  1) Apply now\n  2) Dry run first\n  3) Skip\nEnter your choice [1-3]: ";
        let p = detector.detect_prompt("s", text).expect("menu");
        assert_eq!(
            p.prompt_text,
            "[MenuAgent] Planning database migration...\nHow should I apply the migration?"
        );
    }

    #[test]
    fn test_menu_with_blank_line_before_options() {
        let detector = PromptDetector::new();
        let text = "[MenuAgent] Planning database migration...\nHow should I apply the migration?\n\n  1) Apply now\n  2) Dry run first\n  3) Skip\nEnter your choice [1-3]: ";
        let p = detector.detect_prompt("s", text).expect("menu");
        assert_eq!(
            p.prompt_text,
            "[MenuAgent] Planning database migration...\nHow should I apply the migration?"
        );
    }

    #[test]
    fn test_lettered_menu() {
        let detector = PromptDetector::new();
        let text =
            "Conflict in config.toml\na) Keep mine\nb) Take theirs\nc) Open editor\nChoice: ";
        let p = detector.detect_prompt("s", text).expect("menu");
        assert_eq!(p.options, vec!["a", "b", "c"]);
        assert_eq!(labels(&p), vec!["Keep mine", "Take theirs", "Open editor"]);
    }

    #[test]
    fn test_plain_numbered_output_is_not_a_prompt() {
        let detector = PromptDetector::new();
        let text = "Plan:\n1. Read the files\n2. Write the patch\n3. Run the tests";
        assert!(detector.detect_prompt("s", text).is_none());
    }

    #[test]
    fn test_arrow_menu_sends_arrow_keys() {
        let detector = PromptDetector::new();
        let text = "? Which model should I use? (Use arrow keys)\n  Fast\n❯ Balanced\n  Thorough";
        let p = detector.detect_prompt("s", text).expect("menu");
        assert_eq!(p.prompt_type, PromptType::Select);
        assert_eq!(labels(&p), vec!["Fast", "Balanced", "Thorough"]);
        assert_eq!(p.options, vec!["\x1b[A\r", "\r", "\x1b[B\r"]);
        assert!(!p.press_enter);
    }

    #[test]
    fn test_inline_letter_choices() {
        let detector = PromptDetector::new();

        let p = detector
            .detect_prompt(
                "s",
                "Run shell command? (Y)es/(N)o/(S)kip all/(D)on't ask again [Yes]: ",
            )
            .expect("inline");
        assert_eq!(p.prompt_type, PromptType::Select);
        assert_eq!(labels(&p), vec!["Yes", "No", "Skip all", "Don't ask again"]);
        assert_eq!(p.options, vec!["y", "n", "s", "d"]);

        let p = detector
            .detect_prompt("s", "Overwrite existing file? [y/n/a/q] ")
            .expect("inline");
        assert_eq!(p.prompt_type, PromptType::Select);
        assert_eq!(p.options, vec!["y", "n", "a", "q"]);
        assert_eq!(labels(&p), vec!["Yes", "No", "a", "q"]);
    }

    #[test]
    fn test_numeric_range_without_listed_options() {
        let detector = PromptDetector::new();
        let p = detector
            .detect_prompt("s", "Select an option [1-4]: ")
            .expect("range");
        assert_eq!(p.prompt_type, PromptType::Select);
        assert_eq!(p.options, vec!["1", "2", "3", "4"]);
    }

    #[test]
    fn test_secret_prompt() {
        let detector = PromptDetector::new();
        for sample in [
            "Password: ",
            "Enter your API key: ",
            "[sudo] password for sam: ",
        ] {
            let p = detector.detect_prompt("s", sample).expect(sample);
            assert_eq!(p.prompt_type, PromptType::Secret, "sample: {}", sample);
        }
    }
}
