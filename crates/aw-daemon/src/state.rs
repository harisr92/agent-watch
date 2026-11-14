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
    pub last_resolved_prompt_text: Option<String>,
    /// True when `pending_prompt` came from output heuristics (not an explicit
    /// registration), so later output may refine or retract it.
    pub prompt_auto_detected: bool,
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
            last_resolved_prompt_text: None,
            prompt_auto_detected: false,
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

            // Agents often print a question line by line, so the first detection can fire on
            // a partial prompt (e.g. just the question, before its options arrive). While an
            // auto-detected prompt is pending, keep re-detecting so it picks up the later lines.
            let waiting = active.session.status == AgentStatus::WaitingForInput;
            if waiting && !active.prompt_auto_detected {
                return None;
            }

            let tail_text = active.buffer.get_tail(30).join("\n");
            let detected = self.detector.detect_prompt(session_id, &tail_text);

            if waiting {
                // Only ever refine a pending prompt, never drop it: spinners or screen redraws
                // while the agent waits must not make a real question vanish.
                let mut prompt = detected?;
                let current = active.session.pending_prompt.as_ref();
                let unchanged = current.is_some_and(|c| {
                    c.prompt_text == prompt.prompt_text
                        && c.prompt_type == prompt.prompt_type
                        && c.options == prompt.options
                });
                if unchanged {
                    return None;
                }
                // Keep the id so the UI treats this as the same prompt, now complete.
                if let Some(c) = current {
                    prompt.id = c.id.clone();
                    prompt.created_at = c.created_at;
                }
                active.session.pending_prompt = Some(prompt.clone());
                info!(
                    "Prompt refined for session {}: {:?}",
                    session_id, prompt.prompt_text
                );
                let _ = self
                    .events_tx
                    .send(AgentEvent::SessionUpdated(active.session.clone()));
                return None;
            }

            match detected {
                Some(prompt)
                    if active.last_resolved_prompt_text.as_deref()
                        != Some(&prompt.prompt_text) =>
                {
                    active.last_resolved_prompt_text = None;
                    active.prompt_auto_detected = true;
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
                Some(_) => {}
                None => active.last_resolved_prompt_text = None,
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

            active.last_resolved_prompt_text = active
                .session
                .pending_prompt
                .as_ref()
                .map(|p| p.prompt_text.clone());

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
            active.prompt_auto_detected = false;
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
            active.prompt_auto_detected = false;
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

    /// Removes every finished session (completed, failed or idle) and returns the removed IDs.
    /// Running and waiting sessions are kept.
    pub async fn clear_history(&self) -> Vec<String> {
        let mut lock = self.sessions.write().await;
        let removed: Vec<String> = lock
            .iter()
            .filter(|(_, a)| {
                matches!(
                    a.session.status,
                    AgentStatus::Completed | AgentStatus::Failed | AgentStatus::Idle
                )
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in &removed {
            lock.remove(id);
        }
        drop(lock);

        if !removed.is_empty() {
            let _ = self.events_tx.send(AgentEvent::HistoryCleared {
                session_ids: removed.clone(),
            });
            info!("Cleared {} finished session(s) from history", removed.len());
        }
        removed
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
