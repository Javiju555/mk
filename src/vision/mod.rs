use anyhow::{Context, Result};
use xcap::{Monitor, Window};
use image::{DynamicImage, ImageEncoder, Rgb, RgbImage};
use std::fs::File;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScreenshotFormat {
    Raw,       // PNG, full resolution
    Compressed, // JPEG, scaled down
}

impl ScreenshotFormat {
    pub fn from_args(raw: bool, compressed: bool) -> Self {
        if raw {
            ScreenshotFormat::Raw
        } else if compressed {
            ScreenshotFormat::Compressed
        } else {
            // Default: compressed for smaller files
            ScreenshotFormat::Compressed
        }
    }
}

/// Draw a crosshair at (cx, cy) on the image
fn draw_crosshair(img: &mut RgbImage, cx: i32, cy: i32) {
    let w = img.width() as i32;
    let h = img.height() as i32;
    let color = Rgb([255, 0, 0]); // Red
    let size = 20;
    let thickness = 2;

    // Horizontal line
    for dx in -size..=size {
        let x = cx + dx;
        if x >= 0 && x < w {
            for t in 0..thickness {
                let y = cy + t;
                if y >= 0 && y < h {
                    img.put_pixel(x as u32, y as u32, color);
                }
            }
        }
    }

    // Vertical line
    for dy in -size..=size {
        let y = cy + dy;
        if y >= 0 && y < h {
            for t in 0..thickness {
                let x = cx + t;
                if x >= 0 && x < w {
                    img.put_pixel(x as u32, y as u32, color);
                }
            }
        }
    }
}

/// Parse "x,y,w,h" into a rect tuple. All values in pixels, w/h > 0.
pub fn parse_crop_rect(s: &str) -> anyhow::Result<(u32, u32, u32, u32)> {
    let parts: Vec<&str> = s.split(',').map(|p| p.trim()).collect();
    if parts.len() != 4 {
        anyhow::bail!("--crop debe ser x,y,w,h (ej. 100,200,800,600), recibido: '{s}'");
    }
    let nums: Result<Vec<u32>, _> = parts.iter().map(|p| p.parse::<u32>()).collect();
    let nums = nums.map_err(|_| anyhow::anyhow!("--crop tiene valores no numéricos: '{s}'"))?;
    if nums[2] == 0 || nums[3] == 0 {
        anyhow::bail!("--crop w/h deben ser > 0: '{s}'");
    }
    Ok((nums[0], nums[1], nums[2], nums[3]))
}

/// Crop to `rect` then scale by `zoom` (Nearest neighbour: nítido para UI).
/// Panics if rect is outside the image — los callers validan antes con mensaje accionable.
pub fn crop_and_zoom(img: &DynamicImage, rect: (u32, u32, u32, u32), zoom: u32) -> DynamicImage {
    use image::imageops::FilterType;
    let (x, y, w, h) = rect;
    assert!(x + w <= img.width() && y + h <= img.height(), "crop {rect:?} fuera de imagen {}x{}", img.width(), img.height());
    let zoom = zoom.clamp(1, 8);
    let cropped = img.crop_imm(x, y, w, h);
    if zoom == 1 {
        return cropped;
    }
    cropped.resize(w * zoom, h * zoom, FilterType::Nearest)
}

fn apply_crop_zoom(img: DynamicImage, crop: &Option<String>, zoom: u32) -> anyhow::Result<DynamicImage> {
    let Some(c) = crop else { return Ok(if zoom <= 1 { img } else { crop_and_zoom(&img, (0, 0, img.width(), img.height()), zoom) }); };
    let rect = parse_crop_rect(c)?;
    if rect.0 + rect.2 > img.width() || rect.1 + rect.3 > img.height() {
        anyhow::bail!("crop {rect:?} fuera de imagen {}x{}", img.width(), img.height());
    }
    Ok(crop_and_zoom(&img, rect, zoom))
}

/// Crop/zoom an already-saved image file (post-process without re-capturing).
/// Output format follows the extension (`.png` → PNG, anything else → JPEG
/// with `quality`). Rect outside the image is an error, not a panic.
/// Returns the output dimensions so the CLI can report them.
pub fn crop_image_file(input: &str, output: &str, rect: (u32, u32, u32, u32), zoom: u32, quality: u8) -> Result<(u32, u32)> {
    let img = image::open(input).map_err(|e| anyhow::anyhow!("Failed to open {input}: {e}"))?;
    if rect.0 + rect.2 > img.width() || rect.1 + rect.3 > img.height() {
        anyhow::bail!("crop {rect:?} fuera de imagen {}x{}", img.width(), img.height());
    }
    let out = crop_and_zoom(&img, rect, zoom);
    let format = if output.to_lowercase().ends_with(".png") {
        ScreenshotFormat::Raw
    } else {
        ScreenshotFormat::Compressed
    };
    let (w, h) = (out.width(), out.height());
    save_image(&out, output, format, quality)?;
    Ok((w, h))
}

fn save_image(img: &DynamicImage, dest_path: &str, format: ScreenshotFormat, quality: u8) -> Result<()> {
    let path = Path::new(dest_path);
    
    match format {
        ScreenshotFormat::Raw => {
            // PNG, full quality
            img.save(path).context("Failed to save PNG")?;
        }
        ScreenshotFormat::Compressed => {
            // JPEG with user-specified quality
            let jpeg_path = if path.extension().map(|e| e.to_string_lossy().to_lowercase()) == Some("png".into()) {
                path.with_extension("jpg")
            } else {
                path.to_path_buf()
            };
            
            // Convert to RGB if needed
            let rgb_img = img.as_rgb8().cloned().unwrap_or_else(|| img.to_rgb8());
            
            // Write JPEG with quality
            let file = File::create(&jpeg_path).context("Failed to create JPEG file")?;
            let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(file, quality);
            encoder.write_image(
                rgb_img.as_raw(),
                rgb_img.width(),
                rgb_img.height(),
                image::ExtendedColorType::Rgb8,
            ).context("Failed to write JPEG with quality")?;
        }
    }
    Ok(())
}

pub fn capture_screen(dest_path: &str, format: ScreenshotFormat, quality: u8) -> Result<()> {
    capture_screen_with_options(dest_path, format, quality, &None, 1)
}

pub fn capture_screen_with_options(dest_path: &str, format: ScreenshotFormat, quality: u8, crop: &Option<String>, zoom: u32) -> Result<()> {
    let monitors = Monitor::all().map_err(|e| anyhow::anyhow!("Failed to list monitors: {e}"))?;
    let monitor = monitors.first().context("No monitors found")?;
    let image = monitor.capture_image().map_err(|e| anyhow::anyhow!("Failed to capture screen: {e}"))?;
    let dyn_img = DynamicImage::ImageRgba8(image);
    let final_img = apply_crop_zoom(dyn_img, crop, zoom)?;
    save_image(&final_img, dest_path, format, quality)?;
    Ok(())
}

/// Parse `xdotool getmouselocation --shell` output (`X=<n>` / `Y=<n>`
/// lines, plus SCREEN=/WINDOW= lines we ignore). Returns None when either
/// coordinate is missing or unparsable — the caller falls back to (0, 0).
pub fn parse_getmouselocation(output: &str) -> Option<(i32, i32)> {
    let mut x = None;
    let mut y = None;
    for line in output.lines() {
        if let Some(v) = line.strip_prefix("X=") {
            x = v.trim().parse().ok();
        } else if let Some(v) = line.strip_prefix("Y=") {
            y = v.trim().parse().ok();
        }
    }
    match (x, y) {
        (Some(x), Some(y)) => Some((x, y)),
        _ => None,
    }
}

/// Capture screen and draw a crosshair at the current cursor position
pub fn capture_screen_with_cursor(dest_path: &str, format: ScreenshotFormat, quality: u8) -> Result<()> {
    capture_screen_with_cursor_options(dest_path, format, quality, &None, 1)
}

pub fn capture_screen_with_cursor_options(dest_path: &str, format: ScreenshotFormat, quality: u8, crop: &Option<String>, zoom: u32) -> Result<()> {
    // Get cursor position first
    #[cfg(windows)]
    let cursor_pos = unsafe {
        let mut pos = std::mem::zeroed();
        if windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pos) != 0 {
            (pos.x, pos.y)
        } else {
            (0, 0)
        }
    };

    #[cfg(target_os = "macos")]
    let cursor_pos = crate::input::macos::cursor_position().unwrap_or((0, 0));

    #[cfg(target_os = "linux")]
    let cursor_pos = {
        use std::process::Command;
        Command::new("xdotool")
            .args(["getmouselocation", "--shell"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| parse_getmouselocation(&String::from_utf8_lossy(&o.stdout)))
            .unwrap_or((0, 0))
    };

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let cursor_pos = (0, 0);

    // Capture the screen
    let monitors = Monitor::all().map_err(|e| anyhow::anyhow!("Failed to list monitors: {e}"))?;
    let monitor = monitors.first().context("No monitors found")?;
    let monitor_x = monitor.x().unwrap_or(0);
    let monitor_y = monitor.y().unwrap_or(0);
    let image = monitor.capture_image().map_err(|e| anyhow::anyhow!("Failed to capture screen: {e}"))?;
    let rgba_buf = image;
    let mut rgb_img = RgbImage::new(rgba_buf.width(), rgba_buf.height());
    
    // Convert RGBA to RGB and draw crosshair
    for y in 0..rgba_buf.height() {
        for x in 0..rgba_buf.width() {
            let pixel = rgba_buf.get_pixel(x, y);
            rgb_img.put_pixel(x, y, Rgb([pixel[0], pixel[1], pixel[2]]));
        }
    }

    // Draw crosshair at cursor position (relative to monitor)
    let rel_x = cursor_pos.0 - monitor_x;
    let rel_y = cursor_pos.1 - monitor_y;
    draw_crosshair(&mut rgb_img, rel_x, rel_y);

    // Save
    let dyn_img = DynamicImage::ImageRgb8(rgb_img);
    let final_img = apply_crop_zoom(dyn_img, crop, zoom)?;
    save_image(&final_img, dest_path, format, quality)?;
    Ok(())
}

pub fn capture_monitor(index: usize, dest_path: &str, format: ScreenshotFormat, quality: u8) -> Result<()> {
    capture_monitor_with_options(index, dest_path, format, quality, &None, 1)
}

pub fn capture_monitor_with_options(index: usize, dest_path: &str, format: ScreenshotFormat, quality: u8, crop: &Option<String>, zoom: u32) -> Result<()> {
    let monitors = Monitor::all().map_err(|e| anyhow::anyhow!("Failed to list monitors: {e}"))?;
    let monitor = monitors.get(index).context(format!("Monitor {} not found ({} available)", index, monitors.len()))?;
    let image = monitor.capture_image().map_err(|e| anyhow::anyhow!("Failed to capture monitor: {e}"))?;
    let dyn_img = DynamicImage::ImageRgba8(image);
    let final_img = apply_crop_zoom(dyn_img, crop, zoom)?;
    save_image(&final_img, dest_path, format, quality)?;
    Ok(())
}

pub fn list_monitors() -> Result<Vec<(String, i32, i32, u32, u32)>> {
    let monitors = Monitor::all().map_err(|e| anyhow::anyhow!("Failed to list monitors: {e}"))?;
    let mut result = Vec::new();
    for (i, m) in monitors.iter().enumerate() {
        let name = m.name().unwrap_or_else(|_| format!("Monitor {}", i)).to_string();
        let x = m.x().unwrap_or(0);
        let y = m.y().unwrap_or(0);
        let w = m.width().unwrap_or(0);
        let h = m.height().unwrap_or(0);
        result.push((name, x, y, w, h));
    }
    Ok(result)
}

pub fn capture_window(window_id: &str, dest_path: &str, format: ScreenshotFormat, quality: u8) -> Result<()> {
    capture_window_with_options(window_id, dest_path, format, quality, &None, 1)
}

pub fn capture_window_with_options(window_id: &str, dest_path: &str, format: ScreenshotFormat, quality: u8, crop: &Option<String>, zoom: u32) -> Result<()> {
    let windows = Window::all().map_err(|e| anyhow::anyhow!("Failed to list windows: {e}"))?;

    let target_id: u32 = window_id.parse().context("Invalid window ID")?;

    let window = windows.iter()
        .find(|w| w.id().unwrap_or(0) == target_id)
        .context("Window not found")?;

    if window.is_minimized().unwrap_or(false) {
        anyhow::bail!("Cannot capture minimized window");
    }

    let image = window.capture_image().map_err(|e| anyhow::anyhow!("Failed to capture window: {e}"))?;
    let dyn_img = DynamicImage::ImageRgba8(image);
    let final_img = apply_crop_zoom(dyn_img, crop, zoom)?;
    save_image(&final_img, dest_path, format, quality)?;
    Ok(())
}

pub fn capture_region(x: u32, y: u32, w: u32, h: u32, dest_path: &str, format: ScreenshotFormat, quality: u8) -> Result<()> {
    let monitors = Monitor::all().map_err(|e| anyhow::anyhow!("Failed to list monitors: {e}"))?;
    let monitor = monitors.first().context("No monitors found")?;
    let img_buffer = monitor.capture_image().map_err(|e| anyhow::anyhow!("Failed to capture screen: {e}"))?;
    
    let dynamic_img = DynamicImage::ImageRgba8(img_buffer);
    let cropped = dynamic_img.crop_imm(x, y, w, h);
    save_image(&cropped, dest_path, format, quality)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_crop_and_zoom_center_pixel() {
        use image::{DynamicImage, Rgb, RgbImage};
        let mut img = RgbImage::new(100, 100);
        for y in 0..100 {
            for x in 0..100 {
                img.put_pixel(x, y, Rgb([x as u8, y as u8, 0]));
            }
        }
        let dyn_img = DynamicImage::ImageRgb8(img);
        let out = crop_and_zoom(&dyn_img, (10, 10, 20, 20), 2);
        assert_eq!(out.width(), 40);
        assert_eq!(out.height(), 40);

        let bad = std::panic::catch_unwind(|| crop_and_zoom(&dyn_img, (90, 90, 50, 50), 1));
        assert!(bad.is_err(), "crop fuera de imagen debe fallar");
    }

    #[test]
    fn test_parse_crop_rect() {
        assert_eq!(parse_crop_rect("10,20,300,200").unwrap(), (10, 20, 300, 200));
        assert!(parse_crop_rect("10,20").is_err());
        assert!(parse_crop_rect("a,b,c,d").is_err());
    }

    #[test]
    fn test_parse_getmouselocation() {
        let out = "X=1234\nY=567\nSCREEN=0\nWINDOW=12345678\n";
        assert_eq!(parse_getmouselocation(out), Some((1234, 567)));
        assert_eq!(parse_getmouselocation("X=10\n"), None);
        assert_eq!(parse_getmouselocation("garbage\n"), None);
        assert_eq!(parse_getmouselocation(""), None);
    }

    #[test]
    fn test_capture_screen_and_region() {
        let temp_dir = std::env::temp_dir();
        let screen_path = temp_dir.join("test_screen.png");
        let region_path = temp_dir.join("test_region.png");

        let screen_path_str = screen_path.to_str().unwrap();
        let region_path_str = region_path.to_str().unwrap();

        // Cleanup if any
        let _ = fs::remove_file(screen_path_str);
        let _ = fs::remove_file(region_path_str);

        // Capture screen
        if let Ok(()) = capture_screen(screen_path_str, ScreenshotFormat::Raw, 85) {
            assert!(screen_path.exists());

            // Capture a small region (100x100 starting at 10,10)
            if let Ok(()) = capture_region(10, 10, 100, 100, region_path_str, ScreenshotFormat::Raw, 85) {
                assert!(region_path.exists());
                let _ = fs::remove_file(region_path_str);
            }
            let _ = fs::remove_file(screen_path_str);
        }
    }
}
