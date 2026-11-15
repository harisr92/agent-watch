pub mod activity;
pub mod buffer;
pub mod detector;
pub mod models;

use std::path::PathBuf;

pub const DEFAULT_PORT: u16 = 45123;
pub const DEFAULT_HOST: &str = "127.0.0.1";

pub fn get_socket_path() -> PathBuf {
    let mut dir = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    dir.push(".agent-watch");
    std::fs::create_dir_all(&dir).ok();
    dir.push("aw.sock");
    dir
}

pub fn get_api_base_url() -> String {
    format!("http://{}:{}", DEFAULT_HOST, DEFAULT_PORT)
}

pub fn get_ws_url() -> String {
    format!("ws://{}:{}/api/v1/events", DEFAULT_HOST, DEFAULT_PORT)
}
