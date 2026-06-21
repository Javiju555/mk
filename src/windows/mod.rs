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

#[cfg(target_os = "windows")]
mod os_impl {
    use anyhow::{bail, Result};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, SetForegroundWindow, ShowWindow, SW_RESTORE};

    pub fn get_active_window_id() -> Result<u32> {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd == 0 {
                bail!("No active window found");
            }
            Ok(hwnd as u32)
        }
    }

    pub fn focus_window(window_id: &str) -> Result<()> {
        let hwnd_val: u32 = window_id.parse().map_err(|e| anyhow::anyhow!("Invalid window ID: {e}"))?;
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

    pub fn get_active_window_id() -> Result<u32> {
        let output = Command::new("xdotool")
            .arg("getactivewindow")
            .output();
        if let Ok(out) = output {
            if out.status.success() {
                let s = String::from_utf8_lossy(&out.stdout);
                if let Ok(id) = s.trim().parse::<u32>() {
                    return Ok(id);
                }
            }
        }
        bail!("Could not detect active window ID on Linux (requires xdotool)")
    }

    pub fn focus_window(window_id: &str) -> Result<()> {
        let status = Command::new("xdotool")
            .arg("windowactivate")
            .arg(window_id)
            .status();
        match status {
            Ok(s) if s.success() => Ok(()),
            _ => bail!("Failed to focus window using xdotool"),
        }
    }
}

#[cfg(target_os = "macos")]
mod os_impl {
    use anyhow::{bail, Result};
    use std::process::Command;

    #[allow(dead_code)]
    pub fn get_active_window_id() -> Result<u32> {
        // Return 0 placeholder, frontmost application is matched by app_name
        Ok(0)
    }

    pub fn focus_window(app_name: &str) -> Result<()> {
        let script = format!("activate application \"{}\"", app_name);
        let status = Command::new("osascript")
            .args(&["-e", &script])
            .status();
        match status {
            Ok(s) if s.success() => Ok(()),
            _ => bail!("Failed to activate application via AppleScript"),
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
mod os_impl {
    use anyhow::{bail, Result};
    pub fn get_active_window_id() -> Result<u32> {
        bail!("Platform not supported")
    }
    pub fn focus_window(_window_id: &str) -> Result<()> {
        bail!("Platform not supported")
    }
}

pub fn list_windows() -> Result<Vec<WindowInfo>> {
    let windows = Window::all().map_err(|e| anyhow::anyhow!("Failed to list windows: {e}"))?;
    #[cfg(not(target_os = "macos"))]
    let active_id = os_impl::get_active_window_id().ok();
    
    #[cfg(target_os = "macos")]
    let frontmost_app = get_macos_frontmost_app().ok();

    let mut list = Vec::new();
    for w in windows {
        let w_id = match w.id() {
            Ok(id) => id,
            Err(_) => continue,
        };
        let w_title = match w.title() {
            Ok(title) => title,
            Err(_) => continue,
        };
        let w_app_name = match w.app_name() {
            Ok(app_name) => app_name,
            Err(_) => continue,
        };
        let w_x = match w.x() {
            Ok(x) => x,
            Err(_) => continue,
        };
        let w_y = match w.y() {
            Ok(y) => y,
            Err(_) => continue,
        };
        let w_width = match w.width() {
            Ok(width) => width,
            Err(_) => continue,
        };
        let w_height = match w.height() {
            Ok(height) => height,
            Err(_) => continue,
        };

        let is_active = {
            #[cfg(target_os = "macos")]
            {
                frontmost_app.as_deref() == Some(&w_app_name)
            }
            #[cfg(not(target_os = "macos"))]
            {
                active_id.map_or(false, |id| id == w_id)
            }
        };

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

pub fn active_window() -> Result<WindowInfo> {
    let list = list_windows()?;
    list.into_iter()
        .find(|w| w.is_active)
        .context("No active window found")
}

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

#[cfg(target_os = "macos")]
fn get_macos_frontmost_app() -> Result<String> {
    use std::process::Command;
    let output = Command::new("osascript")
        .args(&["-e", "tell application \"System Events\" to get name of first application process whose frontmost is true"])
        .output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        anyhow::bail!("AppleScript failed")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_windows() {
        // Listing windows should run without errors (even if no GUI window is present, it will return empty list or find terminals)
        let list = list_windows();
        assert!(list.is_ok());
    }
}
