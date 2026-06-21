use super::Backend;
use anyhow::{Context, Result};
use std::process::Command;

pub struct YdotoolBackend;

impl Backend for YdotoolBackend {
    fn type_text(&self, text: &str) -> Result<()> {
        let status = Command::new("ydotool")
            .args(["type", "--", text])
            .status()
            .context("Failed to run ydotool")?;
        if !status.success() {
            anyhow::bail!("ydotool exited with status: {status}");
        }
        Ok(())
    }

    fn press_key(&self, key: &str) -> Result<()> {
        if key.contains('+') {
            press_combo(key)
        } else {
            let keycode = resolve_key_code_str(key)
                .map(|s| s.to_string())
                .unwrap_or_else(|| key.to_string());
            let status = Command::new("ydotool")
                .args(["key", &keycode])
                .status()
                .context("Failed to run ydotool")?;
            if !status.success() {
                anyhow::bail!("ydotool exited with status: {status}");
            }
            Ok(())
        }
    }

    fn display_name(&self) -> &str {
        "ydotool"
    }
}

fn is_modifier(key: &str) -> bool {
    matches!(
        key.to_lowercase().as_str(),
        "ctrl" | "control" | "alt" | "shift" | "super" | "win" | "meta"
    )
}

fn resolve_modifier_code(key: &str) -> Option<&'static str> {
    match key.to_lowercase().as_str() {
        "ctrl" | "control" => Some("29"),
        "alt" | "meta" => Some("56"),
        "shift" => Some("42"),
        "super" | "win" | "logo" => Some("125"),
        _ => None,
    }
}

fn resolve_key_code_str(key: &str) -> Option<&'static str> {
    match key.to_lowercase().as_str() {
        "enter" | "return" => Some("28"),
        "esc" | "escape" => Some("1"),
        "tab" => Some("15"),
        "backspace" => Some("14"),
        "delete" | "del" => Some("111"),
        "up" => Some("103"),
        "down" => Some("108"),
        "left" => Some("105"),
        "right" => Some("106"),
        "space" => Some("57"),
        // letters
        "a" => Some("30"),
        "b" => Some("48"),
        "c" => Some("46"),
        "d" => Some("32"),
        "e" => Some("18"),
        "f" => Some("33"),
        "g" => Some("34"),
        "h" => Some("35"),
        "i" => Some("23"),
        "j" => Some("36"),
        "k" => Some("37"),
        "l" => Some("38"),
        "m" => Some("50"),
        "n" => Some("49"),
        "o" => Some("24"),
        "p" => Some("25"),
        "q" => Some("16"),
        "r" => Some("19"),
        "s" => Some("31"),
        "t" => Some("20"),
        "u" => Some("22"),
        "v" => Some("47"),
        "w" => Some("17"),
        "x" => Some("45"),
        "y" => Some("21"),
        "z" => Some("44"),
        _ => None,
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

    let mut args = Vec::new();
    // Press modifiers
    for &m in &modifiers {
        if let Some(code) = resolve_modifier_code(m) {
            args.push(format!("{code}:1"));
        }
    }
    // Press and release main key
    if let Some(key) = main_key {
        if let Some(code) = resolve_key_code_str(key) {
            args.push(format!("{code}:1"));
            args.push(format!("{code}:0"));
        }
    }
    // Release modifiers in reverse order
    for &m in modifiers.iter().rev() {
        if let Some(code) = resolve_modifier_code(m) {
            args.push(format!("{code}:0"));
        }
    }

    if args.is_empty() {
        return Ok(());
    }

    let status = Command::new("ydotool")
        .arg("key")
        .args(args)
        .status()
        .context("Failed to run ydotool")?;

    if !status.success() {
        anyhow::bail!("ydotool exited with status: {status}");
    }
    Ok(())
}
