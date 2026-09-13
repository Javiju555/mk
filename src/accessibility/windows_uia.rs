//! Backend UI Automation (solo Windows). Todo este archivo está tras cfg(windows) desde mod.rs.
use anyhow::{Context, Result, bail};
use uiautomation::core::UICondition;
use uiautomation::types::TreeScope;
use uiautomation::{UIElement, UIAutomation, UITreeWalker};

use crate::accessibility::{MatchMode, UiElement, match_name};

pub fn automation() -> Result<UIAutomation> {
    UIAutomation::new().context("UIAutomation::new() falló (COM)")
}

fn to_ui_element(el: &UIElement) -> Result<UiElement> {
    let name = el.get_name().unwrap_or_default();
    let role = el
        .get_localized_control_type()
        .unwrap_or_else(|_| el.get_control_type().map(|c| format!("{c:?}")).unwrap_or_default());
    let (x, y, width, height) = el
        .get_bounding_rectangle()
        .map(|r| {
            let w = (r.get_right() - r.get_left()).max(0) as u32;
            let h = (r.get_bottom() - r.get_top()).max(0) as u32;
            (r.get_left(), r.get_top(), w, h)
        })
        .unwrap_or((0, 0, 0, 0));
    let is_enabled = el.is_enabled().unwrap_or(true);
    let is_offscreen = el.is_offscreen().unwrap_or(false);
    let automation_id = el.get_automation_id().unwrap_or_default();
    Ok(UiElement {
        role,
        name,
        x,
        y,
        width,
        height,
        is_enabled,
        is_offscreen,
        automation_id,
    })
}

pub fn ui_tree_for_window(window_id: &str) -> Result<Vec<UiElement>> {
    let hwnd: usize = window_id.parse().map_err(|e| anyhow::anyhow!("Invalid window ID: {e}"))?;
    let automation = automation()?;
    let root = automation
        .element_from_handle((hwnd as isize).into())
        .context("FromHandle falló (¿ventana cerrada?)")?;
    let walker = automation.get_control_view_walker().context("walker")?;
    let condition = automation.create_true_condition().context("true condition")?;
    let mut out = Vec::new();
    walk(&walker, &root, &condition, &mut out, 5000)?;
    Ok(out)
}

fn walk(
    _walker: &UITreeWalker,
    el: &UIElement,
    cond: &UICondition,
    out: &mut Vec<UiElement>,
    cap: usize,
) -> Result<()> {
    if out.len() >= cap {
        return Ok(());
    }
    if let Ok(ui) = to_ui_element(el) {
        out.push(ui);
    }
    let Ok(children) = el.find_all(TreeScope::Children, cond) else {
        return Ok(());
    };
    for child in children {
        if out.len() >= cap {
            break;
        }
        walk(_walker, &child, cond, out, cap)?;
    }
    Ok(())
}

fn find(window_id: &str, query: &str, mode: &MatchMode) -> Result<UIElement> {
    let hwnd: usize = window_id.parse().map_err(|e| anyhow::anyhow!("Invalid window ID: {e}"))?;
    let automation = automation()?;
    let root = automation
        .element_from_handle((hwnd as isize).into())
        .context("FromHandle falló")?;
    let condition = automation.create_true_condition().context("true condition")?;
    let all = root.find_all(TreeScope::Descendants, &condition).context("walk UIA")?;
    let use_id = *mode == MatchMode::AutomationId;
    for el in all {
        if use_id {
            let id = el.get_automation_id().unwrap_or_default();
            if match_name(&id, query, mode) {
                return Ok(el);
            }
        } else {
            let name = el.get_name().unwrap_or_default();
            if match_name(&name, query, mode) {
                return Ok(el);
            }
        }
    }
    let hint = if use_id { "--id" } else { "--name" };
    bail!(
        "control '{query}' ({hint}) no encontrado en ventana {window_id} (prueba `mk ui tree --window {window_id}` para ver nombres/ids exactos)"
    );
}

pub fn ui_click(window_id: &str, query: &str, mode: &MatchMode) -> Result<UiElement> {
    let el = find(window_id, query, mode)?;
    let snapshot = to_ui_element(&el)?;
    // Prefer the control pattern (works unfocused, no coordinates); fall back
    // to a native UIA click at the clickable point (still object-based — the
    // caller never sees pixels).
    match el.get_pattern::<uiautomation::patterns::UIInvokePattern>() {
        Ok(invoke) => {
            invoke.invoke().context("Invoke() falló")?;
            Ok(snapshot)
        }
        Err(_) => el
            .click()
            .context(format!(
                "control '{}' (role={}) no expone InvokePattern y el click nativo falló",
                snapshot.name, snapshot.role
            ))
            .map(|()| snapshot),
    }
}

pub fn ui_double_click(window_id: &str, query: &str, mode: &MatchMode) -> Result<UiElement> {
    let el = find(window_id, query, mode)?;
    let snapshot = to_ui_element(&el)?;
    el.double_click().context(format!(
        "double-click falló en '{}' (role={})",
        snapshot.name, snapshot.role
    ))?;
    Ok(snapshot)
}

pub fn ui_right_click(window_id: &str, query: &str, mode: &MatchMode) -> Result<UiElement> {
    let el = find(window_id, query, mode)?;
    let snapshot = to_ui_element(&el)?;
    el.right_click().context(format!(
        "right-click falló en '{}' (role={})",
        snapshot.name, snapshot.role
    ))?;
    Ok(snapshot)
}

/// Bring the control into view and give it keyboard focus, so a following
/// `mk text` / `mk key` lands in it. ScrollIntoView is best-effort (not all
/// controls expose ScrollItemPattern); SetFocus is the hard requirement.
pub fn ui_focus(window_id: &str, query: &str, mode: &MatchMode) -> Result<UiElement> {
    let el = find(window_id, query, mode)?;
    let snapshot = to_ui_element(&el)?;
    if let Ok(scroll) = el.get_pattern::<uiautomation::patterns::UIScrollItemPattern>() {
        let _ = scroll.scroll_into_view();
    }
    el.set_focus().context(format!(
        "SetFocus falló en '{}' (role={}, ¿control no enfocable?)",
        snapshot.name, snapshot.role
    ))?;
    Ok(snapshot)
}

/// Read what the OS reports about a control: ValuePattern value, Toggle
/// state, ExpandCollapse state. Whatever is absent stays None — a plain
/// label yields just the element snapshot.
pub fn ui_get_value(window_id: &str, query: &str, mode: &MatchMode) -> Result<crate::accessibility::UiState> {
    use crate::accessibility::UiState;
    let el = find(window_id, query, mode)?;
    let element = to_ui_element(&el)?;
    let value = el
        .get_pattern::<uiautomation::patterns::UIValuePattern>()
        .ok()
        .and_then(|v| v.get_value().ok());
    let toggle_state = el
        .get_pattern::<uiautomation::patterns::UITogglePattern>()
        .ok()
        .and_then(|t| t.get_toggle_state().ok())
        .map(|s| format!("{s:?}"));
    let expand_state = el
        .get_pattern::<uiautomation::patterns::UIExpandCollapsePattern>()
        .ok()
        .and_then(|e| e.get_state().ok())
        .map(|s| format!("{s:?}"));
    Ok(UiState {
        element,
        value,
        toggle_state,
        expand_state,
    })
}

/// Expand (default) or collapse (`collapse = true`) a menu/combo/tree node,
/// reporting the resulting state.
pub fn ui_expand(
    window_id: &str,
    query: &str,
    mode: &MatchMode,
    collapse: bool,
) -> Result<crate::accessibility::UiState> {
    use crate::accessibility::UiState;
    let el = find(window_id, query, mode)?;
    let element = to_ui_element(&el)?;
    let pattern = el
        .get_pattern::<uiautomation::patterns::UIExpandCollapsePattern>()
        .map_err(|_| {
            anyhow::anyhow!(
                "control '{}' (role={}) no expone ExpandCollapsePattern",
                element.name,
                element.role
            )
        })?;
    if collapse {
        pattern.collapse().context("Collapse() falló")?;
    } else {
        pattern.expand().context("Expand() falló")?;
    }
    let expand_state = pattern.get_state().ok().map(|s| format!("{s:?}"));
    Ok(UiState {
        element,
        value: None,
        toggle_state: None,
        expand_state,
    })
}

/// Poll until the control exists (and, with `require_visible`, is enabled
/// and on-screen) or `timeout` expires. The "did that panel appear?" check
/// for async UIs — same shape as `window wait`.
pub fn ui_wait(
    window_id: &str,
    query: &str,
    mode: &MatchMode,
    timeout: std::time::Duration,
    interval: std::time::Duration,
    require_visible: bool,
) -> Result<UiElement> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if let Ok(el) = find(window_id, query, mode) {
            let snapshot = to_ui_element(&el)?;
            if !require_visible || (snapshot.is_enabled && !snapshot.is_offscreen) {
                return Ok(snapshot);
            }
        }
        if std::time::Instant::now() >= deadline {
            bail!(
                "Timeout esperando control '{query}' en ventana {window_id} tras {}s",
                timeout.as_secs()
            );
        }
        std::thread::sleep(interval);
    }
}

/// Screenshot just this control: capture the window, crop to the element's
/// bounds (+ `pad` px tolerance for shadows/decorations), save to `out_path`.
/// Best-effort geometry: UIA bounds are screen pixels, the capture starts at
/// the window origin reported by `mk window list`.
pub fn ui_shot(
    window_id: &str,
    query: &str,
    mode: &MatchMode,
    out_path: &str,
    pad: u32,
    zoom: u32,
) -> Result<UiElement> {
    let el = find(window_id, query, mode)?;
    let snapshot = to_ui_element(&el)?;
    let wins = crate::windows::list_windows()?;
    let win = wins
        .into_iter()
        .find(|w| w.id == window_id)
        .ok_or_else(|| anyhow::anyhow!("Window {window_id} not found"))?;
    let rx = snapshot.x.saturating_sub(win.x).saturating_sub(pad as i32).max(0) as u32;
    let ry = snapshot.y.saturating_sub(win.y).saturating_sub(pad as i32).max(0) as u32;
    let rw = snapshot.width.saturating_add(2 * pad).max(1);
    let rh = snapshot.height.saturating_add(2 * pad).max(1);
    let tmp = std::env::temp_dir().join(format!("mk-ui-shot-{}.png", std::process::id()));
    let tmp_str = tmp.to_string_lossy().into_owned();
    crate::vision::capture_window(
        window_id,
        &tmp_str,
        crate::vision::ScreenshotFormat::Raw,
        100,
    )
    .context("capture de ventana falló (¿minimizada?)")?;
    crate::vision::crop_image_file(&tmp_str, out_path, (rx, ry, rw, rh), zoom, 90)
        .context("crop al control falló (¿geometría obsoleta? re-lee `mk window list`)")?;
    let _ = std::fs::remove_file(&tmp);
    Ok(snapshot)
}

pub fn ui_toggle(window_id: &str, query: &str, mode: &MatchMode) -> Result<UiElement> {
    let el = find(window_id, query, mode)?;
    let snapshot = to_ui_element(&el)?;
    match el.get_pattern::<uiautomation::patterns::UITogglePattern>() {
        Ok(t) => {
            t.toggle().context("Toggle() falló")?;
            Ok(snapshot)
        }
        Err(_) => bail!(
            "control '{}' (role={}) no expone TogglePattern",
            snapshot.name,
            snapshot.role
        ),
    }
}

pub fn ui_set_value(window_id: &str, query: &str, value: &str, mode: &MatchMode) -> Result<UiElement> {
    let el = find(window_id, query, mode)?;
    let snapshot = to_ui_element(&el)?;
    match el.get_pattern::<uiautomation::patterns::UIValuePattern>() {
        Ok(v) => {
            v.set_value(value).context("SetValue() falló (¿control read-only?)")?;
            Ok(snapshot)
        }
        Err(_) => bail!(
            "control '{}' (role={}) no expone ValuePattern",
            snapshot.name,
            snapshot.role
        ),
    }
}
