use aw_core::buffer::RollingOutputBuffer;
use aw_core::detector::PromptDetector;
use aw_core::models::{
    AgentEvent, AgentSession, AgentStatus, PendingPrompt, PromptChoice, PromptType,
    RegisterSessionRequest, UserInputRequest,
};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, oneshot, RwLock};
use tracing::{info, warn};
use uuid::Uuid;

pub struct ActiveSession {
    pub session: AgentSession,
    pub buffer: RollingOutputBuffer,
    pub input_sender: Option<mpsc::Sender<UserInputRequest>>,
    pub mcp_pending_responders: HashMap<String, oneshot::Sender<String>>,
}

#[derive(Clone)]
pub struct AppState {
    pub sessions: Arc<RwLock<HashMap<String, ActiveSession>>>,
    pub events_tx: broadcast::Sender<AgentEvent>,
    pub detector: Arc<PromptDetector>,
}

impl AppState {
    pub fn new() -> Self {
        let (events_tx, _) = broadcast::channel(500);
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            events_tx,
            detector: Arc::new(PromptDetector::new()),
        }
    }

    pub async fn register_session(
        &self,
        req: RegisterSessionRequest,
        input_sender: Option<mpsc::Sender<UserInputRequest>>,
    ) -> AgentSession {
        let now = Utc::now();
        let session = AgentSession {
            id: req.id.clone(),
            name: req.name,
            command: req.command,
            args: req.args,
            cwd: req.cwd,
            pid: req.pid,
            status: AgentStatus::Running,
            pending_prompt: None,
            started_at: now,
            updated_at: now,
            recent_output: Vec::new(),
        };

        let active = ActiveSession {
            session: session.clone(),
            buffer: RollingOutputBuffer::new(150),
            input_sender,
            mcp_pending_responders: HashMap::new(),
        };

        {
            let mut lock = self.sessions.write().await;
            lock.insert(req.id.clone(), active);
        }

        let _ = self
            .events_tx
            .send(AgentEvent::SessionRegistered(session.clone()));
        info!("Session registered: {} ({})", session.name, session.id);
        session
    }

    pub async fn append_output(&self, session_id: &str, chunk: &str) -> Option<PendingPrompt> {
        let mut lock = self.sessions.write().await;
        if let Some(active) = lock.get_mut(session_id) {
            active.buffer.append(chunk);
            active.session.updated_at = Utc::now();
            active.session.recent_output = active.buffer.get_tail(30);

            // Broadcast delta
            let _ = self.events_tx.send(AgentEvent::OutputDelta {
                session_id: session_id.to_string(),
                chunk: chunk.to_string(),
            });

            // If not currently waiting for input, test if prompt is detected
            if active.session.status != AgentStatus::WaitingForInput {
                let tail_text = active.buffer.get_tail(30).join("\n");
                if let Some(prompt) = self.detector.detect_prompt(session_id, &tail_text) {
                    active.session.status = AgentStatus::WaitingForInput;
                    active.session.pending_prompt = Some(prompt.clone());

                    let _ = self.events_tx.send(AgentEvent::PromptRequired {
                        session_id: session_id.to_string(),
                        prompt: prompt.clone(),
                    });
                    let _ = self
                        .events_tx
                        .send(AgentEvent::SessionUpdated(active.session.clone()));

                    info!(
                        "Prompt detected for session {}: {:?}",
                        session_id, prompt.prompt_text
                    );
                    return Some(prompt);
                }
            }
        }
        None
    }

    pub async fn submit_input(&self, session_id: &str, req: UserInputRequest) -> bool {
        let mut lock = self.sessions.write().await;
        if let Some(active) = lock.get_mut(session_id) {
            let prompt_id = active
                .session
                .pending_prompt
                .as_ref()
                .map(|p| p.id.clone())
                .unwrap_or_else(|| Uuid::new_v4().to_string());

            // Check if there is an MCP tool responder waiting
            if let Some(sender) = active.mcp_pending_responders.remove(&prompt_id) {
                let _ = sender.send(req.input.clone());
            }

            // If there is an active PTY input sender, send the keystrokes to the PTY
            if let Some(ref sender) = active.input_sender {
                if let Err(e) = sender.send(req.clone()).await {
                    warn!("Failed to forward input to PTY runner: {}", e);
                }
            }

            active.session.status = AgentStatus::Running;
            active.session.pending_prompt = None;
            active.session.updated_at = Utc::now();

            let _ = self.events_tx.send(AgentEvent::PromptResolved {
                session_id: session_id.to_string(),
                prompt_id,
                input: req.input,
                press_enter: req.press_enter,
            });
            let _ = self
                .events_tx
                .send(AgentEvent::SessionUpdated(active.session.clone()));
            return true;
        }
        false
    }

    pub async fn register_prompt(
        &self,
        session_id: &str,
        prompt_text: String,
        prompt_type: PromptType,
        choices: Vec<PromptChoice>,
    ) -> Option<PendingPrompt> {
        self.register_prompt_with_id(session_id, None, prompt_text, prompt_type, choices)
            .await
    }

    /// Like `register_prompt`, but lets the caller fix the prompt id up front
    /// (MCP needs it to route the answer back to the waiting tool call).
    pub async fn register_prompt_with_id(
        &self,
        session_id: &str,
        prompt_id: Option<String>,
        prompt_text: String,
        prompt_type: PromptType,
        choices: Vec<PromptChoice>,
    ) -> Option<PendingPrompt> {
        let mut lock = self.sessions.write().await;
        if let Some(active) = lock.get_mut(session_id) {
            let mut prompt = PendingPrompt::new(session_id, prompt_text, prompt_type, choices);
            if let Some(id) = prompt_id {
                prompt.id = id;
            }

            active.session.status = AgentStatus::WaitingForInput;
            active.session.pending_prompt = Some(prompt.clone());
            active.session.updated_at = Utc::now();

            let _ = self.events_tx.send(AgentEvent::PromptRequired {
                session_id: session_id.to_string(),
                prompt: prompt.clone(),
            });
            let _ = self
                .events_tx
                .send(AgentEvent::SessionUpdated(active.session.clone()));

            Some(prompt)
        } else {
            None
        }
    }

    pub async fn terminate_session(&self, session_id: &str, exit_code: Option<i32>) {
        let mut lock = self.sessions.write().await;
        if let Some(active) = lock.get_mut(session_id) {
            active.session.status = if exit_code == Some(0) {
                AgentStatus::Completed
            } else {
                AgentStatus::Failed
            };
            active.session.pending_prompt = None;
            active.session.updated_at = Utc::now();

            let _ = self.events_tx.send(AgentEvent::SessionTerminated {
                session_id: session_id.to_string(),
                exit_code,
            });
            let _ = self
                .events_tx
                .send(AgentEvent::SessionUpdated(active.session.clone()));
            info!(
                "Session {} terminated with exit code {:?}",
                session_id, exit_code
            );
        }
    }

    pub async fn get_all_sessions(&self) -> Vec<AgentSession> {
        let lock = self.sessions.read().await;
        let mut list: Vec<AgentSession> = lock.values().map(|a| a.session.clone()).collect();
        list.sort_by_key(|a| std::cmp::Reverse(a.updated_at));
        list
    }

    pub async fn get_session(&self, session_id: &str) -> Option<AgentSession> {
        let lock = self.sessions.read().await;
        lock.get(session_id).map(|a| a.session.clone())
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
