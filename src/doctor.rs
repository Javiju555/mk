use crate::input;
#[cfg(target_os = "linux")]
use crate::output::clipboard;
use anyhow::Result;

pub fn run() -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let server = input::detect_display_server();
        let clipboard_tool = clipboard::detect_clipboard_tool(server);

        println!("mk doctor — diagnostics\n");

        println!("Session:");
        println!("  display server: {server}");

        match std::env::var("XDG_SESSION_TYPE") {
            Ok(v) => println!("  XDG_SESSION_TYPE: {v}"),
            Err(_) => println!("  XDG_SESSION_TYPE: (not set)"),
        }
        match std::env::var("WAYLAND_DISPLAY") {
            Ok(v) => println!("  WAYLAND_DISPLAY: {v}"),
            Err(_) => println!("  WAYLAND_DISPLAY: (not set)"),
        }
        match std::env::var("DISPLAY") {
            Ok(v) => println!("  DISPLAY: {v}"),
            Err(_) => println!("  DISPLAY: (not set)"),
        }
        println!("  System Local Time: {}", chrono::Local::now());
        println!("  System UTC Time:   {}", chrono::Utc::now());


        println!("\nKeyboard backends:");
        check_tool("wtype");
        check_tool("xdotool");
        check_tool("ydotool");

        println!("\nClipboard tools:");
        check_tool("wl-copy");
        check_tool("xclip");
        check_tool("xsel");

        println!("\nDetected backend: {}", match input::detect_backend() {
            Ok(b) => b.display_name().to_string(),
            Err(e) => format!("error: {e}"),
        });

        println!("Detected clipboard: {}", match &clipboard_tool {
            clipboard::ClipboardTool::WlCopy => "wl-copy",
            clipboard::ClipboardTool::Xclip => "xclip",
            clipboard::ClipboardTool::Xsel => "xsel",
            clipboard::ClipboardTool::None => "none",
        });

        print_window_control();

        println!("\nRecommendations (pacman -S):");
        match server {
            input::DisplayServer::Wayland => {
                println!("  sudo pacman -S wtype wl-clipboard");
                if !tool_exists("ydotool") {
                    println!("  # optional: sudo pacman -S ydotool");
                }
            }
            input::DisplayServer::X11 => {
                println!("  sudo pacman -S xdotool xclip");
                if !tool_exists("wtype") {
                    println!("  # optional: sudo pacman -S wtype");
                }
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        println!("mk doctor — diagnostics\n");
        println!("Platform: {}", std::env::consts::OS);
        println!("  System Local Time: {}", chrono::Local::now());
        println!("  System UTC Time:   {}", chrono::Utc::now());

        println!("\nDetected backend: {}", match input::detect_backend() {
            Ok(b) => b.display_name().to_string(),
            Err(e) => format!("error: {e}"),
        });

        print_window_control();
    }

    Ok(())
}

/// Report the window-control capability of the current session so callers
/// (e.g. an AI agent) can plan instead of failing by trial-and-error.
/// See docs/window-control.md for the full matrix and rationale.
fn print_window_control() {
    println!("\nWindow control:");

    // Empirical: how many windows can we actually enumerate right now?
    let visible = crate::windows::list_windows().map(|w| w.len());

    #[cfg(target_os = "linux")]
    {
        let wayland = std::env::var("WAYLAND_DISPLAY").is_ok()
            || std::env::var("XDG_SESSION_TYPE")
                .map(|v| v.eq_ignore_ascii_case("wayland"))
                .unwrap_or(false);
        if wayland {
            println!("  backend: wayland (compositor-owned)");
            println!("  [✗] enumerate/focus foreign windows — not exposed by Wayland");
            println!("  [✓] input simulation (mk key/click/move) — use `mk key alt+tab` / overview");
            match visible {
                Ok(n) => println!(
                    "  xcb(XWayland) sees {n} window(s) — Wayland-native apps are invisible here"
                ),
                Err(e) => println!("  window enumeration error: {e}"),
            }
        } else {
            println!("  backend: x11 (xcb/EWMH)");
            println!("  [✓] enumerate/focus windows (X11 apps)");
            if let Ok(n) = visible {
                println!("  enumerated {n} window(s)");
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        println!("  backend: win32 (native)");
        println!("  [✓] enumerate/focus/move windows");
        if let Ok(n) = visible {
            println!("  enumerated {n} window(s)");
        }
    }

    #[cfg(target_os = "macos")]
    {
        println!("  backend: CoreGraphics + Accessibility");
        println!("  [✓] enumerate/focus windows (AX permission required)");
        if let Ok(n) = visible {
            println!("  enumerated {n} window(s)");
        }
    }
}

#[cfg(target_os = "linux")]
fn check_tool(name: &str) {
    let status = if tool_exists(name) { "✓" } else { "✗" };
    println!("  [{status}] {name}");
}

#[cfg(target_os = "linux")]
fn tool_exists(name: &str) -> bool {
    std::process::Command::new("which")
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
