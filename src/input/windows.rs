#![cfg(target_os = "windows")]

use super::Backend;
use anyhow::Result;
use std::mem::size_of;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;

pub struct WindowsBackend;

impl Backend for WindowsBackend {
    fn type_text(&self, text: &str) -> Result<()> {
        // To type text, we can use SendInput with KEYEVENTF_UNICODE
        for ch in text.encode_utf16() {
            let mut inputs = [
                INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: 0,
                            wScan: ch,
                            dwFlags: KEYEVENTF_UNICODE,
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                },
                INPUT {
                    r#type: INPUT_KEYBOARD,
                    Anonymous: INPUT_0 {
                        ki: KEYBDINPUT {
                            wVk: 0,
                            wScan: ch,
                            dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                            time: 0,
                            dwExtraInfo: 0,
                        },
                    },
                },
            ];

            unsafe {
                SendInput(
                    inputs.len() as u32,
                    inputs.as_mut_ptr(),
                    size_of::<INPUT>() as i32,
                );
            }
        }
        Ok(())
    }

    fn press_key(&self, key: &str) -> Result<()> {
        // Map key string to virtual key code (wVk)
        let vk = match key.to_lowercase().as_str() {
            "enter" => VK_RETURN,
            "backspace" => VK_BACK,
            "tab" => VK_TAB,
            "escape" | "esc" => VK_ESCAPE,
            "space" => VK_SPACE,
            "up" => VK_UP,
            "down" => VK_DOWN,
            "left" => VK_LEFT,
            "right" => VK_RIGHT,
            "pageup" | "pgup" => VK_PRIOR,
            "pagedown" | "pgdn" => VK_NEXT,
            "home" => VK_HOME,
            "end" => VK_END,
            "insert" => VK_INSERT,
            "delete" | "del" => VK_DELETE,
            "ctrl" | "control" => VK_CONTROL,
            "shift" => VK_SHIFT,
            "alt" => VK_MENU,
            "super" | "win" | "meta" => VK_LWIN, // Left Windows key
            _ => {
                // If it's a single character, we might map it or try to map it using VkKeyScan
                if key.len() == 1 {
                    let ch = key.chars().next().unwrap() as u16;
                    unsafe { VkKeyScanW(ch) as u16 & 0xFF }
                } else {
                    anyhow::bail!("Unsupported key: {}", key);
                }
            }
        };

        let mut inputs = [
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: vk,
                        wScan: 0,
                        dwFlags: 0,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: vk,
                        wScan: 0,
                        dwFlags: KEYEVENTF_KEYUP,
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            },
        ];

        unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_mut_ptr(),
                size_of::<INPUT>() as i32,
            );
        }
        Ok(())
    }

    fn display_name(&self) -> &str {
        "Windows native"
    }

    fn mouse_move(&self, x: i32, y: i32, duration_ms: u64) -> Result<()> {
        // Win32 SendInput coordinates for absolute mouse movement are normalized from 0 to 65535.
        // But SendInput also has MOUSEEVENTF_MOVE. If we want raw screen pixels, we can also use set_cursor_pos.
        // Wait, set_cursor_pos is simple and works in pixel coordinates!
        // Let's check: Win32 has SetCursorPos(x, y).
        // Let's use SetCursorPos for mouse movement because it takes pixel coordinates directly, and is much cleaner and more accurate than SendInput's normalized coordinates.
        // Wait! What about duration_ms for smooth movement?
        // If duration_ms > 0, we can interpolate the movement.
        // Let's do interpolation using SetCursorPos!
        // To get the start position: GetCursorPos.
        
        let start_pos = unsafe {
            let mut pos = std::mem::zeroed();
            if windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pos) != 0 {
                (pos.x, pos.y)
            } else {
                (0, 0)
            }
        };

        if duration_ms == 0 {
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::SetCursorPos(x, y);
            }
        } else {
            let steps = (duration_ms / 10).max(1) as f64;
            let step_delay = std::time::Duration::from_millis((duration_ms as f64 / steps) as u64);
            for i in 1..=steps as i32 {
                let t = i as f64 / steps;
                let cur_x = start_pos.0 + ((x - start_pos.0) as f64 * t) as i32;
                let cur_y = start_pos.1 + ((y - start_pos.1) as f64 * t) as i32;
                unsafe {
                    windows_sys::Win32::UI::WindowsAndMessaging::SetCursorPos(cur_x, cur_y);
                }
                std::thread::sleep(step_delay);
            }
            // Final snap to target
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::SetCursorPos(x, y);
            }
        }
        Ok(())
    }

    fn mouse_click(&self, x: i32, y: i32, button: &str, duration_ms: u64) -> Result<()> {
        self.mouse_move(x, y, duration_ms)?;
        self.mouse_down(button)?;
        // Short delay to register the click
        std::thread::sleep(std::time::Duration::from_millis(50));
        self.mouse_up(button)?;
        Ok(())
    }

    fn mouse_drag(&self, x1: i32, y1: i32, x2: i32, y2: i32, duration_ms: u64) -> Result<()> {
        self.mouse_move(x1, y1, 0)?;
        self.mouse_down("left")?;
        std::thread::sleep(std::time::Duration::from_millis(50));
        self.mouse_move(x2, y2, duration_ms)?;
        std::thread::sleep(std::time::Duration::from_millis(50));
        self.mouse_up("left")?;
        Ok(())
    }

    fn mouse_down(&self, button: &str) -> Result<()> {
        let flag = match button.to_lowercase().as_str() {
            "left" | "l" => MOUSEEVENTF_LEFTDOWN,
            "right" | "r" => MOUSEEVENTF_RIGHTDOWN,
            "middle" | "m" => MOUSEEVENTF_MIDDLEDOWN,
            _ => anyhow::bail!("Unsupported mouse button: {}", button),
        };

        let mut input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: 0,
                    dwFlags: flag,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };

        unsafe {
            SendInput(1, &mut input, size_of::<INPUT>() as i32);
        }
        Ok(())
    }

    fn mouse_up(&self, button: &str) -> Result<()> {
        let flag = match button.to_lowercase().as_str() {
            "left" | "l" => MOUSEEVENTF_LEFTUP,
            "right" | "r" => MOUSEEVENTF_RIGHTUP,
            "middle" | "m" => MOUSEEVENTF_MIDDLEUP,
            _ => anyhow::bail!("Unsupported mouse button: {}", button),
        };

        let mut input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: 0,
                    dwFlags: flag,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };

        unsafe {
            SendInput(1, &mut input, size_of::<INPUT>() as i32);
        }
        Ok(())
    }

    fn mouse_scroll(&self, clicks: i32, horizontal: bool) -> Result<()> {
        let flag = if horizontal {
            MOUSEEVENTF_HWHEEL
        } else {
            MOUSEEVENTF_WHEEL
        };
        // In Windows, mouseData is the wheel movement. A positive value indicates that the wheel was rotated forward, away from the user;
        // a negative value indicates that the wheel was rotated backward, toward the user.
        // WHEEL_DELTA is 120.
        let mouse_data = clicks * 120;

        let mut input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: mouse_data as u32,
                    dwFlags: flag,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };

        unsafe {
            SendInput(1, &mut input, size_of::<INPUT>() as i32);
        }
        Ok(())
    }
}
