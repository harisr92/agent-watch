use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Idle,
    Running,
    WaitingForInput,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptType {
    Confirm,     // Yes/No, y/n
    Text,        // Freeform text input
    Select,      // Pick one of several choices
    Secret,      // Password / token entry (masked in the UI)
    Acknowledge, // "Press Enter to continue"
}

/// One selectable answer: `label` is shown to the user, `value` is what gets sent to the agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptChoice {
    pub label: String,
    pub value: String,
}

impl PromptChoice {
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingPrompt {
    pub id: String,
    pub session_id: String,
    pub prompt_text: String,
    pub prompt_type: PromptType,
    /// Raw values of the choices (kept for older clients); see `choices` for labels.
    pub options: Vec<String>,
    #[serde(default)]
    pub choices: Vec<PromptChoice>,
    /// Whether a chosen value must be followed by Enter. False for keypress-driven menus.
    #[serde(default = "default_true")]
    pub press_enter: bool,
    pub created_at: DateTime<Utc>,
}

impl PendingPrompt {
    pub fn new(
        session_id: &str,
        prompt_text: impl Into<String>,
        prompt_type: PromptType,
        choices: Vec<PromptChoice>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            prompt_text: prompt_text.into(),
            prompt_type,
            options: choices.iter().map(|c| c.value.clone()).collect(),
            choices,
            press_enter: true,
            created_at: Utc::now(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSession {
    pub id: String,
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub pid: Option<u32>,
    pub status: AgentStatus,
    pub pending_prompt: Option<PendingPrompt>,
    pub started_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub recent_output: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
#[serde(rename_all = "snake_case")]
pub enum AgentEvent {
    SessionRegistered(AgentSession),
    SessionUpdated(AgentSession),
    PromptRequired {
        session_id: String,
        prompt: PendingPrompt,
    },
    PromptResolved {
        session_id: String,
        prompt_id: String,
        input: String,
        #[serde(default = "default_true")]
        press_enter: bool,
    },
    OutputDelta {
        session_id: String,
        chunk: String,
    },
    SessionTerminated {
        session_id: String,
        exit_code: Option<i32>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInputRequest {
    pub input: String,
    #[serde(default = "default_true")]
    pub press_enter: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterSessionRequest {
    pub id: String,
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub pid: Option<u32>,
}
