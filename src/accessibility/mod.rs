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
    /// Stable per-app control identifier (AutomationId). Empty when the app
    /// does not set one. Prefer it over `--name`: it survives relabels and
    /// locales, unlike visible text.
    #[serde(default)]
    pub automation_id: String,
}

/// Readable UI state: the element plus whatever the OS reports through
/// Value / Toggle / ExpandCollapse patterns (each optional — a plain label
/// exposes none of them).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiState {
    pub element: UiElement,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toggle_state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expand_state: Option<String>,
}

/// One desktop-wide search hit: the element plus its top-level window, so
/// the agent can scope the next action with `--window <window_id>`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiHit {
    pub window_id: String,
    pub window_title: String,
    pub element: UiElement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchMode {
    Exact,
    Contains,
    Regex,
    /// Exact match on `automation_id` (case-sensitive, no trim): stable ids.
    AutomationId,
}

/// Exact = trim + case-insensitive. Contains = substring case-insensitive.
/// Regex = crate `regex`, case-sensitive salvo `(?i)`; inválida → false.
/// AutomationId = igualdad exacta (los ids son estables y case-sensitive).
pub fn match_name(candidate: &str, query: &str, mode: &MatchMode) -> bool {
    match mode {
        MatchMode::Exact => candidate.trim().to_lowercase() == query.trim().to_lowercase(),
        MatchMode::Contains => candidate.to_lowercase().contains(&query.to_lowercase()),
        MatchMode::Regex => regex::Regex::new(query).map(|re| re.is_match(candidate)).unwrap_or(false),
        MatchMode::AutomationId => candidate == query,
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
pub use windows_uia::{
    ui_click, ui_double_click, ui_drag, ui_expand, ui_find, ui_focus, ui_get_value,
    ui_right_click, ui_set_value, ui_shot, ui_show_menu, ui_toggle, ui_tree_for_window,
    ui_type, ui_wait,
};

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

#[cfg(not(target_os = "windows"))]
pub fn ui_double_click(_window_id: &str, _query: &str, _mode: &MatchMode) -> Result<UiElement> {
    anyhow::bail!("UI Automation solo disponible en Windows")
}

#[cfg(not(target_os = "windows"))]
pub fn ui_right_click(_window_id: &str, _query: &str, _mode: &MatchMode) -> Result<UiElement> {
    anyhow::bail!("UI Automation solo disponible en Windows")
}

#[cfg(not(target_os = "windows"))]
pub fn ui_focus(_window_id: &str, _query: &str, _mode: &MatchMode) -> Result<UiElement> {
    anyhow::bail!("UI Automation solo disponible en Windows")
}

#[cfg(not(target_os = "windows"))]
pub fn ui_get_value(_window_id: &str, _query: &str, _mode: &MatchMode) -> Result<UiState> {
    anyhow::bail!("UI Automation solo disponible en Windows")
}

#[cfg(not(target_os = "windows"))]
pub fn ui_expand(
    _window_id: &str,
    _query: &str,
    _mode: &MatchMode,
    _collapse: bool,
) -> Result<UiState> {
    anyhow::bail!("UI Automation solo disponible en Windows")
}

#[cfg(not(target_os = "windows"))]
pub fn ui_wait(
    _window_id: &str,
    _query: &str,
    _mode: &MatchMode,
    _timeout: std::time::Duration,
    _interval: std::time::Duration,
    _require_visible: bool,
) -> Result<UiElement> {
    anyhow::bail!("UI Automation solo disponible en Windows")
}

#[cfg(not(target_os = "windows"))]
pub fn ui_shot(
    _window_id: &str,
    _query: &str,
    _mode: &MatchMode,
    _out_path: &str,
    _pad: u32,
    _zoom: u32,
) -> Result<UiElement> {
    anyhow::bail!("UI Automation solo disponible en Windows")
}

#[cfg(not(target_os = "windows"))]
pub fn ui_find(_query: &str, _mode: &MatchMode) -> Result<Vec<UiHit>> {
    anyhow::bail!("UI Automation solo disponible en Windows")
}

#[cfg(not(target_os = "windows"))]
pub fn ui_type(
    _window_id: &str,
    _query: &str,
    _mode: &MatchMode,
    _text: &str,
    _via_clipboard: bool,
) -> Result<UiElement> {
    anyhow::bail!("UI Automation solo disponible en Windows")
}

#[cfg(not(target_os = "windows"))]
pub fn ui_show_menu(_window_id: &str, _query: &str, _mode: &MatchMode) -> Result<UiElement> {
    anyhow::bail!("UI Automation solo disponible en Windows")
}

#[cfg(not(target_os = "windows"))]
pub fn ui_drag(
    _window_id: &str,
    _from: &str,
    _to: &str,
    _mode: &MatchMode,
) -> Result<(UiElement, UiElement)> {
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

    #[test]
    fn test_automation_id_match_and_compat() {
        // Old JSON without automation_id still deserializes (default "").
        let el: UiElement = serde_json::from_value(serde_json::json!({
            "role": "button", "name": "OK", "x": 1, "y": 2, "width": 3, "height": 4
        }))
        .unwrap();
        assert_eq!(el.automation_id, "");
        assert!(match_name("btn_ok", "btn_ok", &MatchMode::AutomationId));
        assert!(!match_name("btn_ok", "BTN_OK", &MatchMode::AutomationId));
    }

    #[test]
    fn test_ui_hit_serializes() {
        let hit = UiHit {
            window_id: "123".into(),
            window_title: "App".into(),
            element: UiElement {
                role: "button".into(),
                name: "OK".into(),
                x: 1,
                y: 2,
                width: 3,
                height: 4,
                is_enabled: true,
                is_offscreen: false,
                automation_id: "btn_ok".into(),
            },
        };
        let v = serde_json::to_value(&hit).unwrap();
        assert_eq!(v["window_id"], "123");
        assert_eq!(v["element"]["automation_id"], "btn_ok");
    }
}
