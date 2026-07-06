use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use xcap::Window;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub id: String,
    pub title: String,
    pub app_name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub is_active: bool,
}

impl WindowInfo {
    /// Center point of the window — the natural target for a `mk click` that
    /// wants to focus/raise this window by clicking its body.
    pub fn center(&self) -> (i32, i32) {
        (
            self.x + (self.width as i32) / 2,
            self.y + (self.height as i32) / 2,
        )
    }
}

// ── Platform-specific focus (raising a window to the foreground) ────────────
//
// Enumeration + geometry + focused-state come from `xcap` (cross-platform,
// below). Only *changing* focus needs per-OS native calls, and only that is
// specialized here.

#[cfg(target_os = "windows")]
mod os_impl {
    use anyhow::{bail, Result};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetForegroundWindow, ShowWindow, SW_RESTORE,
    };

    pub fn focus_window(window_id: &str) -> Result<()> {
        let hwnd_val: u32 = window_id
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid window ID: {e}"))?;
        unsafe {
            let hwnd = hwnd_val as windows_sys::Win32::Foundation::HWND;
            ShowWindow(hwnd, SW_RESTORE);
            if SetForegroundWindow(hwnd) == 0 {
                bail!("Failed to set foreground window");
            }
            Ok(())
        }
    }
}

#[cfg(target_os = "linux")]
mod os_impl {
    use anyhow::{bail, Result};
    use std::process::Command;

    /// True when the current Linux session is Wayland (as opposed to X11).
    fn is_wayland() -> bool {
        std::env::var("WAYLAND_DISPLAY").is_ok()
            || std::env::var("XDG_SESSION_TYPE")
                .map(|v| v.eq_ignore_ascii_case("wayland"))
                .unwrap_or(false)
    }

    pub fn focus_window(window_id: &str) -> Result<()> {
        // Wayland deliberately forbids a client from raising an arbitrary other
        // window by id — the compositor is the sole focus authority. There is
        // no DE-independent syscall for this on Wayland. Be honest instead of
        // shelling out to xdotool (X11-only, and typically absent on Wayland).
        if is_wayland() {
            bail!(
                "focus-by-id is not supported on Wayland: the compositor owns \
                 window focus and exposes no generic protocol for it. Use input \
                 simulation instead (e.g. `mk key alt+tab`, or click the window's \
                 body via `mk window list` → center coords), or a compositor \
                 backend (hyprctl/swaymsg/kwin). See docs/window-control.md."
            );
        }
        // X11 path — no external tool if xdotool is missing; report clearly.
        let status = Command::new("xdotool")
            .arg("windowactivate")
            .arg(window_id)
            .status();
        match status {
            Ok(s) if s.success() => Ok(()),
            Ok(_) => bail!("xdotool windowactivate failed for id {window_id}"),
            Err(_) => bail!(
                "cannot focus window on X11: xdotool not found. Install it, or \
                 use input simulation. See docs/window-control.md."
            ),
        }
    }
}

#[cfg(target_os = "macos")]
mod os_impl {
    use anyhow::{bail, Result};
    use std::process::Command;

    pub fn focus_window(app_name: &str) -> Result<()> {
        let script = format!("activate application \"{}\"", app_name);
        let status = Command::new("osascript").args(["-e", &script]).status();
        match status {
            Ok(s) if s.success() => Ok(()),
            _ => bail!("Failed to activate application via AppleScript"),
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
mod os_impl {
    use anyhow::{bail, Result};
    pub fn focus_window(_window_id: &str) -> Result<()> {
        bail!("Platform not supported")
    }
}

/// Enumerate all on-screen windows with geometry and focused state.
///
/// Enumeration and the `is_active` flag come from `xcap`'s native
/// `Window::is_focused()` — no external tools (xdotool/wmctrl) and no
/// dependency on a specific desktop environment for *detection*. Note: on a
/// Linux Wayland session xcap enumerates via XCB (XWayland), so Wayland-native
/// windows may be invisible here — see docs/window-control.md.
pub fn list_windows() -> Result<Vec<WindowInfo>> {
    let windows = Window::all().map_err(|e| anyhow::anyhow!("Failed to list windows: {e}"))?;

    let mut list = Vec::new();
    for w in windows {
        let (Ok(w_id), Ok(w_title), Ok(w_app_name)) = (w.id(), w.title(), w.app_name()) else {
            continue;
        };
        let (Ok(w_x), Ok(w_y), Ok(w_width), Ok(w_height)) =
            (w.x(), w.y(), w.width(), w.height())
        else {
            continue;
        };
        // Native focused-state; degrade to false if the backend can't tell.
        let is_active = w.is_focused().unwrap_or(false);

        list.push(WindowInfo {
            id: w_id.to_string(),
            title: w_title,
            app_name: w_app_name,
            x: w_x,
            y: w_y,
            width: w_width,
            height: w_height,
            is_active,
        });
    }
    Ok(list)
}

/// The currently focused window, if any backend can report it.
pub fn active_window() -> Result<WindowInfo> {
    let list = list_windows()?;
    list.into_iter()
        .find(|w| w.is_active)
        .context("No active window reported (backend may not expose focus on this session)")
}

/// Raise the window with the given id to the foreground (best-effort, per-OS).
pub fn focus_window(window_id: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let list = list_windows()?;
        if let Some(w) = list.iter().find(|w| w.id == window_id) {
            return os_impl::focus_window(&w.app_name);
        }
        anyhow::bail!("Window not found")
    }

    #[cfg(not(target_os = "macos"))]
    {
        os_impl::focus_window(window_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_windows_runs() {
        // Must not error even in headless/odd sessions (may return an empty list).
        let list = list_windows();
        assert!(list.is_ok());
    }

    #[test]
    fn test_center_is_midpoint() {
        let w = WindowInfo {
            id: "1".into(),
            title: "t".into(),
            app_name: "a".into(),
            x: 100,
            y: 200,
            width: 400,
            height: 300,
            is_active: false,
        };
        assert_eq!(w.center(), (300, 350));
    }
}
