use crate::models::{PendingPrompt, PromptType};
use chrono::Utc;
use regex::Regex;
use uuid::Uuid;

pub struct PromptDetector {
    confirm_regexes: Vec<Regex>,
    choice_regexes: Vec<Regex>,
    text_prompt_regexes: Vec<Regex>,
}

impl Default for PromptDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptDetector {
    pub fn new() -> Self {
        let confirm_patterns = vec![
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
        ];

        let choice_patterns = vec![
            r"(?i)select an option",
            r"(?i)choose [1-9]",
            r"\[[0-9]-[0-9]\]:",
            r"\([0-9]-[0-9]\):",
        ];

        let text_patterns = vec![
            r"(?i)press enter to continue",
            r"(?i)press any key to continue",
            r"(?i)what should i [^?\n]+",
            r"(?i)enter your choice:",
            r"(?i)provide input:",
            r"(?i)input:\s*$",
            r"(?i)>\s*$",
            r"(?i)\?\s*>\s*$",
            r"(?i)prompt:\s*$",
            r"(?i)query:\s*$",
        ];

        Self {
            confirm_regexes: confirm_patterns
                .into_iter()
                .map(|p| Regex::new(p).expect("Valid regex"))
                .collect(),
            choice_regexes: choice_patterns
                .into_iter()
                .map(|p| Regex::new(p).expect("Valid regex"))
                .collect(),
            text_prompt_regexes: text_patterns
                .into_iter()
                .map(|p| Regex::new(p).expect("Valid regex"))
                .collect(),
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
        let trimmed = cleaned.trim_end();

        if trimmed.is_empty() {
            return None;
        }

        // Look at the last non-empty line or the trailing snippet
        let last_line = trimmed.lines().last().unwrap_or(trimmed).trim();

        // 1. Check Confirmation patterns
        for re in &self.confirm_regexes {
            if re.is_match(last_line) {
                return Some(PendingPrompt {
                    id: Uuid::new_v4().to_string(),
                    session_id: session_id.to_string(),
                    prompt_text: last_line.to_string(),
                    prompt_type: PromptType::Confirm,
                    options: vec!["y".to_string(), "n".to_string()],
                    created_at: Utc::now(),
                });
            }
        }

        // 2. Check Choice patterns
        for re in &self.choice_regexes {
            if re.is_match(last_line) {
                return Some(PendingPrompt {
                    id: Uuid::new_v4().to_string(),
                    session_id: session_id.to_string(),
                    prompt_text: last_line.to_string(),
                    prompt_type: PromptType::Select,
                    options: vec![],
                    created_at: Utc::now(),
                });
            }
        }

        // 3. Check Text input patterns
        for re in &self.text_prompt_regexes {
            if re.is_match(last_line) {
                let is_enter = last_line.to_lowercase().contains("press enter");
                return Some(PendingPrompt {
                    id: Uuid::new_v4().to_string(),
                    session_id: session_id.to_string(),
                    prompt_text: last_line.to_string(),
                    prompt_type: PromptType::Text,
                    options: if is_enter {
                        vec!["Enter".to_string()]
                    } else {
                        vec![]
                    },
                    created_at: Utc::now(),
                });
            }
        }

        // 4. Trailing question mark or prompt symbol heuristic
        if (last_line.ends_with('?')
            || last_line.ends_with(": ")
            || last_line.ends_with(':')
            || last_line.ends_with('>')
            || last_line.ends_with("> "))
            && last_line.len() > 3
        {
            let lower = last_line.to_lowercase();
            let is_confirm = lower.contains("proceed")
                || lower.contains("approve")
                || lower.contains("are you sure");
            let prompt_type = if is_confirm {
                PromptType::Confirm
            } else {
                PromptType::Text
            };
            let options = if is_confirm {
                vec!["y".to_string(), "n".to_string()]
            } else {
                vec![]
            };

            return Some(PendingPrompt {
                id: Uuid::new_v4().to_string(),
                session_id: session_id.to_string(),
                prompt_text: last_line.to_string(),
                prompt_type,
                options,
                created_at: Utc::now(),
            });
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            assert_eq!(p.prompt_type, PromptType::Confirm);
            assert_eq!(p.options, vec!["y", "n"]);
        }
    }

    #[test]
    fn test_detect_text_prompts() {
        let detector = PromptDetector::new();

        let prompt = detector
            .detect_prompt("sess-1", "Task completed. Press Enter to continue...")
            .expect("Should detect enter prompt");
        assert_eq!(prompt.prompt_type, PromptType::Text);
        assert_eq!(prompt.options, vec!["Enter"]);

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
}
