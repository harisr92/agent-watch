use aw_core::models::{
    AgentEvent, AgentSession, AgentStatus, PromptType, RegisterSessionRequest, UserInputRequest,
};
use aw_daemon::{create_router, AppState};
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

#[tokio::test]
async fn test_health_check() {
    let state = AppState::new();
    let app = create_router(state);

    let req = Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(req).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"OK");
}

#[tokio::test]
async fn test_session_lifecycle_and_prompt_resolution() {
    let state = AppState::new();
    let app = create_router(state);

    // 1. Initially empty sessions
    let req = Request::builder()
        .uri("/api/v1/sessions")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let sessions: Vec<AgentSession> = serde_json::from_slice(&body).unwrap();
    assert!(sessions.is_empty());

    // 2. Register session
    let reg_req = RegisterSessionRequest {
        id: "sess-lifecycle".into(),
        name: "test-agent".into(),
        command: "python3".into(),
        args: vec!["agent.py".into()],
        cwd: "/tmp".into(),
        pid: Some(12345),
    };

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/sessions/register")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&reg_req).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let created: AgentSession = serde_json::from_slice(&body).unwrap();
    assert_eq!(created.id, "sess-lifecycle");
    assert_eq!(created.status, AgentStatus::Running);

    // 3. Append output with an interactive prompt [y/N]
    let chunk_payload = json!({
        "chunk": "Task requires confirmation.\nRun command rm -rf build? [y/N]: "
    });

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/sessions/sess-lifecycle/output")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&chunk_payload).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 4. Verify session state is now WaitingForInput with prompt
    let req = Request::builder()
        .uri("/api/v1/sessions/sess-lifecycle")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let updated: AgentSession = serde_json::from_slice(&body).unwrap();
    assert_eq!(updated.status, AgentStatus::WaitingForInput);
    let prompt = updated.pending_prompt.expect("Should have pending prompt");
    assert!(prompt.prompt_text.contains("[y/N]"));
    assert_eq!(prompt.prompt_type, PromptType::Confirm);

    // 5. Submit input "y"
    let input_req = UserInputRequest {
        input: "y".into(),
        press_enter: true,
    };
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/sessions/sess-lifecycle/input")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&input_req).unwrap()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 6. Verify status returned to Running and prompt cleared
    let req = Request::builder()
        .uri("/api/v1/sessions/sess-lifecycle")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let resumed: AgentSession = serde_json::from_slice(&body).unwrap();
    assert_eq!(resumed.status, AgentStatus::Running);
    assert!(resumed.pending_prompt.is_none());

    // 7. Terminate session
    let term_payload = json!({ "exit_code": 0 });
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/sessions/sess-lifecycle/terminate")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&term_payload).unwrap()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 8. Verify status is Completed
    let req = Request::builder()
        .uri("/api/v1/sessions/sess-lifecycle")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let finished: AgentSession = serde_json::from_slice(&body).unwrap();
    assert_eq!(finished.status, AgentStatus::Completed);
}

#[tokio::test]
async fn test_mcp_tools_list_and_report_status() {
    let state = AppState::new();
    let app = create_router(state);

    // 1. tools/list
    let list_req = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list"
    });

    let req = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&list_req).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let val: Value = serde_json::from_slice(&body).unwrap();
    let tools = val["result"]["tools"]
        .as_array()
        .expect("Should have tools array");
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0]["name"], "request_user_input");
    assert_eq!(tools[1]["name"], "report_status");

    // 2. tools/call -> report_status
    let call_req = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "report_status",
            "arguments": {
                "session_name": "ClaudeCode",
                "message": "Indexing codebase files..."
            }
        }
    });

    let req = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&call_req).unwrap()))
        .unwrap();

    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let val: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(val["result"]["content"][0]["text"], "Status updated");
}

async fn post_json(app: &axum::Router, uri: &str, body: Value) -> (StatusCode, Value) {
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header("Content-Type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn test_menu_prompt_exposes_choices_and_keeps_press_enter() {
    let state = AppState::new();
    let mut events = state.events_tx.subscribe();
    let app = create_router(state.clone());

    post_json(
        &app,
        "/api/v1/sessions/register",
        json!({ "id": "menu", "name": "claude", "command": "claude", "args": [], "cwd": "/tmp", "pid": null }),
    )
    .await;
    post_json(
        &app,
        "/api/v1/sessions/menu/output",
        json!({ "chunk": "Do you want to make this edit?\n\u{276f} 1. Yes\n  2. Yes, allow all edits\n  3. No\n" }),
    )
    .await;

    let session = state.get_session("menu").await.expect("session");
    let prompt = session.pending_prompt.expect("prompt detected");
    assert_eq!(prompt.prompt_type, PromptType::Select);
    let labels: Vec<_> = prompt.choices.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(labels, vec!["Yes", "Yes, allow all edits", "No"]);
    assert!(!prompt.press_enter);

    let (status, _) = post_json(
        &app,
        "/api/v1/sessions/menu/input",
        json!({ "input": "2", "press_enter": false }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    loop {
        match events.recv().await.unwrap() {
            AgentEvent::PromptResolved {
                input, press_enter, ..
            } => {
                assert_eq!(input, "2");
                assert!(!press_enter, "press_enter must reach the PTY runner");
                break;
            }
            _ => continue,
        }
    }
}

#[tokio::test]
async fn test_mock_menu_agent_output_communicates_entire_message() {
    let state = AppState::new();
    let app = create_router(state.clone());

    post_json(
        &app,
        "/api/v1/sessions/register",
        json!({ "id": "menu-agent", "name": "mock_menu_agent.py", "command": "python3", "args": ["scripts/mock_menu_agent.py"], "cwd": "/tmp", "pid": null }),
    )
    .await;

    // Simulate chunk 1 from mock_menu_agent.py
    post_json(
        &app,
        "/api/v1/sessions/menu-agent/output",
        json!({ "chunk": "\x1b[1;35m[MenuAgent]\x1b[0m Planning database migration...\r\n" }),
    )
    .await;

    // Simulate chunk 2 from mock_menu_agent.py (options and ask prompt)
    post_json(
        &app,
        "/api/v1/sessions/menu-agent/output",
        json!({ "chunk": "How should I apply the migration?\r\n  1) Apply now\r\n  2) Dry run first\r\n  3) Skip this step\r\nEnter your choice [1-3]: " }),
    )
    .await;

    let session = state.get_session("menu-agent").await.expect("session");
    let prompt = session.pending_prompt.expect("prompt detected");
    assert_eq!(prompt.prompt_type, PromptType::Select);
    assert_eq!(
        prompt.prompt_text,
        "[MenuAgent] Planning database migration...\nHow should I apply the migration?\n  1) Apply now\n  2) Dry run first\n  3) Skip this step\nEnter your choice [1-3]:"
    );
    let labels: Vec<_> = prompt.choices.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(labels, vec!["Apply now", "Dry run first", "Skip this step"]);
    assert_eq!(prompt.options, vec!["1", "2", "3"]);
    assert!(prompt.press_enter);

    // 1. User answers "1"
    post_json(
        &app,
        "/api/v1/sessions/menu-agent/input",
        json!({ "input": "1", "press_enter": true }),
    )
    .await;

    // Terminal echoes input and next output
    post_json(
        &app,
        "/api/v1/sessions/menu-agent/output",
        json!({ "chunk": "1\r\n\x1b[1;32m[MenuAgent]\x1b[0m You picked option '1'\r\n" }),
    )
    .await;

    // Verify session status is Running, no phantom prompt
    let session = state.get_session("menu-agent").await.unwrap();
    assert_eq!(session.status, AgentStatus::Running);
    assert!(session.pending_prompt.is_none());

    // 2. Mock agent asks for API key (getpass)
    post_json(
        &app,
        "/api/v1/sessions/menu-agent/output",
        json!({ "chunk": "Enter your API key: " }),
    )
    .await;

    let session = state.get_session("menu-agent").await.unwrap();
    let prompt2 = session.pending_prompt.expect("api key prompt detected");
    assert_eq!(prompt2.prompt_type, PromptType::Secret);
    assert_eq!(prompt2.prompt_text, "Enter your API key:");

    // User submits API key
    post_json(
        &app,
        "/api/v1/sessions/menu-agent/input",
        json!({ "input": "sk-secret123", "press_enter": true }),
    )
    .await;

    // Agent prints next step and wait for Enter
    post_json(
        &app,
        "/api/v1/sessions/menu-agent/output",
        json!({ "chunk": "\r\n\x1b[1;32m[MenuAgent]\x1b[0m Got a key of 12 characters\r\nMigration plan ready. Press Enter to continue..." }),
    )
    .await;

    let session = state.get_session("menu-agent").await.unwrap();
    let prompt3 = session.pending_prompt.expect("acknowledge prompt detected");
    assert_eq!(prompt3.prompt_type, PromptType::Acknowledge);

    // User submits Enter
    post_json(
        &app,
        "/api/v1/sessions/menu-agent/input",
        json!({ "input": "", "press_enter": true }),
    )
    .await;

    // Agent finishes
    post_json(
        &app,
        "/api/v1/sessions/menu-agent/output",
        json!({ "chunk": "\r\n\x1b[1;32m[MenuAgent]\x1b[0m Done.\r\n" }),
    )
    .await;
    post_json(
        &app,
        "/api/v1/sessions/menu-agent/terminate",
        json!({ "exit_code": 0 }),
    )
    .await;

    let session = state.get_session("menu-agent").await.unwrap();
    assert_eq!(session.status, AgentStatus::Completed);
    assert!(session.pending_prompt.is_none());

    // NOW: Run it a SECOND time (new session menu-agent-2)
    post_json(
        &app,
        "/api/v1/sessions/register",
        json!({ "id": "menu-agent-2", "name": "mock_menu_agent.py", "command": "python3", "args": ["scripts/mock_menu_agent.py"], "cwd": "/tmp", "pid": null }),
    )
    .await;

    post_json(
        &app,
        "/api/v1/sessions/menu-agent-2/output",
        json!({ "chunk": "\x1b[1;35m[MenuAgent]\x1b[0m Planning database migration...\r\n" }),
    )
    .await;
    post_json(
        &app,
        "/api/v1/sessions/menu-agent-2/output",
        json!({ "chunk": "How should I apply the migration?\r\n  1) Apply now\r\n  2) Dry run first\r\n  3) Skip this step\r\nEnter your choice [1-3]: " }),
    )
    .await;

    let session2 = state.get_session("menu-agent-2").await.expect("session 2");
    let prompt_run2 = session2.pending_prompt.expect("prompt detected on run 2");
    assert_eq!(prompt_run2.prompt_type, PromptType::Select);
    assert_eq!(
        prompt_run2.prompt_text,
        "[MenuAgent] Planning database migration...\nHow should I apply the migration?\n  1) Apply now\n  2) Dry run first\n  3) Skip this step\nEnter your choice [1-3]:"
    );
}

#[tokio::test]
async fn test_menu_streamed_line_by_line_shows_all_options() {
    // A real PTY delivers print() output one line at a time; the question line alone
    // already looks like a prompt, so the pending prompt must be refined as options arrive.
    let state = AppState::new();
    let app = create_router(state.clone());

    post_json(
        &app,
        "/api/v1/sessions/register",
        json!({ "id": "menu-lines", "name": "mock_menu_agent.py", "command": "python3", "args": ["scripts/mock_menu_agent.py"], "cwd": "/tmp", "pid": null }),
    )
    .await;

    for chunk in [
        "\x1b[1;35m[MenuAgent]\x1b[0m Planning database migration...\r\n",
        "How should I apply the migration?\r\n",
        "  1) Apply now\r\n",
        "  2) Dry run first\r\n",
        "  3) Skip this step\r\n",
        "Enter your choice [1-3]: ",
    ] {
        post_json(
            &app,
            "/api/v1/sessions/menu-lines/output",
            json!({ "chunk": chunk }),
        )
        .await;
    }

    let session = state.get_session("menu-lines").await.expect("session");
    assert_eq!(session.status, AgentStatus::WaitingForInput);
    let prompt = session.pending_prompt.expect("prompt detected");
    assert_eq!(prompt.prompt_type, PromptType::Select);
    assert_eq!(
        prompt.prompt_text,
        "[MenuAgent] Planning database migration...\nHow should I apply the migration?\n  1) Apply now\n  2) Dry run first\n  3) Skip this step\nEnter your choice [1-3]:"
    );
    assert_eq!(prompt.options, vec!["1", "2", "3"]);
    let prompt_id = prompt.id;

    // Noise while waiting (e.g. a spinner redraw) must not remove the pending prompt.
    post_json(
        &app,
        "/api/v1/sessions/menu-lines/output",
        json!({ "chunk": "\r\n\u{280b} waiting for input\r\n" }),
    )
    .await;
    let session = state.get_session("menu-lines").await.expect("session");
    assert_eq!(session.status, AgentStatus::WaitingForInput);
    let prompt = session.pending_prompt.expect("prompt kept");
    assert_eq!(prompt.id, prompt_id);
    assert_eq!(prompt.options, vec!["1", "2", "3"]);
}

#[tokio::test]
async fn test_registered_prompt_with_options_is_select() {
    let state = AppState::new();
    let app = create_router(state.clone());
    post_json(
        &app,
        "/api/v1/sessions/register",
        json!({ "id": "s", "name": "n", "command": "c", "args": [], "cwd": "/tmp", "pid": null }),
    )
    .await;

    let (_, p) = post_json(
        &app,
        "/api/v1/sessions/s/prompt",
        json!({ "prompt_text": "Deploy where?", "options": ["staging", "production"] }),
    )
    .await;
    assert_eq!(p["prompt_type"], "select");
    assert_eq!(p["choices"][1]["label"], "production");

    let (_, p) = post_json(
        &app,
        "/api/v1/sessions/s/prompt",
        json!({ "prompt_text": "Ship it?" }),
    )
    .await;
    assert_eq!(p["prompt_type"], "confirm");
    assert_eq!(p["options"], json!(["y", "n"]));
}

#[tokio::test]
async fn test_mcp_request_user_input_returns_chosen_option() {
    let state = AppState::new();
    let app = create_router(state.clone());

    let call = json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": {
            "name": "request_user_input",
            "arguments": {
                "prompt": "Which environment?",
                "choices": [
                    { "label": "Staging", "value": "stg" },
                    { "label": "Production", "value": "prod" }
                ]
            }
        }
    });
    let app_clone = app.clone();
    let pending = tokio::spawn(async move { post_json(&app_clone, "/mcp", call).await });

    // Wait for the MCP session to show up, then answer it like the menu bar would.
    let session = loop {
        let waiting = state
            .get_all_sessions()
            .await
            .into_iter()
            .find(|s| s.pending_prompt.is_some());
        if let Some(s) = waiting {
            break s;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    };
    let prompt = session.pending_prompt.unwrap();
    assert_eq!(prompt.prompt_type, PromptType::Select);
    assert_eq!(prompt.choices[1].label, "Production");

    state
        .submit_input(
            &session.id,
            UserInputRequest {
                input: "prod".into(),
                press_enter: true,
            },
        )
        .await;

    let (_, val) = tokio::time::timeout(std::time::Duration::from_secs(2), pending)
        .await
        .expect("MCP call should return once answered")
        .unwrap();
    assert_eq!(val["result"]["content"][0]["text"], "prod");
}

#[tokio::test]
async fn test_clear_history_removes_only_finished_sessions() {
    let state = AppState::new();
    let app = create_router(state.clone());
    let mut rx = state.events_tx.subscribe();

    for id in ["done", "failed", "live"] {
        state
            .register_session(
                RegisterSessionRequest {
                    id: id.into(),
                    name: id.into(),
                    command: "echo".into(),
                    args: vec![],
                    cwd: ".".into(),
                    pid: None,
                },
                None,
            )
            .await;
    }
    state.terminate_session("done", Some(0)).await;
    state.terminate_session("failed", Some(1)).await;

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/sessions/history/clear")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let mut removed: Vec<String> = serde_json::from_value(json["removed"].clone()).unwrap();
    removed.sort();
    assert_eq!(removed, vec!["done".to_string(), "failed".to_string()]);

    let remaining = state.get_all_sessions().await;
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, "live");

    let mut saw_cleared = false;
    while let Ok(event) = rx.try_recv() {
        if let AgentEvent::HistoryCleared { session_ids } = event {
            assert_eq!(session_ids.len(), 2);
            saw_cleared = true;
        }
    }
    assert!(saw_cleared, "expected a history_cleared event");
}

#[tokio::test]
async fn test_busy_agent_diff_output_raises_no_prompt() {
    // Claude Code in auto mode printing an edit: no input is needed, so no prompt (and no
    // notification) may be raised, even though a diff line contains a '?'.
    let state = AppState::new();
    let mut events = state.events_tx.subscribe();
    let app = create_router(state.clone());
    post_json(
        &app,
        "/api/v1/sessions/register",
        json!({ "id": "busy", "name": "claude", "command": "claude", "args": [], "cwd": "/tmp", "pid": null }),
    )
    .await;
    post_json(
        &app,
        "/api/v1/sessions/busy/output",
        json!({ "chunk": "\u{23fa} Update(src/Picker.test.tsx)\r\n       185      it(\"focuses the picker?\", () => {\r\n       186 -      expect(screen.getByRole(\"combobox\", { name: /Pick one/ })).toHaveFocus();\r\n       187 +      expect(screen.getByRole(\"combobox\", { name: \"Pick one\" })).toHaveFocus();\r\n\r\n\u{273b} Crafting\u{2026} (8s \u{b7} esc to interrupt)\r\n\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\r\n\u{276f} \r\n\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\r\n" }),
    )
    .await;

    let session = state.get_session("busy").await.expect("session");
    assert_eq!(session.status, AgentStatus::Running);
    assert!(session.pending_prompt.is_none());
    while let Ok(event) = events.try_recv() {
        assert!(
            !matches!(event, AgentEvent::PromptRequired { .. }),
            "unexpected prompt: {event:?}"
        );
    }
}

#[tokio::test]
async fn test_auto_detected_prompt_cleared_when_agent_gets_busy() {
    // A prompt answered in the terminal: the agent goes back to work, so the pending
    // auto-detected prompt must not linger in the menu bar.
    let state = AppState::new();
    let app = create_router(state.clone());
    post_json(
        &app,
        "/api/v1/sessions/register",
        json!({ "id": "answered", "name": "agent", "command": "agent", "args": [], "cwd": "/tmp", "pid": null }),
    )
    .await;
    post_json(
        &app,
        "/api/v1/sessions/answered/output",
        json!({ "chunk": "Overwrite existing file? [y/n/a/q] " }),
    )
    .await;
    let session = state.get_session("answered").await.expect("session");
    assert_eq!(session.status, AgentStatus::WaitingForInput);

    post_json(
        &app,
        "/api/v1/sessions/answered/output",
        json!({ "chunk": "y\r\nWorking (2s \u{2022} esc to interrupt)\r\n" }),
    )
    .await;
    let session = state.get_session("answered").await.expect("session");
    assert_eq!(session.status, AgentStatus::Running);
    assert!(session.pending_prompt.is_none());
}

/// Terminal title as Claude Code sets it (OSC 0, BEL-terminated).
fn title(text: &str) -> String {
    format!("\x1b]0;{text}\x07")
}

const RULE: &str = "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}";

fn idle_box() -> String {
    format!("\r\n{RULE}\r\n\u{276f} \r\n{RULE}\r\n")
}

async fn register(app: &axum::Router, id: &str) {
    post_json(
        app,
        "/api/v1/sessions/register",
        json!({ "id": id, "name": "claude", "command": "claude", "args": [], "cwd": "/tmp", "pid": null }),
    )
    .await;
}

async fn output(app: &axum::Router, id: &str, chunk: &str) {
    post_json(
        app,
        &format!("/api/v1/sessions/{id}/output"),
        json!({ "chunk": chunk }),
    )
    .await;
}

fn session_updates(events: &mut tokio::sync::broadcast::Receiver<AgentEvent>) -> Vec<AgentSession> {
    let mut out = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let AgentEvent::SessionUpdated(s) = event {
            out.push(s);
        }
    }
    out
}

#[tokio::test]
async fn test_activity_follows_title_working_then_done() {
    use aw_core::models::AgentActivity;
    let state = AppState::new();
    let app = create_router(state.clone());
    register(&app, "turn").await;

    // Startup: stopped glyph, idle box. Not "done": the agent never worked.
    output(
        &app,
        "turn",
        &format!("{}{}", title("\u{2733} Claude Code"), idle_box()),
    )
    .await;
    let s = state.get_session("turn").await.unwrap();
    assert_eq!(s.activity, None);

    // Working: the spinner title retracts the idle-box prompt even without "esc to interrupt".
    output(
        &app,
        "turn",
        &format!("{}\r\nthinking\r\n", title("\u{25d0} Ok reply")),
    )
    .await;
    let s = state.get_session("turn").await.unwrap();
    assert_eq!(s.activity, Some(AgentActivity::Working));
    assert_eq!(s.task_title.as_deref(), Some("Ok reply"));
    assert_eq!(s.status, AgentStatus::Running);
    assert!(s.pending_prompt.is_none());

    // Done: stopped glyph with only the idle box on screen.
    output(
        &app,
        "turn",
        &format!("{}ok\r\n{}", title("\u{2733} Ok reply"), idle_box()),
    )
    .await;
    let s = state.get_session("turn").await.unwrap();
    assert_eq!(s.activity, Some(AgentActivity::Done));
    assert_eq!(s.task_title.as_deref(), Some("Ok reply"));
}

#[tokio::test]
async fn test_activity_waiting_on_permission_menu() {
    use aw_core::models::AgentActivity;
    let state = AppState::new();
    let app = create_router(state.clone());
    register(&app, "perm").await;

    output(&app, "perm", &title("\u{25d0} Create file")).await;
    output(
        &app,
        "perm",
        &format!(
            "{}Do you want to create hello.txt?\r\n\u{276f} 1. Yes\r\n  2. Yes, and allow all edits\r\n  3. No\r\n\r\nEsc to cancel\r\n",
            title("\u{2733} Create file")
        ),
    )
    .await;
    let s = state.get_session("perm").await.unwrap();
    assert_eq!(s.activity, Some(AgentActivity::Waiting));
    assert_eq!(s.status, AgentStatus::WaitingForInput);
    assert_eq!(s.pending_prompt.unwrap().options, vec!["1", "2", "3"]);
}

#[tokio::test]
async fn test_spinner_frames_do_not_spam_session_updates() {
    let state = AppState::new();
    let mut events = state.events_tx.subscribe();
    let app = create_router(state.clone());
    register(&app, "spin").await;
    output(&app, "spin", &title("\u{25d0} Task")).await;
    let _ = session_updates(&mut events);

    for glyph in ["\u{25d1}", "\u{25d0}", "\u{25d1}", "\u{25d0}"] {
        output(
            &app,
            "spin",
            &format!("{}.", title(&format!("{glyph} Task"))),
        )
        .await;
    }
    assert!(session_updates(&mut events).is_empty());

    // A new task name is worth an update.
    output(&app, "spin", &title("\u{25d1} Renamed task")).await;
    let updates = session_updates(&mut events);
    assert_eq!(updates.len(), 1);
    assert_eq!(updates[0].task_title.as_deref(), Some("Renamed task"));
}

#[tokio::test]
async fn test_no_title_signal_leaves_activity_unknown() {
    let state = AppState::new();
    let app = create_router(state.clone());
    register(&app, "plain").await;
    output(
        &app,
        "plain",
        "\x1b]0;bash\x07Overwrite existing file? [y/n] ",
    )
    .await;
    let s = state.get_session("plain").await.unwrap();
    assert_eq!(s.activity, None);
    assert_eq!(s.task_title, None);
    // Screen heuristics still work as before.
    assert_eq!(s.status, AgentStatus::WaitingForInput);
}

#[tokio::test]
async fn test_stopped_title_before_menu_is_drawn_is_not_done() {
    // Claude Code sets the stopped title a moment before drawing a permission menu; the gap
    // must not read as "done" (a bogus done notification right before every approval).
    use aw_core::models::AgentActivity;
    let state = AppState::new();
    let app = create_router(state.clone());
    register(&app, "race").await;

    output(&app, "race", &title("\u{25d0} Create file")).await;
    output(
        &app,
        "race",
        &format!(
            "{}Create file hello.txt\r\n  1  hi\r\n",
            title("\u{2733} Create file")
        ),
    )
    .await;
    let s = state.get_session("race").await.unwrap();
    assert_eq!(s.activity, Some(AgentActivity::Working));

    output(
        &app,
        "race",
        "Do you want to create hello.txt?\r\n\u{276f} 1. Yes\r\n  2. No\r\n\r\nEsc to cancel\r\n",
    )
    .await;
    let s = state.get_session("race").await.unwrap();
    assert_eq!(s.activity, Some(AgentActivity::Waiting));
}
