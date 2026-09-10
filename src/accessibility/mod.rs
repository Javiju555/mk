use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiElement {
    pub role: String,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub is_enabled: bool,
    #[serde(default)]
    pub is_offscreen: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchMode {
    Exact,
    Contains,
    Regex,
}

/// Exact = trim + case-insensitive. Contains = substring case-insensitive.
/// Regex = crate `regex`, case-sensitive salvo `(?i)`; inválida → false.
pub fn match_name(candidate: &str, query: &str, mode: &MatchMode) -> bool {
    match mode {
        MatchMode::Exact => candidate.trim().to_lowercase() == query.trim().to_lowercase(),
        MatchMode::Contains => candidate.to_lowercase().contains(&query.to_lowercase()),
        MatchMode::Regex => regex::Regex::new(query).map(|re| re.is_match(candidate)).unwrap_or(false),
    }
}

pub fn get_ui_tree() -> Result<Vec<UiElement>> {
    // Accessibility stubs - returns an empty list for now.
    // Ready for OS Accessibility API native integrations (UIA, AXUIElement, AT-SPI2).
    Ok(Vec::new())
}

pub fn find_button(name: &str) -> Result<Option<UiElement>> {
    let tree = get_ui_tree()?;
    Ok(tree.into_iter().find(|el| {
        el.role.to_lowercase() == "button" && match_name(&el.name, name, &MatchMode::Exact)
    }))
}

pub fn find_input(placeholder: &str) -> Result<Option<UiElement>> {
    let tree = get_ui_tree()?;
    Ok(tree.into_iter().find(|el| {
        let role = el.role.to_lowercase();
        (role == "input" || role == "text_input" || role == "edit") && match_name(&el.name, placeholder, &MatchMode::Exact)
    }))
}

#[cfg(target_os = "windows")]
pub mod windows_uia;

#[cfg(target_os = "windows")]
pub use windows_uia::{ui_click, ui_set_value, ui_toggle, ui_tree_for_window};

#[cfg(not(target_os = "windows"))]
pub fn ui_tree_for_window(_window_id: &str) -> Result<Vec<UiElement>> {
    Ok(Vec::new())
}

#[cfg(not(target_os = "windows"))]
pub fn ui_click(_window_id: &str, _query: &str, _mode: &MatchMode) -> Result<UiElement> {
    anyhow::bail!("UI Automation solo disponible en Windows")
}

#[cfg(not(target_os = "windows"))]
pub fn ui_toggle(_window_id: &str, _query: &str, _mode: &MatchMode) -> Result<UiElement> {
    anyhow::bail!("UI Automation solo disponible en Windows")
}

#[cfg(not(target_os = "windows"))]
pub fn ui_set_value(
    _window_id: &str,
    _query: &str,
    _value: &str,
    _mode: &MatchMode,
) -> Result<UiElement> {
    anyhow::bail!("UI Automation solo disponible en Windows")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accessibility_stubs() {
        let tree = get_ui_tree();
        assert!(tree.is_ok());
        let btn = find_button("Save");
        assert!(btn.is_ok());
        let input = find_input("Username");
        assert!(input.is_ok());
    }

    #[test]
    fn test_match_name_exact_is_trim_case_insensitive() {
        assert!(match_name("Mezclador", "mezclador", &MatchMode::Exact));
        assert!(match_name("  Mezclador  ", "MEZCLADOR", &MatchMode::Exact));
        assert!(!match_name("Mezclador panel", "mezclador", &MatchMode::Exact));
    }

    #[test]
    fn test_match_name_contains_and_regex() {
        assert!(match_name("Delay: relativamente sencillo", "delay", &MatchMode::Contains));
        assert!(!match_name("Delay", "lay$", &MatchMode::Contains));
        assert!(match_name("Track 01", r"^Track \d+$", &MatchMode::Regex));
        assert!(!match_name("Track AB", r"^Track \d+$", &MatchMode::Regex));
        assert!(!match_name("cualquier cosa", "(unclosed", &MatchMode::Regex), "regex inválida = false, no panic");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_uia_module_loads() {
        let automation = crate::accessibility::windows_uia::automation();
        assert!(automation.is_ok(), "UIAutomation::new() debe inicializar COM");
    }
}
