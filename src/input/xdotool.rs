use super::Backend;
use anyhow::{Context, Result};
use std::process::Command;

pub struct XdotoolBackend;

impl Backend for XdotoolBackend {
    fn type_text(&self, text: &str) -> Result<()> {
        let status = Command::new("xdotool")
            .args(["type", "--clearmodifiers", text])
            .status()
            .context("Failed to run xdotool")?;
        if !status.success() {
            anyhow::bail!("xdotool exited with status: {status}");
        }
        Ok(())
    }

    fn press_key(&self, key: &str) -> Result<()> {
        let xdo_key = convert_key(key);
        let status = Command::new("xdotool")
            .args(["key", "--clearmodifiers", &xdo_key])
            .status()
            .context("Failed to run xdotool")?;
        if !status.success() {
            anyhow::bail!("xdotool exited with status: {status}");
        }
        Ok(())
    }

    fn display_name(&self) -> &str {
        "xdotool"
    }
}

fn convert_key(key: &str) -> String {
    match key.to_lowercase().as_str() {
        "ctrl+s" => "ctrl+s".into(),
        "ctrl+c" => "ctrl+c".into(),
        "ctrl+v" => "ctrl+v".into(),
        "ctrl+z" => "ctrl+z".into(),
        "ctrl+x" => "ctrl+x".into(),
        "ctrl+a" => "ctrl+a".into(),
        "alt+tab" => "alt+Tab".into(),
        "enter" => "Return".into(),
        "esc" | "escape" => "Escape".into(),
        "tab" => "Tab".into(),
        "backspace" => "BackSpace".into(),
        "delete" => "Delete".into(),
        "up" => "Up".into(),
        "down" => "Down".into(),
        "left" => "Left".into(),
        "right" => "Right".into(),
        "space" => "space".into(),
        _ => key.into(),
    }
}
