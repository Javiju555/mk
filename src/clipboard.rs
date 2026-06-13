use crate::backend::DisplayServer;
use std::process::Command;

fn command_exists(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub enum ClipboardTool {
    WlCopy,
    Xclip,
    Xsel,
    None,
}

pub fn detect_clipboard_tool(server: DisplayServer) -> ClipboardTool {
    match server {
        DisplayServer::Wayland => {
            if command_exists("wl-copy") {
                return ClipboardTool::WlCopy;
            }
        }
        DisplayServer::X11 => {
            if command_exists("xclip") {
                return ClipboardTool::Xclip;
            }
            if command_exists("xsel") {
                return ClipboardTool::Xsel;
            }
        }
    }
    ClipboardTool::None
}
