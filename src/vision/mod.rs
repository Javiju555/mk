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
    let monitors = Monitor::all().map_err(|e| anyhow::anyhow!("Failed to list monitors: {e}"))?;
    let monitor = monitors.first().context("No monitors found")?;
    let image = monitor.capture_image().map_err(|e| anyhow::anyhow!("Failed to capture screen: {e}"))?;
    let dyn_img = DynamicImage::ImageRgba8(image);
    save_image(&dyn_img, dest_path, format, quality)?;
    Ok(())
}

/// Capture screen and draw a crosshair at the current cursor position
pub fn capture_screen_with_cursor(dest_path: &str, format: ScreenshotFormat, quality: u8) -> Result<()> {
    // Get cursor position first
    let cursor_pos = unsafe {
        let mut pos = std::mem::zeroed();
        if windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut pos) != 0 {
            (pos.x, pos.y)
        } else {
            (0, 0)
        }
    };

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
    save_image(&dyn_img, dest_path, format, quality)?;
    Ok(())
}

pub fn capture_monitor(index: usize, dest_path: &str, format: ScreenshotFormat, quality: u8) -> Result<()> {
    let monitors = Monitor::all().map_err(|e| anyhow::anyhow!("Failed to list monitors: {e}"))?;
    let monitor = monitors.get(index).context(format!("Monitor {} not found ({} available)", index, monitors.len()))?;
    let image = monitor.capture_image().map_err(|e| anyhow::anyhow!("Failed to capture monitor: {e}"))?;
    let dyn_img = DynamicImage::ImageRgba8(image);
    save_image(&dyn_img, dest_path, format, quality)?;
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
    save_image(&dyn_img, dest_path, format, quality)?;
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
        if let Ok(()) = capture_screen(screen_path_str, ScreenshotFormat::Raw) {
            assert!(screen_path.exists());
            
            // Capture a small region (100x100 starting at 10,10)
            if let Ok(()) = capture_region(10, 10, 100, 100, region_path_str, ScreenshotFormat::Raw) {
                assert!(region_path.exists());
                let _ = fs::remove_file(region_path_str);
            }
            let _ = fs::remove_file(screen_path_str);
        }
    }
}
