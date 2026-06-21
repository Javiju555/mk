#[cfg(target_os = "linux")]
use crate::backend;
use crate::backend::Backend;
#[cfg(target_os = "linux")]
use crate::clipboard;
#[cfg(target_os = "linux")]
use anyhow::bail;
use anyhow::Result;
#[cfg(target_os = "linux")]
use std::process::Command;

pub fn paste(text: &str, shortcut: &str, backend: &dyn Backend) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let server = backend::detect_display_server();
        let tool = clipboard::detect_clipboard_tool(server);

        match tool {
            clipboard::ClipboardTool::WlCopy => {
                let status = Command::new("wl-copy").arg(text).status()?;
                if !status.success() {
                    bail!("wl-copy exited with status: {status}");
                }
            }
            clipboard::ClipboardTool::Xclip => {
                use std::io::Write;
                let mut child = Command::new("xclip")
                    .args(["-selection", "clipboard"])
                    .stdin(std::process::Stdio::piped())
                    .spawn()?;
                if let Some(mut stdin) = child.stdin.take() {
                    stdin.write_all(text.as_bytes())?;
                }
                let status = child.wait()?;
                if !status.success() {
                    bail!("xclip exited with status: {status}");
                }
            }
            clipboard::ClipboardTool::Xsel => {
                use std::io::Write;
                let mut child = Command::new("xsel")
                    .args(["--clipboard", "--input"])
                    .stdin(std::process::Stdio::piped())
                    .spawn()?;
                if let Some(mut stdin) = child.stdin.take() {
                    stdin.write_all(text.as_bytes())?;
                }
                let status = child.wait()?;
                if !status.success() {
                    bail!("xsel exited with status: {status}");
                }
            }
            clipboard::ClipboardTool::None => {
                backend.type_text(text)?;
                return Ok(());
            }
        }

        backend.press_key(shortcut)?;
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = shortcut; // unused parameter warning bypass
        backend.type_text(text)?;
        Ok(())
    }
}
