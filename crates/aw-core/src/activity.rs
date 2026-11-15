//! Agent activity (working / waiting / done) from what TUI agents report through the
//! terminal title (OSC 0/2), the same signal terminal tab headers show.
//!
//! Claude Code puts a spinner glyph in front of the task name while it works
//! (`◐ Fix login bug`) and `✳` once it stops (`✳ Fix login bug`). Stopping alone does not say
//! why: the screen tells whether a question is waiting or the turn is done. The title flips
//! a moment before the screen is redrawn, so a stopped title over a screen showing neither
//! keeps the previous activity.

use crate::models::AgentActivity;

// These glyphs are not a documented protocol: they were captured from Claude Code 2.1.x.
// If an update changes them, `parse_title` returns `None` and activity silently falls back to
// unknown (screen heuristics only), so "done" notifications stop: check here first.

/// Title glyphs drawn while the agent works: half-circle and braille spinners.
const WORKING_GLYPHS: &[char] = &['◐', '◓', '◑', '◒'];
pub(crate) const BRAILLE_SPINNER: std::ops::RangeInclusive<char> = '\u{2801}'..='\u{28FF}';
/// Title glyphs drawn once the agent has stopped.
const STOPPED_GLYPHS: &[char] = &['✳'];

/// What the screen shows, as far as activity is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenState {
    /// A question the user has to answer (menu, confirmation, ...).
    Question,
    /// The idle chat input box: the agent waits for the next message.
    IdleInput,
    /// Neither, e.g. output still being drawn.
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleState {
    Working,
    Stopped,
}

/// A terminal title that carries an agent status glyph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentTitle {
    pub state: TitleState,
    /// The task name after the glyph, e.g. "Fix login bug".
    pub task: String,
}

/// Whether a session's terminal title can be trusted for activity. Only Claude Code's title
/// glyphs are known; another program with a spinner in its title could otherwise hide its
/// questions as "working".
pub fn reports_title_activity(command: &str, args: &[String]) -> bool {
    let program = std::path::Path::new(command)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(command);
    program == "claude" || args.iter().any(|a| a.contains("claude-code"))
}

/// Parses a terminal title; `None` when it has no known status glyph (plain shells,
/// other programs), so callers fall back to screen heuristics.
pub fn parse_title(title: &str) -> Option<AgentTitle> {
    let title = title.trim();
    let glyph = title.chars().next()?;
    let state = if WORKING_GLYPHS.contains(&glyph) || BRAILLE_SPINNER.contains(&glyph) {
        TitleState::Working
    } else if STOPPED_GLYPHS.contains(&glyph) {
        TitleState::Stopped
    } else {
        return None;
    };
    let task = title[glyph.len_utf8()..].trim().to_string();
    Some(AgentTitle { state, task })
}

/// The activity after a screen update.
///
/// Without a title signal the activity is unknown. "Done" needs the idle input box on screen
/// after the agent was seen at all, so the idle screen at startup is not reported as done.
pub fn next_activity(
    prev: Option<AgentActivity>,
    title: Option<TitleState>,
    screen: ScreenState,
) -> Option<AgentActivity> {
    match (title?, screen) {
        (TitleState::Working, _) => Some(AgentActivity::Working),
        (TitleState::Stopped, ScreenState::Question) => Some(AgentActivity::Waiting),
        (TitleState::Stopped, ScreenState::IdleInput) => prev.map(|_| AgentActivity::Done),
        (TitleState::Stopped, ScreenState::Other) => prev,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use AgentActivity::{Done, Waiting, Working};

    #[test]
    fn test_parse_title() {
        let cases: &[(&str, Option<(TitleState, &str)>)] = &[
            // Captured from Claude Code.
            ("◐ Claude Code", Some((TitleState::Working, "Claude Code"))),
            ("◑ Ok reply", Some((TitleState::Working, "Ok reply"))),
            ("✳ Ok reply", Some((TitleState::Stopped, "Ok reply"))),
            ("⠋ Building", Some((TitleState::Working, "Building"))),
            ("  ✳  Padded  ", Some((TitleState::Stopped, "Padded"))),
            ("✳", Some((TitleState::Stopped, ""))),
            // Near misses: no status glyph in front.
            ("Claude Code", None),
            ("vim ✳ notes.md", None),
            ("✻ Thinking", None),
            ("⠀ blank braille", None),
            ("", None),
        ];
        for (title, want) in cases {
            let got = parse_title(title).map(|t| (t.state, t.task));
            let want = want.map(|(s, t)| (s, t.to_string()));
            assert_eq!(got, want, "title {title:?}");
        }
    }

    #[test]
    fn test_reports_title_activity() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(reports_title_activity("claude", &[]));
        assert!(reports_title_activity(
            "/opt/homebrew/bin/claude",
            &args(&["-c"])
        ));
        assert!(reports_title_activity(
            "npx",
            &args(&["@anthropic-ai/claude-code"])
        ));
        assert!(!reports_title_activity("agy", &[]));
        assert!(!reports_title_activity("claude-wrapper", &[]));
        assert!(!reports_title_activity("python3", &args(&["claude.py"])));
    }

    #[test]
    fn test_next_activity() {
        use ScreenState::{IdleInput, Other, Question};
        use TitleState::{Stopped, Working as Busy};
        let cases = [
            // No title signal: unknown, whatever came before.
            (Some(Working), None, IdleInput, None),
            // Working always wins, even over a question-looking screen.
            (None, Some(Busy), Other, Some(Working)),
            (Some(Waiting), Some(Busy), Question, Some(Working)),
            // Stopped with a question on screen: waiting.
            (Some(Working), Some(Stopped), Question, Some(Waiting)),
            (None, Some(Stopped), Question, Some(Waiting)),
            // Stopped at the idle box: done, but not at startup.
            (Some(Working), Some(Stopped), IdleInput, Some(Done)),
            (Some(Waiting), Some(Stopped), IdleInput, Some(Done)),
            (Some(Done), Some(Stopped), IdleInput, Some(Done)),
            (None, Some(Stopped), IdleInput, None),
            // Title flipped before the screen redrew (menu not drawn yet): hold.
            (Some(Working), Some(Stopped), Other, Some(Working)),
            (Some(Waiting), Some(Stopped), Other, Some(Waiting)),
            (None, Some(Stopped), Other, None),
        ];
        for (prev, title, screen, want) in cases {
            assert_eq!(
                next_activity(prev, title, screen),
                want,
                "prev {prev:?}, title {title:?}, screen {screen:?}"
            );
        }
    }
}
