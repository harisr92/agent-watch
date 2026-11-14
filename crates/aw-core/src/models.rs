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
    Confirm, // Yes/No, y/n
    Text,    // Freeform text input
    Select,  // Multiple choice selection
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingPrompt {
    pub id: String,
    pub session_id: String,
    pub prompt_text: String,
    pub prompt_type: PromptType,
    pub options: Vec<String>,
    pub created_at: DateTime<Utc>,
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
