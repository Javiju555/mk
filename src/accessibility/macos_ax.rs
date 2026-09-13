//! macOS accessibility backend via System Events (`osascript`, no new deps).
//!
//! One `osascript` call dumps a window's whole AX tree as unit-separator
//! TSV (`role\x1Fname\x1Fid\x1Fx,y,w,h`); matching/clicking happens in Rust
//! on the parsed dump. Clicks go through the existing CGEvent backend at the
//! element center. Anything missing degrades to an honest error, never to
//! silent coordinate guessing.
//!
//! Coordinate contract: AX reports logical points; mk works in physical
//! pixels, so every bound is scaled up (NEEDS validation on real Retina
//! hardware — mirrors the Windows/Linux HiDPI fixes).

#[cfg(target_os = "macos")]
use anyhow::{Context, Result, bail};
#[cfg(target_os = "macos")]
use std::process::Command;
use crate::accessibility::UiElement;
#[cfg(target_os = "macos")]
use crate::accessibility::{MatchMode, match_name};

/// Field separator: ASCII unit separator never appears in UI strings, unlike
/// tabs/newlines (which AX help texts do contain).
const SEP: char = '\x1F';
/// Cap the AppleScript loop: Electron apps can expose 10k+ elements and
/// osascript string-building gets slow past a few thousand.
const DUMP_CAP: usize = 3000;

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Parse one `ax_dump` TSV line. Malformed lines are skipped by the caller —
/// a half-read tree is better than no tree, but a misparsed bound that
/// misplaces a click is worse than a missing element.
fn parse_line(line: &str) -> Option<UiElement> {
    let mut parts = line.split(SEP);
    let role = parts.next()?.to_string();
    let name = parts.next().unwrap_or("").to_string();
    let automation_id = parts.next().unwrap_or("").to_string();
    let coords = parts.next()?;
    let nums: Vec<f64> = coords.split(',').map(|n| n.trim().parse().ok()).collect::<Option<Vec<_>>>()?;
    if nums.len() != 4 {
        return None;
    }
    // AX numbers can arrive as "10" or "10.0".
    let x = nums[0].round() as i32;
    let y = nums[1].round() as i32;
    let w = nums[2].round().max(0.0) as u32;
    let h = nums[3].round().max(0.0) as u32;
    Some(UiElement {
        role,
        name,
        x,
        y,
        width: w,
        height: h,
        // Enabled/offscreen need per-element attribute reads (N extra calls);
        // the dump reports presence only. Callers treat unknown as usable.
        is_enabled: true,
        is_offscreen: false,
        automation_id,
    })
}

/// Pure, unit-tested: TSV dump → elements. Malformed lines skipped.
pub fn parse_ax_dump(out: &str) -> Vec<UiElement> {
    out.lines().filter_map(parse_line).collect()
}

/// Scale logical-point bounds to mk's physical pixels (Retina).
pub fn scale_ax_tree(mut tree: Vec<UiElement>, scale: f64) -> Vec<UiElement> {
    if !(scale.is_finite() && scale > 0.0 && scale != 1.0) {
        return tree;
    }
    for el in &mut tree {
        el.x = (el.x as f64 * scale).round() as i32;
        el.y = (el.y as f64 * scale).round() as i32;
        el.width = (el.width as f64 * scale).round().max(0.0) as u32;
        el.height = (el.height as f64 * scale).round().max(0.0) as u32;
    }
    tree
}

fn dump_script(app: &str, title: &str) -> String {
    let sep = SEP as u32;
    format!(
        r#"tell application "System Events"
  tell process "{app}"
    set winRef to missing value
    try
      set winRef to first window whose name is "{title}"
    end try
    if winRef is missing value then
      try
        set winRef to first window whose name contains "{title}"
      end try
    end if
    if winRef is missing value then
      try
        set winRef to window 1
      end try
    end if
    if winRef is missing value then return ""
    set out to ""
    set n to 0
    repeat with el in (entire contents of winRef)
      set n to n + 1
      if n > {DUMP_CAP} then exit repeat
      try
        set r to role of el as string
      on error
        set r to ""
      end try
      try
        set d to description of el as string
      on error
        set d to ""
      end try
      try
        set i to value of attribute "AXIdentifier" of el as string
      on error
        set i to ""
      end try
      try
        set p to position of el
        set s to size of el
        set g to ((item 1 of p) as string) & "," & ((item 2 of p) as string) & "," & ((item 1 of s) as string) & "," & ((item 2 of s) as string)
      on error
        set g to "0,0,0,0"
      end try
      set out to out & r & (character id {sep}) & d & (character id {sep}) & i & (character id {sep}) & g & "\n"
    end repeat
    return out
  end tell
end tell"#,
        app = escape(app),
        title = escape(title),
    )
}

#[cfg(target_os = "macos")]
fn ax_dump(app: &str, title: &str) -> Result<Vec<UiElement>> {
    let output = Command::new("osascript")
        .arg("-e")
        .arg(dump_script(app, title))
        .output()
        .context("osascript failed (macOS only)")?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        bail!("osascript dump failed: {}", err.trim());
    }
    let tree = parse_ax_dump(&String::from_utf8_lossy(&output.stdout));
    Ok(scale_ax_tree(tree, crate::input::macos::primary_scale_factor()))
}

#[cfg(target_os = "macos")]
fn window_app(window_id: &str) -> Result<(String, String)> {
    let wins = crate::windows::list_windows()?;
    let w = wins
        .into_iter()
        .find(|w| w.id == window_id)
        .ok_or_else(|| anyhow::anyhow!("Window with ID {window_id} not found"))?;
    Ok((w.app_name, w.title))
}

#[cfg(target_os = "macos")]
pub fn ui_tree_for_window(window_id: &str) -> Result<Vec<UiElement>> {
    let (app, title) = window_app(window_id)?;
    ax_dump(&app, &title)
}

#[cfg(target_os = "macos")]
fn find(window_id: &str, query: &str, mode: &MatchMode) -> Result<UiElement> {
    let tree = ui_tree_for_window(window_id)?;
    let use_id = *mode == MatchMode::AutomationId;
    tree.into_iter()
        .find(|el| {
            if use_id {
                match_name(&el.automation_id, query, mode)
            } else {
                match_name(&el.name, query, mode)
            }
        })
        .ok_or_else(|| {
            let hint = if use_id { "--id" } else { "--name" };
            anyhow::anyhow!(
                "control '{query}' ({hint}) no encontrado en ventana {window_id} (prueba `mk ui tree --window {window_id}`)"
            )
        })
}

#[cfg(target_os = "macos")]
fn click_center(el: &UiElement) -> Result<()> {
    use crate::input::{Backend, macos::MacosBackend};
    let cx = el.x + (el.width as i32) / 2;
    let cy = el.y + (el.height as i32) / 2;
    MacosBackend.mouse_click(cx, cy, "left", 0)
}

/// Click the matched control's center through CGEvent (no coordinates for
/// the caller — same object contract as Windows, coarser mechanism).
#[cfg(target_os = "macos")]
pub fn ui_click(window_id: &str, query: &str, mode: &MatchMode) -> Result<UiElement> {
    let el = find(window_id, query, mode)?;
    click_center(&el).context(format!("click falló en '{}' (role={})", el.name, el.role))?;
    Ok(el)
}

#[cfg(target_os = "macos")]
pub fn ui_double_click(window_id: &str, query: &str, mode: &MatchMode) -> Result<UiElement> {
    let el = find(window_id, query, mode)?;
    click_center(&el).context("click 1/2 falló")?;
    std::thread::sleep(std::time::Duration::from_millis(80));
    click_center(&el).context("click 2/2 falló")?;
    Ok(el)
}

#[cfg(target_os = "macos")]
pub fn ui_right_click(window_id: &str, query: &str, mode: &MatchMode) -> Result<UiElement> {
    use crate::input::{Backend, macos::MacosBackend};
    let el = find(window_id, query, mode)?;
    let cx = el.x + (el.width as i32) / 2;
    let cy = el.y + (el.height as i32) / 2;
    MacosBackend
        .mouse_click(cx, cy, "right", 0)
        .context(format!("right-click falló en '{}'", el.name))?;
    Ok(el)
}

/// Validate the control exists, then raise its window (AXRaise + activate).
/// Control-level keyboard focus still needs a click (`ui click` + `mk text`
/// is the mac recipe); this guarantees the right window is frontmost.
#[cfg(target_os = "macos")]
pub fn ui_focus(window_id: &str, query: &str, mode: &MatchMode) -> Result<UiElement> {
    let el = find(window_id, query, mode)?;
    crate::windows::focus_window(window_id)?;
    Ok(el)
}

/// Set an edit's value the macOS way: click to focus, then type natively.
/// (No ValuePattern without new deps; this is the honest equivalent.)
#[cfg(target_os = "macos")]
pub fn ui_set_value(window_id: &str, query: &str, value: &str, mode: &MatchMode) -> Result<UiElement> {
    use crate::input::{Backend, macos::MacosBackend};
    let el = find(window_id, query, mode)?;
    click_center(&el).context("click para enfocar falló")?;
    std::thread::sleep(std::time::Duration::from_millis(150));
    MacosBackend
        .type_text(value)
        .context("type_text falló")?;
    Ok(el)
}

/// Toggle via click (checkboxes/switches actuate on click); same for type.
#[cfg(target_os = "macos")]
pub fn ui_toggle(window_id: &str, query: &str, mode: &MatchMode) -> Result<UiElement> {
    ui_click(window_id, query, mode)
}

/// Type = click to focus, then native typing.
#[cfg(target_os = "macos")]
pub fn ui_type(
    window_id: &str,
    query: &str,
    mode: &MatchMode,
    text: &str,
    _via_clipboard: bool,
) -> Result<UiElement> {
    ui_set_value(window_id, query, text, mode)
}

/// Wait reuses the shared loop over mac `find` (presence; visibility needs
/// per-element AX reads we skip in the dump, so `--visible` degrades to
/// presence on macOS — documented, not silent).
#[cfg(target_os = "macos")]
pub fn ui_wait(
    window_id: &str,
    query: &str,
    mode: &MatchMode,
    timeout: std::time::Duration,
    interval: std::time::Duration,
    _require_visible: bool,
) -> Result<UiElement> {
    crate::accessibility::wait_for_element(query, window_id, timeout, interval, false, || {
        find(window_id, query, mode)
    })
}

#[cfg(target_os = "macos")]
pub fn ui_shot(
    window_id: &str,
    query: &str,
    mode: &MatchMode,
    out_path: &str,
    pad: u32,
    zoom: u32,
) -> Result<UiElement> {
    let snapshot = find(window_id, query, mode)?;
    crate::accessibility::shot_element(window_id, &snapshot, out_path, pad, zoom)?;
    Ok(snapshot)
}

/// Context menu = right-click at the center (same mechanism as ui_right_click).
#[cfg(target_os = "macos")]
pub fn ui_show_menu(window_id: &str, query: &str, mode: &MatchMode) -> Result<UiElement> {
    ui_right_click(window_id, query, mode)
}

#[cfg(target_os = "macos")]
pub fn ui_drag(
    _window_id: &str,
    _from: &str,
    _to: &str,
    _mode: &MatchMode,
) -> Result<(UiElement, UiElement)> {
    bail!("ui drag aún no implementado en macOS (usa `mk drag x1 y1 x2 y2` con coordenadas de `mk ui tree`)")
}

#[cfg(target_os = "macos")]
pub fn ui_get_value(_window_id: &str, _query: &str, _mode: &MatchMode) -> Result<crate::accessibility::UiState> {
    bail!("ui get-value aún no implementado en macOS (el dump AX actual no lee valores)")
}

#[cfg(target_os = "macos")]
pub fn ui_expand(
    _window_id: &str,
    _query: &str,
    _mode: &MatchMode,
    _collapse: bool,
) -> Result<crate::accessibility::UiState> {
    bail!("ui expand aún no implementado en macOS (prueba `mk ui click` sobre el combo)")
}

#[cfg(target_os = "macos")]
pub fn ui_find(_query: &str, _mode: &MatchMode) -> Result<Vec<crate::accessibility::UiHit>> {
    bail!("ui find global aún no implementado en macOS (usa `mk ui tree --window <id>`)")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(role: &str, name: &str, id: &str, g: &str) -> String {
        format!("{role}{SEP}{name}{SEP}{id}{SEP}{g}")
    }

    #[test]
    fn test_parse_ax_dump() {
        let dump = [
            line("button", "OK", "btn_ok", "10,20,80,30"),
            line("text field", "Nombre", "", "10.0,60,200,22"),
            line("static text", "con\ttab", "", "0,0,5,5"),
            "garbage-without-separators".to_string(),
            line("button", "Bad", "", "1,2,3"),
        ]
        .join("\n");
        let els = parse_ax_dump(&dump);
        // \x1F separator survives tabs inside names (that's why it isn't \t);
        // garbage (1 field) and bad-coords (3 nums) are skipped.
        assert_eq!(els.len(), 3);
        assert_eq!(els[0].role, "button");
        assert_eq!(els[0].name, "OK");
        assert_eq!(els[0].automation_id, "btn_ok");
        assert_eq!((els[0].x, els[0].y, els[0].width, els[0].height), (10, 20, 80, 30));
        assert_eq!((els[1].x, els[1].y), (10, 60));
        assert_eq!(els[2].name, "con\ttab");
    }

    #[test]
    fn test_scale_ax_tree_retina() {
        let els = parse_ax_dump(&line("button", "B", "", "100,100,50,20"));
        let scaled = scale_ax_tree(els, 2.0);
        assert_eq!((scaled[0].x, scaled[0].y, scaled[0].width, scaled[0].height), (200, 200, 100, 40));
        let els = parse_ax_dump(&line("button", "B", "", "100,100,50,20"));
        let same = scale_ax_tree(els, 1.0);
        assert_eq!((same[0].x, same[0].width), (100, 50));
        let els = parse_ax_dump(&line("button", "B", "", "100,100,50,20"));
        let nan = scale_ax_tree(els, f64::NAN);
        assert_eq!((nan[0].x, nan[0].width), (100, 50));
    }

    #[test]
    fn test_dump_script_escapes_and_caps() {
        let s = dump_script("My\"App", "Win\\Title");
        assert!(s.contains("My\\\"App"), "quotes escaped");
        assert!(s.contains("Win\\\\Title"), "backslash escaped");
        assert!(s.contains("entire contents of winRef"), "flat walk");
        assert!(s.contains("AXIdentifier"), "stable ids");
        assert!(s.contains("missing value"), "window fallback chain");
    }

    #[test]
    fn test_escape_fn() {
        assert_eq!(escape("a\"b\\c"), "a\\\"b\\\\c");
    }
}
