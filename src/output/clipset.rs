#![cfg(target_os = "windows")]

//! Set the Windows clipboard to UTF-16 text (the write half; read lives in
//! `clipget`). Used by `mk paste` so long texts go through the clipboard +
//! Ctrl+V instead of key-by-key SendInput.

use anyhow::{Result, bail};
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows_sys::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};

// CF_UNICODETEXT = 13 (not exported by windows-sys in this version).
const CF_UNICODETEXT: u32 = 13;

pub fn set_clipboard_text(text: &str) -> Result<()> {
    unsafe {
        if OpenClipboard(0) == 0 {
            bail!("OpenClipboard failed (is the clipboard locked by another app?)");
        }
        // Always release the clipboard, even on write errors.
        let result = (|| -> Result<()> {
            EmptyClipboard();
            let utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            let h = GlobalAlloc(GMEM_MOVEABLE, utf16.len() * 2);
            if h.is_null() {
                bail!("GlobalAlloc failed");
            }
            let ptr = GlobalLock(h);
            if ptr.is_null() {
                bail!("GlobalLock failed");
            }
            std::ptr::copy_nonoverlapping(utf16.as_ptr(), ptr as *mut u16, utf16.len());
            GlobalUnlock(h);
            // NOTE: windows-sys 0.52 has no GlobalFree; a failed
            // SetClipboardData below leaks one small block on an
            // already-failing path. Accepted and documented.
            if SetClipboardData(CF_UNICODETEXT, h as HANDLE) == 0 {
                bail!("SetClipboardData failed");
            }
            Ok(())
        })();
        CloseClipboard();
        result
    }
}
