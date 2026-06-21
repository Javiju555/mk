use super::Backend;
use anyhow::{Context, Result};
use std::process::Command;

pub struct WtypeBackend;

impl Backend for WtypeBackend {
    fn type_text(&self, text: &str) -> Result<()> {
        let status = Command::new("wtype")
            .arg("--")
            .arg(text)
            .status()
            .context("Failed to run wtype")?;
        if !status.success() {
            anyhow::bail!("wtype exited with status: {status}");
        }
        Ok(())
    }

    fn press_key(&self, key: &str) -> Result<()> {
        if key.contains('+') {
            press_combo(key)
        } else {
            let xdo_key = convert_key(key);
            let status = Command::new("wtype")
                .arg("-k")
                .arg(&xdo_key)
                .status()
                .context("Failed to run wtype")?;
            if !status.success() {
                anyhow::bail!("wtype exited with status: {status}");
            }
            Ok(())
        }
    }

    fn display_name(&self) -> &str {
        "wtype"
    }
}

fn is_modifier(key: &str) -> bool {
    matches!(
        key.to_lowercase().as_str(),
        "ctrl" | "control" | "alt" | "shift" | "super" | "win" | "meta"
    )
}

fn convert_modifier(key: &str) -> String {
    match key.to_lowercase().as_str() {
        "ctrl" | "control" => "ctrl".to_string(),
        "alt" => "alt".to_string(),
        "shift" => "shift".to_string(),
        "super" | "win" | "meta" => "logo".to_string(),
        _ => key.to_string(),
    }
}


fn press_combo(combo: &str) -> Result<()> {
    let parts: Vec<&str> = combo.split('+').collect();
    let mut modifiers = Vec::new();
    let mut main_key = None;

    for part in parts {
        let part = part.trim();
        if is_modifier(part) {
            modifiers.push(part);
        } else {
            main_key = Some(part);
        }
    }

    let mut cmd = Command::new("wtype");
    for &m in &modifiers {
        cmd.arg("-M").arg(convert_modifier(m));
    }
    if let Some(key) = main_key {
        cmd.arg("-k").arg(convert_key(key));
    }
    for &m in modifiers.iter().rev() {
        cmd.arg("-m").arg(convert_modifier(m));
    }

    let status = cmd.status().context("Failed to run wtype")?;
    if !status.success() {
        anyhow::bail!("wtype exited with status: {status}");
    }
    Ok(())
}

fn convert_key(key: &str) -> String {
    match key.to_lowercase().as_str() {
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

