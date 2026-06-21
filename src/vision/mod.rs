use anyhow::{Context, Result};
use xcap::Monitor;
use image::DynamicImage;

pub fn capture_screen(dest_path: &str) -> Result<()> {
    let monitors = Monitor::all().map_err(|e| anyhow::anyhow!("Failed to list monitors: {e}"))?;
    let monitor = monitors.first().context("No monitors found")?;
    let image = monitor.capture_image().map_err(|e| anyhow::anyhow!("Failed to capture screen: {e}"))?;
    image.save(dest_path).context("Failed to save screenshot image")?;
    Ok(())
}

pub fn capture_region(x: u32, y: u32, w: u32, h: u32, dest_path: &str) -> Result<()> {
    let monitors = Monitor::all().map_err(|e| anyhow::anyhow!("Failed to list monitors: {e}"))?;
    let monitor = monitors.first().context("No monitors found")?;
    let img_buffer = monitor.capture_image().map_err(|e| anyhow::anyhow!("Failed to capture screen: {e}"))?;
    
    let dynamic_img = DynamicImage::ImageRgba8(img_buffer);
    let cropped = dynamic_img.crop_imm(x, y, w, h);
    cropped.save(dest_path).context("Failed to save cropped screenshot region")?;
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
        if let Ok(()) = capture_screen(screen_path_str) {
            assert!(screen_path.exists());
            
            // Capture a small region (100x100 starting at 10,10)
            if let Ok(()) = capture_region(10, 10, 100, 100, region_path_str) {
                assert!(region_path.exists());
                let _ = fs::remove_file(region_path_str);
            }
            let _ = fs::remove_file(screen_path_str);
        }
    }
}
