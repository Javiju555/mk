//! Read the system clipboard (the reverse of `paste`: after an agent does
//! e.g. select-all + copy inside an app, the text can be pulled back out).
//!
//! Platform notes: Linux uses the same tool detection as `paste`
//! (`wl-paste` / `xclip -o` / `xsel -o`); macOS shells out to `pbpaste`;
//! Windows reads CF_UNICODETEXT natively. Content is returned raw
//! (no trimming) so agents see exactly what was copied.

use anyhow::Result;

#[cfg(target_os = "linux")]
use super::clipboard;
#[cfg(target_os = "linux")]
use crate::input;
#[cfg(any(target_os = "windows", target_os = "linux", target_os = "macos"))]
use anyhow::bail;
#[cfg(target_os = "linux")]
use std::process::Command;

pub fn read_clipboard() -> Result<String> {
    #[cfg(target_os = "linux")]
    {
        let server = input::detect_display_server();
        let tool = clipboard::detect_clipboard_tool(server);

        let output = match tool {
            clipboard::ClipboardTool::WlCopy => Command::new("wl-paste").output()?,
            clipboard::ClipboardTool::Xclip => Command::new("xclip")
                .args(["-selection", "clipboard", "-o"])
                .output()?,
            clipboard::ClipboardTool::Xsel => Command::new("xsel")
                .args(["--clipboard", "--output"])
                .output()?,
            clipboard::ClipboardTool::None => {
                bail!("No clipboard reader available. Install wl-clipboard (Wayland) or xclip/xsel (X11).")
            }
        };
        if !output.status.success() {
            bail!("Clipboard reader exited with status: {}", output.status);
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("pbpaste").output()?;
        if !output.status.success() {
            bail!("pbpaste exited with status: {}", output.status);
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    #[cfg(target_os = "windows")]
    {
        read_windows_clipboard()
    }
}

#[cfg(target_os = "windows")]
fn read_windows_clipboard() -> Result<String> {
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, OpenClipboard,
    };
    use windows_sys::Win32::System::Memory::{GlobalLock, GlobalUnlock};

    // CF_UNICODETEXT = 13 (not exported by windows-sys in this version)
    const CF_UNICODETEXT: u32 = 13;

    unsafe {
        if OpenClipboard(0) == 0 {
            bail!("OpenClipboard failed (is the clipboard locked by another app?)");
        }
        // Always release the clipboard, even on read errors.
        let text = (|| -> Result<String> {
            let handle = GetClipboardData(CF_UNICODETEXT);
            if handle == 0 {
                bail!("No text in clipboard");
            }
            let ptr = GlobalLock(handle as *mut std::ffi::c_void);
            if ptr.is_null() {
                bail!("GlobalLock failed");
            }
            let uptr = ptr as *const u16;
            let mut len = 0usize;
            while *uptr.add(len) != 0 {
                len += 1;
            }
            let slice = std::slice::from_raw_parts(uptr, len);
            let text = String::from_utf16_lossy(slice);
            GlobalUnlock(handle as *mut std::ffi::c_void);
            Ok(text)
        })();
        CloseClipboard();
        text
    }
}
