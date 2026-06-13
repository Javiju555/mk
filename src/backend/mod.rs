pub mod daemon;
mod wtype;
mod xdotool;
mod ydotool;

use anyhow::{bail, Result};
use std::fmt;
use std::process::Command;

pub trait Backend {
    fn type_text(&self, text: &str) -> Result<()>;
    fn press_key(&self, key: &str) -> Result<()>;
    fn display_name(&self) -> &str;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayServer {
    Wayland,
    X11,
}

impl fmt::Display for DisplayServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DisplayServer::Wayland => write!(f, "Wayland"),
            DisplayServer::X11 => write!(f, "X11"),
        }
    }
}

fn command_exists(name: &str) -> bool {
    Command::new("which")
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn detect_display_server() -> DisplayServer {
    if let Ok(session) = std::env::var("XDG_SESSION_TYPE") {
        if session == "wayland" {
            return DisplayServer::Wayland;
        }
    }
    if std::env::var("WAYLAND_DISPLAY").is_ok() {
        return DisplayServer::Wayland;
    }
    if std::env::var("DISPLAY").is_ok() {
        return DisplayServer::X11;
    }
    DisplayServer::X11
}

pub fn detect_backend() -> Result<Box<dyn Backend>> {
    // Prefer daemon if running
    if daemon::daemon_is_running() {
        return Ok(Box::new(daemon::DaemonBackend));
    }

    let server = detect_display_server();
    match server {
        DisplayServer::Wayland => {
            if command_exists("wtype") {
                return Ok(Box::new(wtype::WtypeBackend));
            }
            if command_exists("ydotool") {
                return Ok(Box::new(ydotool::YdotoolBackend));
            }
            bail!(
                "No backend available for Wayland.\n\
                 Options:\n\
                 1. Start mk-daemon:  sudo mk-daemon\n\
                 2. Install wtype:    sudo pacman -S wtype\n\
                 3. Install ydotool:  sudo pacman -S ydotool"
            );
        }
        DisplayServer::X11 => {
            if command_exists("xdotool") {
                return Ok(Box::new(xdotool::XdotoolBackend));
            }
            if command_exists("wtype") {
                return Ok(Box::new(wtype::WtypeBackend));
            }
            bail!(
                "No backend available for X11.\n\
                 Options:\n\
                 1. Start mk-daemon:  sudo mk-daemon\n\
                 2. Install xdotool:  sudo pacman -S xdotool"
            );
        }
    }
}

pub struct DryRunBackend;

impl Backend for DryRunBackend {
    fn type_text(&self, text: &str) -> Result<()> {
        println!("[dry-run] type_text: \"{text}\"");
        Ok(())
    }

    fn press_key(&self, key: &str) -> Result<()> {
        println!("[dry-run] press_key: \"{key}\"");
        Ok(())
    }

    fn display_name(&self) -> &str {
        "dry-run"
    }
}
