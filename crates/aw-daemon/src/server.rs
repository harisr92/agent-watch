use crate::state::AppState;
use aw_core::models::{
    AgentSession, PromptChoice, PromptType, RegisterSessionRequest, UserInputRequest,
};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::sync::oneshot;
use tower_http::cors::CorsLayer;
use tracing::info;

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/api/v1/sessions", get(list_sessions))
        .route("/api/v1/sessions/register", post(register_session))
        .route("/api/v1/sessions/:id", get(get_session))
        .route("/api/v1/sessions/:id/output", post(append_output))
        .route("/api/v1/sessions/:id/prompt", post(register_prompt))
        .route("/api/v1/sessions/:id/input", post(submit_input))
        .route("/api/v1/sessions/:id/terminate", post(terminate_session))
        .route("/api/v1/events", get(ws_handler))
        .route("/mcp", post(mcp_handler))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

pub async fn run_server(state: AppState, addr: SocketAddr) -> anyhow::Result<()> {
    let app = create_router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("Agent Watch Daemon running on http://{}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health_check() -> &'static str {
    "OK"
}

async fn list_sessions(State(state): State<AppState>) -> Json<Vec<AgentSession>> {
    Json(state.get_all_sessions().await)
}

async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<AgentSession>, StatusCode> {
    match state.get_session(&id).await {
        Some(s) => Ok(Json(s)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn register_session(
    State(state): State<AppState>,
    Json(req): Json<RegisterSessionRequest>,
) -> Json<AgentSession> {
    let session = state.register_session(req, None).await;
    Json(session)
}

#[derive(Deserialize)]
struct OutputPayload {
    chunk: String,
}

async fn append_output(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<OutputPayload>,
) -> StatusCode {
    state.append_output(&id, &payload.chunk).await;
    StatusCode::OK
}

#[derive(Deserialize)]
struct PromptPayload {
    prompt_text: String,
    prompt_type: Option<PromptType>,
    options: Option<Vec<String>>,
    choices: Option<Vec<PromptChoice>>,
}

/// Resolves the prompt type and choices from loosely specified input.
///
/// - explicit `choices` (label + value) win over plain `options`
/// - with no choices at all, the prompt defaults to a yes/no confirmation
/// - with choices but no type, it is a Select (or Confirm when the choices are just y/n)
pub(crate) fn resolve_prompt_shape(
    prompt_type: Option<PromptType>,
    options: Option<Vec<String>>,
    choices: Option<Vec<PromptChoice>>,
) -> (PromptType, Vec<PromptChoice>) {
    let choices = choices
        .filter(|c| !c.is_empty())
        .or_else(|| {
            options.filter(|o| !o.is_empty()).map(|o| {
                o.into_iter()
                    .map(|v| PromptChoice::new(v.clone(), v))
                    .collect()
            })
        })
        .unwrap_or_default();

    let is_yes_no = |c: &[PromptChoice]| {
        c.len() == 2 && c[0].value.eq_ignore_ascii_case("y") && c[1].value.eq_ignore_ascii_case("n")
    };

    match (prompt_type, choices.is_empty()) {
        (Some(PromptType::Confirm), _) | (None, true) => (
            PromptType::Confirm,
            vec![PromptChoice::new("Yes", "y"), PromptChoice::new("No", "n")],
        ),
        (Some(t), _) => (t, choices),
        (None, false) if is_yes_no(&choices) => (
            PromptType::Confirm,
            vec![PromptChoice::new("Yes", "y"), PromptChoice::new("No", "n")],
        ),
        (None, false) => (PromptType::Select, choices),
    }
}

async fn register_prompt(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<PromptPayload>,
) -> Result<Json<Value>, StatusCode> {
    let (prompt_type, choices) =
        resolve_prompt_shape(payload.prompt_type, payload.options, payload.choices);
    match state
        .register_prompt(&id, payload.prompt_text, prompt_type, choices)
        .await
    {
        Some(p) => Ok(Json(json!(p))),
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn submit_input(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UserInputRequest>,
) -> StatusCode {
    if state.submit_input(&id, req).await {
        StatusCode::OK
    } else {
        StatusCode::NOT_FOUND
    }
}

#[derive(Deserialize)]
struct TerminatePayload {
    exit_code: Option<i32>,
}

async fn terminate_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<TerminatePayload>,
) -> StatusCode {
    state.terminate_session(&id, payload.exit_code).await;
    StatusCode::OK
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

async fn handle_ws(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();

    // On connection, send initial state snapshot
    let initial_sessions = state.get_all_sessions().await;
    if let Ok(json_str) = serde_json::to_string(&json!({
        "type": "init",
        "sessions": initial_sessions,
    })) {
        let _ = sender.send(Message::Text(json_str)).await;
    }

    let mut rx = state.events_tx.subscribe();

    // Task to forward broadcast events to the websocket client
    let mut send_task = tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            if let Ok(json_str) = serde_json::to_string(&event) {
                if sender.send(Message::Text(json_str)).await.is_err() {
                    break;
                }
            }
        }
    });

    // Task to receive commands from the client (e.g. submit input via websocket)
    let state_clone = state.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let Message::Text(text) = msg {
                if let Ok(val) = serde_json::from_str::<Value>(&text) {
                    if val.get("type").and_then(|v| v.as_str()) == Some("submit_input") {
                        if let (Some(sess_id), Some(input)) = (
                            val.get("session_id").and_then(|v| v.as_str()),
                            val.get("input").and_then(|v| v.as_str()),
                        ) {
                            let press_enter = val
                                .get("press_enter")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(true);
                            state_clone
                                .submit_input(
                                    sess_id,
                                    UserInputRequest {
                                        input: input.to_string(),
                                        press_enter,
                                    },
                                )
                                .await;
                        }
                    }
                }
            }
        }
    });

    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    };
}

// MCP (Model Context Protocol) JSON-RPC handler
async fn mcp_handler(State(state): State<AppState>, Json(payload): Json<Value>) -> Json<Value> {
    let id = payload.get("id").cloned().unwrap_or(Value::Null);
    let method = payload.get("method").and_then(|m| m.as_str()).unwrap_or("");

    match method {
        "tools/list" => Json(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "tools": [
                    {
                        "name": "request_user_input",
                        "description": "Ask the user a question via the macOS menu bar and block until they answer. Supports yes/no confirmations, multiple choice, free text, secrets and acknowledgements. Returns the chosen option's value or the typed text.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "prompt": { "type": "string", "description": "Question to display to the user" },
                                "session_name": { "type": "string", "description": "Optional name of the agent or task requesting input" },
                                "prompt_type": { "type": "string", "enum": ["confirm", "select", "text", "secret", "acknowledge"], "description": "Kind of answer expected. Defaults to 'select' when options are given, otherwise 'confirm'." },
                                "options": { "type": "array", "items": { "type": "string" }, "description": "Choices shown as buttons; the chosen string is returned. E.g. ['staging', 'production']" },
                                "choices": { "type": "array", "items": { "type": "object", "properties": { "label": { "type": "string" }, "value": { "type": "string" } }, "required": ["label", "value"] }, "description": "Choices with a display label distinct from the returned value. Takes precedence over options." }
                            },
                            "required": ["prompt"]
                        }
                    },
                    {
                        "name": "report_status",
                        "description": "Report current status or activity message of the agent to the macOS menu bar.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "session_name": { "type": "string" },
                                "status": { "type": "string" },
                                "message": { "type": "string" }
                            },
                            "required": ["message"]
                        }
                    }
                ]
            }
        })),
        "tools/call" => {
            let params = payload.get("params");
            let tool_name = params
                .and_then(|p| p.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("");
            let arguments = params
                .and_then(|p| p.get("arguments"))
                .cloned()
                .unwrap_or(Value::Null);

            if tool_name == "request_user_input" {
                let prompt_text = arguments
                    .get("prompt")
                    .and_then(|p| p.as_str())
                    .unwrap_or("Input required")
                    .to_string();
                let session_name = arguments
                    .get("session_name")
                    .and_then(|s| s.as_str())
                    .unwrap_or("MCP Agent")
                    .to_string();
                let options = arguments
                    .get("options")
                    .and_then(|o| o.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    });
                let choices = arguments
                    .get("choices")
                    .and_then(|c| serde_json::from_value::<Vec<PromptChoice>>(c.clone()).ok());
                let prompt_type = arguments
                    .get("prompt_type")
                    .and_then(|t| serde_json::from_value::<PromptType>(t.clone()).ok());
                let (prompt_type, choices) = resolve_prompt_shape(prompt_type, options, choices);

                let session_id = format!("mcp-{}", uuid::Uuid::new_v4());
                let prompt_id = uuid::Uuid::new_v4().to_string();

                // Register session & prompt
                let _ = state
                    .register_session(
                        RegisterSessionRequest {
                            id: session_id.clone(),
                            name: session_name,
                            command: "mcp-agent".into(),
                            args: vec![],
                            cwd: ".".into(),
                            pid: None,
                        },
                        None,
                    )
                    .await;

                let (tx, rx) = oneshot::channel();
                {
                    let mut lock = state.sessions.write().await;
                    if let Some(active) = lock.get_mut(&session_id) {
                        active.mcp_pending_responders.insert(prompt_id.clone(), tx);
                    }
                }

                let _ = state
                    .register_prompt_with_id(
                        &session_id,
                        Some(prompt_id.clone()),
                        prompt_text.clone(),
                        prompt_type,
                        choices,
                    )
                    .await;

                // Wait for response or timeout after 5 minutes
                match tokio::time::timeout(Duration::from_secs(300), rx).await {
                    Ok(Ok(user_input)) => {
                        let _ = state.terminate_session(&session_id, Some(0)).await;
                        Json(json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "content": [
                                    { "type": "text", "text": user_input }
                                ]
                            }
                        }))
                    }
                    _ => {
                        let _ = state.terminate_session(&session_id, Some(1)).await;
                        Json(json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "error": {
                                "code": -32000,
                                "message": "Timed out waiting for user response"
                            }
                        }))
                    }
                }
            } else if tool_name == "report_status" {
                let message = arguments
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("");
                let session_name = arguments
                    .get("session_name")
                    .and_then(|s| s.as_str())
                    .unwrap_or("MCP Agent");
                let session_id = format!("mcp-{}", session_name.to_lowercase().replace(' ', "-"));

                state
                    .append_output(&session_id, &format!("[Status] {}\n", message))
                    .await;

                Json(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "content": [
                            { "type": "text", "text": "Status updated" }
                        ]
                    }
                }))
            } else {
                Json(json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32601,
                        "message": format!("Tool not found: {}", tool_name)
                    }
                }))
            }
        }
        _ => Json(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32601,
                "message": format!("Method not found: {}", method)
            }
        })),
    }
}
