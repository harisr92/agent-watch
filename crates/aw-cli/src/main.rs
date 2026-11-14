mod pty_runner;

use anyhow::Result;
use aw_core::models::{AgentSession, AgentStatus, UserInputRequest};
use clap::{Parser, Subcommand};
use pty_runner::{run_with_pty, PtyRunnerConfig};
use std::net::SocketAddr;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;
use uuid::Uuid;

#[derive(Parser)]
#[command(name = "aw")]
#[command(about = "Agent Watch: Monitor & interact with AI agents from your macOS desktop", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the Agent Watch background daemon
    Daemon {
        /// Port to listen on
        #[arg(short, long, default_value_t = aw_core::DEFAULT_PORT)]
        port: u16,

        /// Bind host
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
    },

    /// Run an agent command under Agent Watch monitoring
    Run {
        /// Display name for this agent session
        #[arg(short, long)]
        name: Option<String>,

        /// Command to run
        command: String,

        /// Arguments for the command
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// List active and recent agent sessions
    List,

    /// Request input or approval from the user via the macOS Menu Bar
    Ask {
        /// Question or confirmation prompt to display
        prompt: String,

        /// Session name to identify who is asking
        #[arg(short, long, default_value = "Script / CLI")]
        name: String,

        /// A choice to offer; repeat for several (`-o staging -o production`).
        /// Use `label=value` to show a label but return a different value.
        #[arg(short = 'o', long = "option")]
        options: Vec<String>,

        /// Kind of answer: confirm, select, text, secret or acknowledge.
        /// Defaults to select when options are given, otherwise confirm.
        #[arg(short = 't', long = "type")]
        prompt_type: Option<String>,
    },

    /// Send input to an active agent session
    Input {
        /// Session ID
        session_id: String,

        /// Input text to inject into the agent
        text: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing (suppress debug unless RUST_LOG is set)
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Daemon { port, host } => {
            let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
            println!("🚀 Starting Agent Watch Daemon on http://{}", addr);
            let state = aw_daemon::AppState::new();
            aw_daemon::run_server(state, addr).await?;
        }

        Commands::Run {
            name,
            command,
            args,
        } => {
            let session_id = format!(
                "agent-{}",
                Uuid::new_v4()
                    .to_string()
                    .chars()
                    .take(8)
                    .collect::<String>()
            );
            let session_name = name.unwrap_or_else(|| {
                let cmd_file = PathBuf::from(&command);
                cmd_file
                    .file_name()
                    .and_then(|f| f.to_str())
                    .unwrap_or(&command)
                    .to_string()
            });

            let cwd = std::env::current_dir()?.to_string_lossy().to_string();
            let daemon_url = aw_core::get_api_base_url();
            let ws_url = aw_core::get_ws_url();

            let config = PtyRunnerConfig {
                session_id,
                session_name,
                command,
                args,
                cwd,
                daemon_url,
                ws_url,
            };

            let exit_code = run_with_pty(config).await?;
            std::process::exit(exit_code);
        }

        Commands::List => {
            let daemon_url = aw_core::get_api_base_url();
            let client = reqwest::Client::new();
            match client
                .get(format!("{}/api/v1/sessions", daemon_url))
                .send()
                .await
            {
                Ok(resp) => {
                    let sessions: Vec<AgentSession> = resp.json().await?;
                    if sessions.is_empty() {
                        println!("No active or recent agent sessions found.");
                        println!("💡 Run an agent with: aw run <command>");
                    } else {
                        println!(
                            "{:<14} {:<16} {:<18} {:<30}",
                            "ID", "NAME", "STATUS", "COMMAND"
                        );
                        println!("{}", "-".repeat(80));
                        for s in sessions {
                            let status_str = match s.status {
                                AgentStatus::Running => "🟢 Running",
                                AgentStatus::WaitingForInput => "⚠️  Needs Input",
                                AgentStatus::Idle => "⚪ Idle",
                                AgentStatus::Completed => "✅ Completed",
                                AgentStatus::Failed => "❌ Failed",
                            };
                            let cmd_display = format!("{} {}", s.command, s.args.join(" "));
                            let truncated_cmd = if cmd_display.len() > 30 {
                                format!("{}...", &cmd_display[..27])
                            } else {
                                cmd_display
                            };
                            println!(
                                "{:<14} {:<16} {:<18} {:<30}",
                                s.id, s.name, status_str, truncated_cmd
                            );
                            if let Some(prompt) = s.pending_prompt {
                                println!("   └── 🔔 Prompt: {}", prompt.prompt_text);
                            }
                        }
                    }
                }
                Err(_) => {
                    eprintln!("❌ Unable to reach Agent Watch daemon at {}", daemon_url);
                    eprintln!("Start the daemon with: aw daemon");
                }
            }
        }

        Commands::Ask {
            prompt,
            name,
            options,
            prompt_type,
        } => {
            let daemon_url = aw_core::get_api_base_url();
            let client = reqwest::Client::new();
            let choices: Vec<serde_json::Value> = options
                .iter()
                .map(|o| match o.split_once('=') {
                    Some((label, value)) => serde_json::json!({ "label": label, "value": value }),
                    None => serde_json::json!({ "label": o, "value": o }),
                })
                .collect();
            let mcp_req = serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "tools/call",
                "params": {
                    "name": "request_user_input",
                    "arguments": {
                        "prompt": prompt,
                        "session_name": name,
                        "choices": choices,
                        "prompt_type": prompt_type,
                    }
                }
            });

            match client
                .post(format!("{}/mcp", daemon_url))
                .json(&mcp_req)
                .send()
                .await
            {
                Ok(resp) => {
                    let res: serde_json::Value = resp.json().await?;
                    if let Some(text) = res
                        .get("result")
                        .and_then(|r| r.get("content"))
                        .and_then(|c| c.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|item| item.get("text"))
                        .and_then(|t| t.as_str())
                    {
                        println!("{}", text);
                    } else if let Some(err) = res
                        .get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(|m| m.as_str())
                    {
                        eprintln!("Error: {}", err);
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("❌ Failed to connect to Agent Watch daemon: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Commands::Input { session_id, text } => {
            let daemon_url = aw_core::get_api_base_url();
            let client = reqwest::Client::new();
            let req = UserInputRequest {
                input: text,
                press_enter: true,
            };
            let resp = client
                .post(format!(
                    "{}/api/v1/sessions/{}/input",
                    daemon_url, session_id
                ))
                .json(&req)
                .send()
                .await?;

            if resp.status().is_success() {
                println!("✅ Input submitted to session {}", session_id);
            } else {
                eprintln!("❌ Failed to submit input: session not found or inactive.");
            }
        }
    }

    Ok(())
}
