use anyhow::{Context, Result};
use aw_core::models::{AgentEvent, RegisterSessionRequest};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, size};
use futures_util::StreamExt;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
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

pub async fn run_with_pty(config: PtyRunnerConfig) -> Result<i32> {
    let pty_system = native_pty_system();

    let (cols, rows) = size().unwrap_or((80, 24));
    let pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
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

    // Enable raw mode on terminal for transparent keystrokes
    let _ = enable_raw_mode();
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
        let is_running_clone = is_running.clone();

        tokio::spawn(async move {
            if let Ok((ws_stream, _)) = connect_async(&ws_url).await {
                let (_, mut read) = ws_stream.split();
                while is_running_clone.load(Ordering::Relaxed) {
                    if let Some(Ok(tokio_tungstenite::tungstenite::Message::Text(txt))) =
                        read.next().await
                    {
                        if let Ok(AgentEvent::PromptResolved {
                            session_id,
                            input,
                            press_enter,
                            ..
                        }) = serde_json::from_str::<AgentEvent>(&txt)
                        {
                            if session_id == target_session_id {
                                let _ = input_tx_clone.send((input, press_enter)).await;
                            }
                        }
                    }
                }
            }
        });

        // Background task: Send stdout chunks to daemon
        let daemon_url = config.daemon_url.clone();
        let session_id = config.session_id.clone();
        let client_clone = client.clone();
        let is_running_clone = is_running.clone();

        tokio::spawn(async move {
            while is_running_clone.load(Ordering::Relaxed) {
                if let Some(chunk) = output_rx.recv().await {
                    let url = format!("{}/api/v1/sessions/{}/output", daemon_url, session_id);
                    let _ = client_clone
                        .post(&url)
                        .json(&serde_json::json!({ "chunk": chunk }))
                        .send()
                        .await;
                }
            }
        });
    }

    // Thread: Forward PTY stdout -> terminal stdout AND output_tx
    let is_running_clone = is_running.clone();
    let _stdout_thread = std::thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        let mut pending_utf8: Vec<u8> = Vec::new();
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
                        let _ = output_tx.blocking_send(text);
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
    let is_running_clone = is_running.clone();
    tokio::spawn(async move {
        while is_running_clone.load(Ordering::Relaxed) {
            if let Some((mut text, press_enter)) = input_rx.recv().await {
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
        }
    });

    // Wait for child process to finish
    let status = child.wait()?;
    is_running.store(false, Ordering::Relaxed);
    let _ = disable_raw_mode();

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
    use super::take_complete_utf8;

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
