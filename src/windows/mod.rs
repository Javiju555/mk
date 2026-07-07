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

#[cfg(target_os = "windows")]
mod os_impl {
    use anyhow::{bail, Result};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetForegroundWindow, ShowWindow, SetWindowPos, PostMessageW,
        SW_RESTORE, SW_MINIMIZE, SW_MAXIMIZE, WM_CLOSE,
        SWP_NOSIZE, SWP_NOMOVE, SWP_NOZORDER, SWP_NOACTIVATE,
    };

    fn parse_hwnd(window_id: &str) -> Result<windows_sys::Win32::Foundation::HWND> {
        let hwnd_val: usize = window_id
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid window ID: {e}"))?;
        Ok(hwnd_val as windows_sys::Win32::Foundation::HWND)
    }

    pub fn focus_window(window_id: &str) -> Result<()> {
        let hwnd = parse_hwnd(window_id)?;
        unsafe {
            ShowWindow(hwnd, SW_RESTORE);
            if SetForegroundWindow(hwnd) == 0 {
                bail!("Failed to set foreground window");
            }
            Ok(())
        }
    }

    pub fn move_window(window_id: &str, x: i32, y: i32) -> Result<()> {
        let hwnd = parse_hwnd(window_id)?;
        unsafe {
            if SetWindowPos(hwnd, 0, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE) == 0 {
                bail!("Failed to move window");
            }
            Ok(())
        }
    }

    pub fn resize_window(window_id: &str, width: u32, height: u32) -> Result<()> {
        let hwnd = parse_hwnd(window_id)?;
        unsafe {
            if SetWindowPos(hwnd, 0, 0, 0, width as i32, height as i32, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE) == 0 {
                bail!("Failed to resize window");
            }
            Ok(())
        }
    }

    pub fn minimize_window(window_id: &str) -> Result<()> {
        let hwnd = parse_hwnd(window_id)?;
        unsafe {
            ShowWindow(hwnd, SW_MINIMIZE);
            Ok(())
        }
    }

    pub fn maximize_window(window_id: &str) -> Result<()> {
        let hwnd = parse_hwnd(window_id)?;
        unsafe {
            ShowWindow(hwnd, SW_MAXIMIZE);
            Ok(())
        }
    }

    pub fn restore_window(window_id: &str) -> Result<()> {
        let hwnd = parse_hwnd(window_id)?;
        unsafe {
            ShowWindow(hwnd, SW_RESTORE);
            Ok(())
        }
    }

    pub fn close_window(window_id: &str) -> Result<()> {
        let hwnd = parse_hwnd(window_id)?;
        unsafe {
            if PostMessageW(hwnd, WM_CLOSE, 0, 0) == 0 {
                bail!("Failed to post WM_CLOSE message to window");
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

    fn check_wayland_unsupported(op_name: &str) -> Result<()> {
        if is_wayland() {
            bail!(
                "{op_name}-by-id is not supported on Wayland: the compositor owns \
                 window management and exposes no generic protocol for it. \
                 Use input simulation or a compositor backend (hyprctl/swaymsg/kwin). \
                 See docs/window-control.md."
            );
        }
        Ok(())
    }

    pub fn focus_window(window_id: &str) -> Result<()> {
        check_wayland_unsupported("focus")?;
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

    pub fn move_window(window_id: &str, x: i32, y: i32) -> Result<()> {
        check_wayland_unsupported("move")?;
        let status = Command::new("xdotool")
            .args(["windowmove", window_id, &x.to_string(), &y.to_string()])
            .status();
        match status {
            Ok(s) if s.success() => Ok(()),
            Ok(_) => bail!("xdotool windowmove failed for id {window_id}"),
            Err(_) => bail!("cannot move window on X11: xdotool not found."),
        }
    }

    pub fn resize_window(window_id: &str, width: u32, height: u32) -> Result<()> {
        check_wayland_unsupported("resize")?;
        let status = Command::new("xdotool")
            .args(["windowsize", window_id, &width.to_string(), &height.to_string()])
            .status();
        match status {
            Ok(s) if s.success() => Ok(()),
            Ok(_) => bail!("xdotool windowsize failed for id {window_id}"),
            Err(_) => bail!("cannot resize window on X11: xdotool not found."),
        }
    }

    pub fn minimize_window(window_id: &str) -> Result<()> {
        check_wayland_unsupported("minimize")?;
        let status = Command::new("xdotool")
            .args(["windowminimize", window_id])
            .status();
        match status {
            Ok(s) if s.success() => Ok(()),
            Ok(_) => bail!("xdotool windowminimize failed for id {window_id}"),
            Err(_) => bail!("cannot minimize window on X11: xdotool not found."),
        }
    }

    pub fn maximize_window(window_id: &str) -> Result<()> {
        check_wayland_unsupported("maximize")?;
        let status = Command::new("wmctrl")
            .args(["-ir", window_id, "-b", "add,maximized_vert,maximized_horz"])
            .status();
        match status {
            Ok(s) if s.success() => Ok(()),
            Ok(_) => bail!("wmctrl maximize failed for id {window_id}"),
            Err(_) => bail!("cannot maximize window on X11: wmctrl not found."),
        }
    }

    pub fn restore_window(window_id: &str) -> Result<()> {
        check_wayland_unsupported("restore")?;
        let status = Command::new("wmctrl")
            .args(["-ir", window_id, "-b", "remove,maximized_vert,maximized_horz"])
            .status();
        match status {
            Ok(s) if s.success() => Ok(()),
            Ok(_) => bail!("wmctrl restore failed for id {window_id}"),
            Err(_) => bail!("cannot restore window on X11: wmctrl not found."),
        }
    }

    pub fn close_window(window_id: &str) -> Result<()> {
        check_wayland_unsupported("close")?;
        let status = Command::new("xdotool")
            .args(["windowclose", window_id])
            .status();
        match status {
            Ok(s) if s.success() => Ok(()),
            Ok(_) => bail!("xdotool windowclose failed for id {window_id}"),
            Err(_) => bail!("cannot close window on X11: xdotool not found."),
        }
    }
}

#[cfg(target_os = "macos")]
mod os_impl {
    use anyhow::{bail, Result};
    use std::process::Command;

    fn run_applescript(script: &str) -> Result<()> {
        let status = Command::new("osascript").args(["-e", script]).status()?;
        if status.success() {
            Ok(())
        } else {
            bail!("AppleScript execution failed")
        }
    }

    fn escape(s: &str) -> String {
        s.replace('\\', "\\\\").replace('"', "\\\"")
    }

    pub fn focus_window(app_name: &str, title: &str) -> Result<()> {
        let app = escape(app_name);
        let title_esc = escape(title);
        let script = format!(
            "tell application \"System Events\"\n\
             tell process \"{app}\"\n\
             set frontmost to true\n\
             try\n\
             perform action \"AXRaise\" of (first window whose name is \"{title_esc}\")\n\
             on error\n\
             try\n\
             perform action \"AXRaise\" of (first window whose name contains \"{title_esc}\")\n\
             on error\n\
             perform action \"AXRaise\" of window 1\n\
             end try\n\
             end try\n\
             end tell\n\
             end tell\n\
             tell application \"{app}\" to activate"
        );
        run_applescript(&script)
    }

    pub fn move_window(app_name: &str, title: &str, x: i32, y: i32) -> Result<()> {
        let app = escape(app_name);
        let title_esc = escape(title);
        let script = format!(
            "tell application \"System Events\"\n\
             tell process \"{app}\"\n\
             try\n\
             set position of (first window whose name is \"{title_esc}\") to {{ {x}, {y} }}\n\
             on error\n\
             try\n\
             set position of (first window whose name contains \"{title_esc}\") to {{ {x}, {y} }}\n\
             on error\n\
             set position of window 1 to {{ {x}, {y} }}\n\
             end try\n\
             end try\n\
             end tell\n\
             end tell"
        );
        run_applescript(&script)
    }

    pub fn resize_window(app_name: &str, title: &str, width: u32, height: u32) -> Result<()> {
        let app = escape(app_name);
        let title_esc = escape(title);
        let script = format!(
            "tell application \"System Events\"\n\
             tell process \"{app}\"\n\
             try\n\
             set size of (first window whose name is \"{title_esc}\") to {{ {width}, {height} }}\n\
             on error\n\
             try\n\
             set size of (first window whose name contains \"{title_esc}\") to {{ {width}, {height} }}\n\
             on error\n\
             set size of window 1 to {{ {width}, {height} }}\n\
             end try\n\
             end try\n\
             end tell\n\
             end tell"
        );
        run_applescript(&script)
    }

    pub fn minimize_window(app_name: &str, title: &str) -> Result<()> {
        let app = escape(app_name);
        let title_esc = escape(title);
        let script = format!(
            "tell application \"System Events\"\n\
             tell process \"{app}\"\n\
             try\n\
             set value of attribute \"AXMinimized\" of (first window whose name is \"{title_esc}\") to true\n\
             on error\n\
             try\n\
             set value of attribute \"AXMinimized\" of (first window whose name contains \"{title_esc}\") to true\n\
             on error\n\
             set value of attribute \"AXMinimized\" of window 1 to true\n\
             end try\n\
             end try\n\
             end tell\n\
             end tell"
        );
        run_applescript(&script)
    }

    pub fn maximize_window(app_name: &str, title: &str) -> Result<()> {
        let app = escape(app_name);
        let title_esc = escape(title);
        let script = format!(
            "tell application \"System Events\"\n\
             tell process \"{app}\"\n\
             try\n\
             set value of attribute \"AXZoomed\" of (first window whose name is \"{title_esc}\") to true\n\
             on error\n\
             try\n\
             set value of attribute \"AXZoomed\" of (first window whose name contains \"{title_esc}\") to true\n\
             on error\n\
             set value of attribute \"AXZoomed\" of window 1 to true\n\
             end try\n\
             end try\n\
             end tell\n\
             end tell"
        );
        run_applescript(&script)
    }

    pub fn restore_window(app_name: &str, title: &str) -> Result<()> {
        let app = escape(app_name);
        let title_esc = escape(title);
        let script = format!(
            "tell application \"System Events\"\n\
             tell process \"{app}\"\n\
             try\n\
             set value of attribute \"AXMinimized\" of (first window whose name is \"{title_esc}\") to false\n\
             on error\n\
             try\n\
             set value of attribute \"AXMinimized\" of (first window whose name contains \"{title_esc}\") to false\n\
             on error\n\
             set value of attribute \"AXMinimized\" of window 1 to false\n\
             end try\n\
             end try\n\
             try\n\
             set value of attribute \"AXZoomed\" of (first window whose name is \"{title_esc}\") to false\n\
             on error\n\
             try\n\
             set value of attribute \"AXZoomed\" of (first window whose name contains \"{title_esc}\") to false\n\
             on error\n\
             set value of attribute \"AXZoomed\" of window 1 to false\n\
             end try\n\
             end try\n\
             end tell\n\
             end tell"
        );
        run_applescript(&script)
    }

    pub fn close_window(app_name: &str, title: &str) -> Result<()> {
        let app = escape(app_name);
        let title_esc = escape(title);
        let script = format!(
            "tell application \"{app}\"\n\
             try\n\
             close (first window whose name is \"{title_esc}\")\n\
             on error\n\
             try\n\
             close (first window whose name contains \"{title_esc}\")\n\
             on error\n\
             close window 1\n\
             end try\n\
             end try\n\
             end tell"
        );
        run_applescript(&script)
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
mod os_impl {
    use anyhow::{bail, Result};
    pub fn focus_window(_window_id: &str) -> Result<()> {
        bail!("Platform not supported")
    }
    pub fn move_window(_window_id: &str, _x: i32, _y: i32) -> Result<()> {
        bail!("Platform not supported")
    }
    pub fn resize_window(_window_id: &str, _width: u32, _height: u32) -> Result<()> {
        bail!("Platform not supported")
    }
    pub fn minimize_window(_window_id: &str) -> Result<()> {
        bail!("Platform not supported")
    }
    pub fn maximize_window(_window_id: &str) -> Result<()> {
        bail!("Platform not supported")
    }
    pub fn restore_window(_window_id: &str) -> Result<()> {
        bail!("Platform not supported")
    }
    pub fn close_window(_window_id: &str) -> Result<()> {
        bail!("Platform not supported")
    }
}

/// Enumerate all on-screen windows with geometry and focused state.
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

/// Helper to find a window by its ID and return a clone of WindowInfo.
#[allow(dead_code)]
fn find_window(window_id: &str) -> Result<WindowInfo> {
    let list = list_windows()?;
    list.into_iter()
        .find(|w| w.id == window_id)
        .ok_or_else(|| anyhow::anyhow!("Window with ID {window_id} not found"))
}

/// Raise the window with the given id to the foreground (best-effort, per-OS).
pub fn focus_window(window_id: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let w = find_window(window_id)?;
        os_impl::focus_window(&w.app_name, &w.title)
    }

    #[cfg(not(target_os = "macos"))]
    {
        os_impl::focus_window(window_id)
    }
}

/// Move the window to the specified coordinates.
pub fn move_window(window_id: &str, x: i32, y: i32) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let w = find_window(window_id)?;
        os_impl::move_window(&w.app_name, &w.title, x, y)
    }

    #[cfg(not(target_os = "macos"))]
    {
        os_impl::move_window(window_id, x, y)
    }
}

/// Resize the window to the specified width and height.
pub fn resize_window(window_id: &str, width: u32, height: u32) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let w = find_window(window_id)?;
        os_impl::resize_window(&w.app_name, &w.title, width, height)
    }

    #[cfg(not(target_os = "macos"))]
    {
        os_impl::resize_window(window_id, width, height)
    }
}

/// Minimize the window.
pub fn minimize_window(window_id: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let w = find_window(window_id)?;
        os_impl::minimize_window(&w.app_name, &w.title)
    }

    #[cfg(not(target_os = "macos"))]
    {
        os_impl::minimize_window(window_id)
    }
}

/// Maximize the window.
pub fn maximize_window(window_id: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let w = find_window(window_id)?;
        os_impl::maximize_window(&w.app_name, &w.title)
    }

    #[cfg(not(target_os = "macos"))]
    {
        os_impl::maximize_window(window_id)
    }
}

/// Restore the window from a minimized or maximized state.
pub fn restore_window(window_id: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let w = find_window(window_id)?;
        os_impl::restore_window(&w.app_name, &w.title)
    }

    #[cfg(not(target_os = "macos"))]
    {
        os_impl::restore_window(window_id)
    }
}

/// Close the window.
pub fn close_window(window_id: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let w = find_window(window_id)?;
        os_impl::close_window(&w.app_name, &w.title)
    }

    #[cfg(not(target_os = "macos"))]
    {
        os_impl::close_window(window_id)
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
