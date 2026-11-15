use anyhow::{Context, Result};
use aw_core::models::{AgentEvent, RegisterSessionRequest};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, size};
use futures_util::{Stream, StreamExt};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{info, warn};

pub struct PtyRunnerConfig {
    pub session_id: String,
    pub session_name: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: String,
    pub daemon_url: String,
    pub ws_url: String,
}

async fn ensure_daemon_running(daemon_url: &str) -> bool {
    let client = reqwest::Client::new();
    let health_url = format!("{}/health", daemon_url);
    if let Ok(resp) = client.get(&health_url).send().await {
        if resp.status().is_success() {
            return true;
        }
    }

    // Auto-spawn daemon in background
    if let Ok(current_exe) = std::env::current_exe() {
        let spawned = std::process::Command::new(current_exe)
            .arg("daemon")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();

        if spawned.is_ok() {
            for _ in 0..15 {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                if let Ok(resp) = client.get(&health_url).send().await {
                    if resp.status().is_success() {
                        return true;
                    }
                }
            }
        }
    }

    false
}

/// Decodes the complete UTF-8 prefix of `buf`, leaving a character split across PTY reads
/// in `buf` for the next read instead of mangling it into U+FFFD.
fn take_complete_utf8(buf: &mut Vec<u8>) -> String {
    let keep_from = match std::str::from_utf8(buf) {
        Ok(_) => buf.len(),
        // Truncated sequence at the end (error_len None): keep it, up to a 3-byte prefix.
        Err(e) if e.error_len().is_none() && buf.len() - e.valid_up_to() < 4 => e.valid_up_to(),
        Err(_) => buf.len(),
    };
    let rest = buf.split_off(keep_from);
    let text = String::from_utf8_lossy(buf).into_owned();
    *buf = rest;
    text
}

/// Upper bound on PTY output held back while the daemon forwarder is busy. Only the tail
/// matters to prompt detection, so older text is dropped first.
const DAEMON_BACKLOG_CAP: usize = 64 * 1024;

/// Puts the controlling terminal in raw mode for its lifetime, restoring it on drop, which
/// also covers early returns and panics.
struct RawModeGuard;

impl RawModeGuard {
    fn enable() -> Self {
        let _ = enable_raw_mode();
        RawModeGuard
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

fn pty_size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// Appends `text` to `backlog`, keeping at most the newest `cap` bytes, cut on a char boundary.
fn push_capped(backlog: &mut String, text: &str, cap: usize) {
    backlog.push_str(text);
    if backlog.len() > cap {
        let mut cut = backlog.len() - cap;
        while !backlog.is_char_boundary(cut) {
            cut += 1;
        }
        backlog.drain(..cut);
    }
}

/// Hands `text` (plus anything held back earlier) to the daemon forwarder without blocking.
/// When the channel is full the text stays in `backlog` for the next attempt, so terminal
/// output never waits on HTTP requests to the daemon.
fn queue_for_daemon(tx: &mpsc::Sender<String>, backlog: &mut String, text: &str, cap: usize) {
    push_capped(backlog, text, cap);
    if backlog.is_empty() {
        return;
    }
    match tx.try_send(std::mem::take(backlog)) {
        Ok(()) => {}
        Err(TrySendError::Full(pending)) => *backlog = pending,
        Err(TrySendError::Closed(_)) => {}
    }
}

/// Forwards inputs resolved from the menu bar for `session_id` until the stream ends, errors,
/// or the receiver goes away.
async fn forward_remote_inputs<S, E>(
    mut stream: S,
    session_id: &str,
    tx: &mpsc::Sender<(String, bool)>,
) where
    S: Stream<Item = Result<Message, E>> + Unpin,
{
    while let Some(Ok(msg)) = stream.next().await {
        let Message::Text(txt) = msg else {
            continue;
        };
        if let Ok(AgentEvent::PromptResolved {
            session_id: target,
            input,
            press_enter,
            ..
        }) = serde_json::from_str::<AgentEvent>(&txt)
        {
            if target == session_id && tx.send((input, press_enter)).await.is_err() {
                break;
            }
        }
    }
}

pub async fn run_with_pty(config: PtyRunnerConfig) -> Result<i32> {
    let pty_system = native_pty_system();

    let (cols, rows) = size().unwrap_or((80, 24));
    let pair = pty_system
        .openpty(pty_size(cols, rows))
        .context("Failed to allocate PTY")?;

    let mut cmd = CommandBuilder::new(&config.command);
    cmd.args(&config.args);
    cmd.cwd(&config.cwd);

    let client = reqwest::Client::new();

    // Ensure daemon is running or auto-spawn it
    let reg_url = format!("{}/api/v1/sessions/register", config.daemon_url);
    let reg_req = RegisterSessionRequest {
        id: config.session_id.clone(),
        name: config.session_name.clone(),
        command: config.command.clone(),
        args: config.args.clone(),
        cwd: config.cwd.clone(),
        pid: None,
    };

    let daemon_available = if ensure_daemon_running(&config.daemon_url).await {
        match client.post(&reg_url).json(&reg_req).send().await {
            Ok(resp) if resp.status().is_success() => {
                info!(
                    "Registered with Agent Watch daemon at {}",
                    config.daemon_url
                );
                true
            }
            _ => false,
        }
    } else {
        warn!(
            "Agent Watch daemon not reachable at {}. Running in standalone mode.",
            config.daemon_url
        );
        false
    };

    // Raw mode must be on before the child starts: agents query the terminal at startup, and
    // in cooked mode the replies get echoed on screen and held until a newline.
    let _raw_mode = RawModeGuard::enable();

    let mut child = pair
        .slave
        .spawn_command(cmd)
        .context("Failed to spawn command in PTY")?;
    drop(pair.slave); // Close slave handle in master process

    let mut pty_reader = pair
        .master
        .try_clone_reader()
        .context("Failed to clone PTY reader")?;
    let pty_writer = pair
        .master
        .take_writer()
        .context("Failed to get PTY writer")?;
    let pty_writer_arc = Arc::new(Mutex::new(pty_writer));

    // Forward terminal resizes so the agent redraws at the real width.
    let master = pair.master;
    tokio::spawn(async move {
        let Ok(mut winch) = signal(SignalKind::window_change()) else {
            return;
        };
        while winch.recv().await.is_some() {
            if let Ok((cols, rows)) = size() {
                let _ = master.resize(pty_size(cols, rows));
            }
        }
    });

    let is_running = Arc::new(AtomicBool::new(true));

    // Channel for remote inputs (from daemon / menu bar)
    let (input_tx, mut input_rx) = mpsc::channel::<(String, bool)>(32);

    // Channel for PTY output to be sent to daemon
    let (output_tx, mut output_rx) = mpsc::channel::<String>(64);

    // Background task: Listen for events from WebSocket if daemon is available
    if daemon_available {
        let ws_url = config.ws_url.clone();
        let target_session_id = config.session_id.clone();
        let input_tx_clone = input_tx.clone();

        tokio::spawn(async move {
            if let Ok((ws_stream, _)) = connect_async(&ws_url).await {
                let (_, read) = ws_stream.split();
                forward_remote_inputs(read, &target_session_id, &input_tx_clone).await;
            }
        });

        // Background task: Send stdout chunks to daemon
        let daemon_url = config.daemon_url.clone();
        let session_id = config.session_id.clone();
        let client_clone = client.clone();

        tokio::spawn(async move {
            let url = format!("{}/api/v1/sessions/{}/output", daemon_url, session_id);
            while let Some(mut chunk) = output_rx.recv().await {
                // Batch whatever piled up during the previous request into one POST.
                while let Ok(more) = output_rx.try_recv() {
                    chunk.push_str(&more);
                }
                let _ = client_clone
                    .post(&url)
                    .json(&serde_json::json!({ "chunk": chunk }))
                    .send()
                    .await;
            }
        });
    } else {
        drop(output_rx);
    }

    // Thread: Forward PTY stdout -> terminal stdout AND output_tx
    let is_running_clone = is_running.clone();
    let _stdout_thread = std::thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        let mut pending_utf8: Vec<u8> = Vec::new();
        let mut daemon_backlog = String::new();
        let mut stdout = std::io::stdout();
        while is_running_clone.load(Ordering::Relaxed) {
            match pty_reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    let bytes = &buffer[..n];
                    let _ = stdout.write_all(bytes);
                    let _ = stdout.flush();

                    pending_utf8.extend_from_slice(bytes);
                    let text = take_complete_utf8(&mut pending_utf8);
                    if !text.is_empty() {
                        queue_for_daemon(
                            &output_tx,
                            &mut daemon_backlog,
                            &text,
                            DAEMON_BACKLOG_CAP,
                        );
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Thread: Forward terminal stdin -> PTY writer
    let is_running_clone = is_running.clone();
    let stdin_writer_arc = pty_writer_arc.clone();
    let _stdin_thread = std::thread::spawn(move || {
        let mut stdin = std::io::stdin();
        let mut buffer = [0u8; 1024];
        while is_running_clone.load(Ordering::Relaxed) {
            match stdin.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    if let Ok(mut writer) = stdin_writer_arc.lock() {
                        if writer.write_all(&buffer[..n]).is_err() {
                            break;
                        }
                        let _ = writer.flush();
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Task: Forward inputs from daemon/menu bar -> PTY writer
    let remote_writer_arc = pty_writer_arc.clone();
    tokio::spawn(async move {
        while let Some((mut text, press_enter)) = input_rx.recv().await {
            // Enter is a carriage return on a terminal; the tty turns it into \n for
            // line-mode programs, and raw-mode TUIs expect \r itself.
            if press_enter && !text.ends_with(['\r', '\n']) {
                text.push('\r');
            }
            if let Ok(mut writer) = remote_writer_arc.lock() {
                let _ = writer.write_all(text.as_bytes());
                let _ = writer.flush();
            }
        }
    });

    // Wait for child process to finish
    let status = child.wait()?;
    is_running.store(false, Ordering::Relaxed);
    drop(_raw_mode);

    let exit_code = if status.success() { 0 } else { 1 };

    // Notify daemon of termination
    if daemon_available {
        let term_url = format!(
            "{}/api/v1/sessions/{}/terminate",
            config.daemon_url, config.session_id
        );
        let _ = client
            .post(&term_url)
            .json(&serde_json::json!({ "exit_code": exit_code }))
            .send()
            .await;
    }

    Ok(exit_code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::stream;

    fn resolved(session_id: &str, input: &str) -> Message {
        let event = AgentEvent::PromptResolved {
            session_id: session_id.to_string(),
            prompt_id: "p1".to_string(),
            input: input.to_string(),
            press_enter: true,
        };
        Message::Text(serde_json::to_string(&event).unwrap())
    }

    #[tokio::test]
    async fn forward_remote_inputs_stops_when_stream_ends() {
        let (tx, mut rx) = mpsc::channel(8);
        let frames: Vec<Result<Message, ()>> = vec![
            Ok(resolved("other", "no")),
            Ok(Message::Ping(vec![])),
            Ok(Message::Text("not json".to_string())),
            Ok(resolved("me", "yes")),
        ];
        forward_remote_inputs(stream::iter(frames), "me", &tx).await;
        drop(tx);
        assert_eq!(rx.recv().await, Some(("yes".to_string(), true)));
        assert_eq!(rx.recv().await, None);
    }

    #[tokio::test]
    async fn forward_remote_inputs_stops_on_error() {
        let (tx, mut rx) = mpsc::channel(8);
        let frames: Vec<Result<Message, ()>> = vec![Err(()), Ok(resolved("me", "late"))];
        forward_remote_inputs(stream::iter(frames), "me", &tx).await;
        drop(tx);
        assert_eq!(rx.recv().await, None);
    }

    #[test]
    fn queue_for_daemon_holds_text_back_while_channel_is_full() {
        let (tx, mut rx) = mpsc::channel(1);
        let mut backlog = String::new();
        queue_for_daemon(&tx, &mut backlog, "a", 1024);
        assert!(backlog.is_empty());
        queue_for_daemon(&tx, &mut backlog, "b", 1024);
        queue_for_daemon(&tx, &mut backlog, "c", 1024);
        assert_eq!(backlog, "bc");
        assert_eq!(rx.try_recv().unwrap(), "a");
        queue_for_daemon(&tx, &mut backlog, "d", 1024);
        assert!(backlog.is_empty());
        assert_eq!(rx.try_recv().unwrap(), "bcd");
    }

    #[test]
    fn queue_for_daemon_drops_text_when_forwarder_is_gone() {
        let (tx, rx) = mpsc::channel(1);
        drop(rx);
        let mut backlog = String::new();
        queue_for_daemon(&tx, &mut backlog, "a", 1024);
        assert!(backlog.is_empty());
    }

    #[test]
    fn push_capped_keeps_newest_tail_on_char_boundary() {
        let mut backlog = String::new();
        push_capped(&mut backlog, "ab──", 4); // 'a','b', then two 3-byte chars = 8 bytes
        assert_eq!(backlog, "─");
        push_capped(&mut backlog, "x", 4);
        assert_eq!(backlog, "─x");
    }

    #[test]
    fn pty_size_maps_columns_and_rows() {
        let s = pty_size(120, 40);
        assert_eq!(
            (s.cols, s.rows, s.pixel_width, s.pixel_height),
            (120, 40, 0, 0)
        );
    }

    #[test]
    fn split_utf8_character_is_carried_over() {
        let rule = "──".as_bytes(); // 6 bytes, two 3-byte characters
        let mut buf = rule[..4].to_vec();
        assert_eq!(take_complete_utf8(&mut buf), "─");
        assert_eq!(buf.len(), 1);
        buf.extend_from_slice(&rule[4..]);
        assert_eq!(take_complete_utf8(&mut buf), "─");
        assert!(buf.is_empty());
    }

    #[test]
    fn invalid_bytes_are_not_held_back() {
        let mut buf = vec![b'a', 0xff, b'b'];
        assert_eq!(take_complete_utf8(&mut buf), "a\u{fffd}b");
        assert!(buf.is_empty());
    }
}
