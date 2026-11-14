use aw_core::models::{
    AgentSession, AgentStatus, PromptType, RegisterSessionRequest, UserInputRequest,
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
