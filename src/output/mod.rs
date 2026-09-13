pub mod paste;
pub mod clipget;

#[cfg(target_os = "windows")]
pub mod clipset;

#[cfg(target_os = "linux")]
pub mod clipboard;
