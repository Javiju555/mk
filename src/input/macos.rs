#![cfg(target_os = "macos")]

use super::Backend;
use anyhow::{Result, anyhow};
use core_graphics::event::{
    CGEvent, CGEventTapLocation, CGEventType, CGMouseButton, CGEventFlags,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::geometry::CGPoint;
use foreign_types::ForeignType;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventCreateScrollWheelEvent(
        source: core_graphics::sys::CGEventSourceRef,
        units: u32,
        wheelCount: u32,
        wheel1: i32,
        ...
    ) -> core_graphics::sys::CGEventRef;
}

pub struct MacosBackend;

/// Scale factor of the primary monitor (e.g. 2.0 on Retina), used to convert
/// mk's physical-pixel input into the logical points CGPoint expects. Falls
/// back to 1.0 (no-op) if xcap is unavailable or reports something unusable.
fn primary_scale_factor() -> f64 {
    if let Ok(monitors) = xcap::Monitor::all() {
        if let Some(m) = monitors.first() {
            if let Ok(scale) = m.scale_factor() {
                if scale.is_finite() && scale > 0.0 {
                    return scale as f64;
                }
            }
        }
    }
    1.0
}

impl Backend for MacosBackend {
    fn type_text(&self, text: &str) -> Result<()> {
        let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
            .map_err(|_| anyhow!("Failed to create CGEventSource"))?;
        
        // We create a keyboard event (keycode 0 as dummy) and set the Unicode string
        let event = CGEvent::new_keyboard_event(source, 0, true)
            .map_err(|_| anyhow!("Failed to create keyboard event"))?;
        
        event.set_string(text);
        event.post(CGEventTapLocation::HID);
        Ok(())
    }

    fn press_key(&self, key: &str) -> Result<()> {
        let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
            .map_err(|_| anyhow!("Failed to create CGEventSource"))?;

        if key.contains('+') {
            return self.press_combo(&source, key);
        }

        let keycode = convert_key(key)?;

        let event_down = CGEvent::new_keyboard_event(source.clone(), keycode, true)
            .map_err(|_| anyhow!("Failed to create keyboard down event"))?;
        let event_up = CGEvent::new_keyboard_event(source, keycode, false)
            .map_err(|_| anyhow!("Failed to create keyboard up event"))?;

        event_down.post(CGEventTapLocation::HID);
        std::thread::sleep(std::time::Duration::from_millis(10));
        event_up.post(CGEventTapLocation::HID);

        Ok(())
    }

    fn display_name(&self) -> &str {
        "macOS native"
    }

    fn mouse_move(&self, x: i32, y: i32, duration_ms: u64) -> Result<()> {
        let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
            .map_err(|_| anyhow!("Failed to create CGEventSource"))?;

        // To get the start position, we can create an empty/dummy event
        let start_pos = if let Ok(current_event) = CGEvent::new(source.clone()) {
            current_event.location()
        } else {
            CGPoint::new(0.0, 0.0)
        };

        // `x,y` arrive in physical pixels (mk's screenshot/click contract);
        // CGPoint expects logical points, so convert before building the
        // target. See src/input/scaling.rs module doc. start_pos above is a
        // live OS readback already in logical points — left untouched.
        // NEEDS validation on real macOS hardware (none available here);
        // logic mirrors the already-validated Linux HiDPI fix (commit 8680594).
        let scale = primary_scale_factor();
        let (logical_x, logical_y) = crate::input::scaling::physical_to_logical(x, y, scale);
        let target_pos = CGPoint::new(logical_x, logical_y);

        if duration_ms == 0 {
            let event = CGEvent::new_mouse_event(
                source,
                CGEventType::MouseMoved,
                target_pos,
                CGMouseButton::Left, // ignored for movement
            ).map_err(|_| anyhow!("Failed to create mouse move event"))?;
            event.post(CGEventTapLocation::HID);
        } else {
            let steps = (duration_ms / 10).max(1) as f64;
            let step_delay = std::time::Duration::from_millis((duration_ms as f64 / steps) as u64);
            for i in 1..=steps as i32 {
                let t = i as f64 / steps;
                let cur_x = start_pos.x + (target_pos.x - start_pos.x) * t;
                let cur_y = start_pos.y + (target_pos.y - start_pos.y) * t;
                let cur_pos = CGPoint::new(cur_x, cur_y);
                
                let event = CGEvent::new_mouse_event(
                    source.clone(),
                    CGEventType::MouseMoved,
                    cur_pos,
                    CGMouseButton::Left,
                ).map_err(|_| anyhow!("Failed to create mouse move event"))?;
                event.post(CGEventTapLocation::HID);
                
                std::thread::sleep(step_delay);
            }
            // Final snap to target
            let event = CGEvent::new_mouse_event(
                source,
                CGEventType::MouseMoved,
                target_pos,
                CGMouseButton::Left,
            ).map_err(|_| anyhow!("Failed to create mouse move event"))?;
            event.post(CGEventTapLocation::HID);
        }

        Ok(())
    }

    fn mouse_click(&self, x: i32, y: i32, button: &str, duration_ms: u64) -> Result<()> {
        self.mouse_move(x, y, duration_ms)?;
        self.mouse_down(button)?;
        std::thread::sleep(std::time::Duration::from_millis(50));
        self.mouse_up(button)?;
        Ok(())
    }

    fn mouse_drag(&self, x1: i32, y1: i32, x2: i32, y2: i32, duration_ms: u64) -> Result<()> {
        let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
            .map_err(|_| anyhow!("Failed to create CGEventSource"))?;

        // 1. Move to start coordinates (mouse_move already converts physical
        // -> logical internally, so no double conversion happens there)
        self.mouse_move(x1, y1, 0)?;

        // x1,y1/x2,y2 arrive in physical pixels; convert to logical points
        // for the CGPoints built locally in this function. See
        // src/input/scaling.rs module doc. NEEDS validation on real macOS
        // hardware (none available here); mirrors the validated Linux fix.
        let scale = primary_scale_factor();
        let (start_x, start_y) = crate::input::scaling::physical_to_logical(x1, y1, scale);
        let (end_x, end_y) = crate::input::scaling::physical_to_logical(x2, y2, scale);

        // 2. Press left button down at start
        let start_pos = CGPoint::new(start_x, start_y);
        let event_down = CGEvent::new_mouse_event(
            source.clone(),
            CGEventType::LeftMouseDown,
            start_pos,
            CGMouseButton::Left,
        ).map_err(|_| anyhow!("Failed to create mouse down event"))?;
        event_down.post(CGEventTapLocation::HID);
        std::thread::sleep(std::time::Duration::from_millis(50));

        // 3. Move to end coordinates simulating dragging
        let target_pos = CGPoint::new(end_x, end_y);
        if duration_ms == 0 {
            let event_drag = CGEvent::new_mouse_event(
                source.clone(),
                CGEventType::LeftMouseDragged,
                target_pos,
                CGMouseButton::Left,
            ).map_err(|_| anyhow!("Failed to create drag event"))?;
            event_drag.post(CGEventTapLocation::HID);
        } else {
            let steps = (duration_ms / 10).max(1) as f64;
            let step_delay = std::time::Duration::from_millis((duration_ms as f64 / steps) as u64);
            for i in 1..=steps as i32 {
                let t = i as f64 / steps;
                let cur_x = start_x + (end_x - start_x) * t;
                let cur_y = start_y + (end_y - start_y) * t;
                let cur_pos = CGPoint::new(cur_x, cur_y);
                let event_drag = CGEvent::new_mouse_event(
                    source.clone(),
                    CGEventType::LeftMouseDragged,
                    cur_pos,
                    CGMouseButton::Left,
                ).map_err(|_| anyhow!("Failed to create drag event"))?;
                event_drag.post(CGEventTapLocation::HID);
                std::thread::sleep(step_delay);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));

        // 4. Release mouse button at end
        let event_up = CGEvent::new_mouse_event(
            source,
            CGEventType::LeftMouseUp,
            target_pos,
            CGMouseButton::Left,
        ).map_err(|_| anyhow!("Failed to create mouse up event"))?;
        event_up.post(CGEventTapLocation::HID);

        Ok(())
    }

    fn mouse_down(&self, button: &str) -> Result<()> {
        let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
            .map_err(|_| anyhow!("Failed to create CGEventSource"))?;
        
        let current_event = CGEvent::new(source.clone())
            .map_err(|_| anyhow!("Failed to get current mouse position"))?;
        let pos = current_event.location();

        let (event_type, btn) = match button.to_lowercase().as_str() {
            "left" | "l" => (CGEventType::LeftMouseDown, CGMouseButton::Left),
            "right" | "r" => (CGEventType::RightMouseDown, CGMouseButton::Right),
            "middle" | "m" => (CGEventType::OtherMouseDown, CGMouseButton::Center),
            _ => return Err(anyhow!("Unsupported mouse button: {}", button)),
        };

        let event = CGEvent::new_mouse_event(source, event_type, pos, btn)
            .map_err(|_| anyhow!("Failed to create mouse down event"))?;
        event.post(CGEventTapLocation::HID);
        Ok(())
    }

    fn mouse_up(&self, button: &str) -> Result<()> {
        let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
            .map_err(|_| anyhow!("Failed to create CGEventSource"))?;
        
        let current_event = CGEvent::new(source.clone())
            .map_err(|_| anyhow!("Failed to get current mouse position"))?;
        let pos = current_event.location();

        let (event_type, btn) = match button.to_lowercase().as_str() {
            "left" | "l" => (CGEventType::LeftMouseUp, CGMouseButton::Left),
            "right" | "r" => (CGEventType::RightMouseUp, CGMouseButton::Right),
            "middle" | "m" => (CGEventType::OtherMouseUp, CGMouseButton::Center),
            _ => return Err(anyhow!("Unsupported mouse button: {}", button)),
        };

        let event = CGEvent::new_mouse_event(source, event_type, pos, btn)
            .map_err(|_| anyhow!("Failed to create mouse up event"))?;
        event.post(CGEventTapLocation::HID);
        Ok(())
    }

    fn mouse_scroll(&self, clicks: i32, horizontal: bool) -> Result<()> {
        let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
            .map_err(|_| anyhow!("Failed to create CGEventSource"))?;

        // Units: 0 = line-based scrolling
        let raw_event = unsafe {
            if horizontal {
                CGEventCreateScrollWheelEvent(
                    source.as_ptr(),
                    0,
                    2,
                    0,
                    clicks,
                )
            } else {
                CGEventCreateScrollWheelEvent(
                    source.as_ptr(),
                    0,
                    1,
                    clicks,
                )
            }
        };

        if raw_event.is_null() {
            return Err(anyhow!("Failed to create scroll event"));
        }

        let event = unsafe { CGEvent::from_ptr(raw_event) };
        event.post(CGEventTapLocation::HID);
        Ok(())
    }
}

impl MacosBackend {
    fn press_combo(&self, source: &CGEventSource, combo: &str) -> Result<()> {
        let parts: Vec<&str> = combo.split('+').collect();
        let mut flags = CGEventFlags::empty();
        let mut main_keycode = None;

        for part in parts {
            let part = part.trim().to_lowercase();
            match part.as_str() {
                "ctrl" | "control" => flags |= CGEventFlags::CGEventFlagControl,
                "shift" => flags |= CGEventFlags::CGEventFlagShift,
                "alt" | "option" => flags |= CGEventFlags::CGEventFlagAlternate,
                "super" | "win" | "meta" | "cmd" | "command" => flags |= CGEventFlags::CGEventFlagCommand,
                _ => {
                    main_keycode = Some(convert_key(&part)?);
                }
            }
        }

        let keycode = main_keycode.ok_or_else(|| anyhow!("No main key specified in shortcut combo: {}", combo))?;

        let event_down = CGEvent::new_keyboard_event(source.clone(), keycode, true)
            .map_err(|_| anyhow!("Failed to create combo keyboard down event"))?;
        event_down.set_flags(flags);

        let event_up = CGEvent::new_keyboard_event(source.clone(), keycode, false)
            .map_err(|_| anyhow!("Failed to create combo keyboard up event"))?;
        event_up.set_flags(flags);

        event_down.post(CGEventTapLocation::HID);
        std::thread::sleep(std::time::Duration::from_millis(10));
        event_up.post(CGEventTapLocation::HID);

        Ok(())
    }
}

// Convert common key labels to macOS virtual keycodes
fn convert_key(key: &str) -> Result<u16> {
    match key.to_lowercase().as_str() {
        "a" => Ok(0),
        "b" => Ok(11),
        "c" => Ok(8),
        "d" => Ok(2),
        "e" => Ok(14),
        "f" => Ok(3),
        "g" => Ok(5),
        "h" => Ok(4),
        "i" => Ok(34),
        "j" => Ok(38),
        "k" => Ok(40),
        "l" => Ok(37),
        "m" => Ok(46),
        "n" => Ok(45),
        "o" => Ok(31),
        "p" => Ok(35),
        "q" => Ok(12),
        "r" => Ok(15),
        "s" => Ok(1),
        "t" => Ok(17),
        "u" => Ok(32),
        "v" => Ok(9),
        "w" => Ok(13),
        "x" => Ok(7),
        "y" => Ok(16),
        "z" => Ok(6),
        "0" => Ok(29),
        "1" => Ok(18),
        "2" => Ok(19),
        "3" => Ok(20),
        "4" => Ok(21),
        "5" => Ok(23),
        "6" => Ok(22),
        "7" => Ok(26),
        "8" => Ok(28),
        "9" => Ok(25),
        "enter" | "return" => Ok(36),
        "esc" | "escape" => Ok(53),
        "tab" => Ok(48),
        "space" => Ok(49),
        "backspace" => Ok(51),
        "delete" | "del" => Ok(117),
        "up" => Ok(126),
        "down" => Ok(125),
        "left" => Ok(123),
        "right" => Ok(124),
        "pageup" | "pgup" => Ok(116),
        "pagedown" | "pgdn" => Ok(121),
        "home" => Ok(115),
        "end" => Ok(119),
        "f1" => Ok(122),
        "f2" => Ok(120),
        "f3" => Ok(99),
        "f4" => Ok(118),
        "f5" => Ok(96),
        "f6" => Ok(97),
        "f7" => Ok(98),
        "f8" => Ok(100),
        "f9" => Ok(101),
        "f10" => Ok(109),
        "f11" => Ok(103),
        "f12" => Ok(111),
        "f13" => Ok(105),
        "f14" => Ok(107),
        "f15" => Ok(113),
        "f16" => Ok(106),
        _ => {
            if key.len() == 1 {
                let ch = key.chars().next().unwrap();
                if ch.is_ascii_alphabetic() {
                    let code = ch.to_ascii_lowercase() as u16;
                    if (97..=122).contains(&code) {
                        return convert_key(&ch.to_string());
                    }
                }
            }
            Err(anyhow!("Unsupported macOS key: {}", key))
        }
    }
}
