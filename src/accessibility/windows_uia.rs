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
    Ok(UiElement {
        role,
        name,
        x,
        y,
        width,
        height,
        is_enabled,
        is_offscreen,
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
    for el in all {
        let name = el.get_name().unwrap_or_default();
        if match_name(&name, query, mode) {
            return Ok(el);
        }
    }
    bail!(
        "control '{query}' no encontrado en ventana {window_id} (prueba `mk ui tree --window {window_id}` para ver nombres exactos)"
    );
}

pub fn ui_click(window_id: &str, query: &str, mode: &MatchMode) -> Result<UiElement> {
    let el = find(window_id, query, mode)?;
    let snapshot = to_ui_element(&el)?;
    match el.get_pattern::<uiautomation::patterns::UIInvokePattern>() {
        Ok(invoke) => {
            invoke.invoke().context("Invoke() falló")?;
            Ok(snapshot)
        }
        Err(_) => bail!(
            "control '{}' (role={}) no expone InvokePattern. Prueba `mk ui toggle` o coordenadas con --force-coords",
            snapshot.name,
            snapshot.role
        ),
    }
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
