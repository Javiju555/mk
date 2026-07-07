//! Coordinate-space conversion shared by the platform mouse backends.
//!
//! mk's contract: `mk click X Y` / `mk move X Y` take coordinates in the SAME
//! space as `mk screenshot` output — **physical pixels**. Each platform's
//! native pointer API, however, may expect a different space:
//!
//! - Linux (uinput virtual absolute pointer): 0..32767 across the *physical*
//!   extent — handled in `daemon.rs::scale_coords` (uses physical resolution).
//! - macOS (CGEvent / CGPoint): **logical points**. On a Retina display the
//!   screenshot is 2× the points, so a physical coordinate must be divided by
//!   the backing scale factor before it becomes a CGPoint.
//! - Windows (SetCursorPos): **physical pixels IF the process is per-monitor
//!   DPI-aware**, else DPI-virtualized (logical). The clean fix is to make the
//!   process DPI-aware so SetCursorPos matches the physical screenshot; if that
//!   isn't done, a physical coordinate must be divided by the monitor scale.
//!
//! This module is intentionally NOT `cfg`-gated so its unit tests run on every
//! platform (including the Linux CI/dev box where the macOS/Windows backends
//! themselves can't be compiled-in).

/// Convert a physical-pixel coordinate to a logical-coordinate one by dividing
/// by the display scale factor. `scale` is e.g. 2.0 on macOS Retina, 1.5 on a
/// Windows 150% display, 1.0 when unscaled. Non-finite or non-positive scales
/// are treated as 1.0 (identity) so a bad reading can never move the pointer
/// somewhere wild.
pub fn physical_to_logical(x: i32, y: i32, scale: f64) -> (f64, f64) {
    let s = sane_scale(scale);
    (x as f64 / s, y as f64 / s)
}

/// Inverse of `physical_to_logical`: logical → physical (multiply by scale).
/// Useful when a backend reports geometry in logical units but mk works in
/// physical pixels.
pub fn logical_to_physical(x: f64, y: f64, scale: f64) -> (f64, f64) {
    let s = sane_scale(scale);
    (x * s, y * s)
}

/// Clamp a scale factor to a usable value: finite and > 0, else 1.0.
fn sane_scale(scale: f64) -> f64 {
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: (f64, f64), b: (f64, f64)) {
        assert!((a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6, "{a:?} != {b:?}");
    }

    #[test]
    fn retina_2x_halves_physical_to_points() {
        // A physical (3456,2160) Retina pixel is logical point (1728,1080).
        approx(physical_to_logical(3456, 2160, 2.0), (1728.0, 1080.0));
    }

    #[test]
    fn windows_150_percent() {
        // 150% scale: physical 2880 → logical 1920.
        approx(physical_to_logical(2880, 1920, 1.5), (1920.0, 1280.0));
    }

    #[test]
    fn fractional_1_667() {
        approx(physical_to_logical(2880, 1920, 5.0 / 3.0), (1728.0, 1152.0));
    }

    #[test]
    fn unscaled_is_identity() {
        approx(physical_to_logical(1234, 567, 1.0), (1234.0, 567.0));
    }

    #[test]
    fn bad_scale_falls_back_to_identity() {
        approx(physical_to_logical(100, 200, 0.0), (100.0, 200.0));
        approx(physical_to_logical(100, 200, -2.0), (100.0, 200.0));
        approx(physical_to_logical(100, 200, f64::NAN), (100.0, 200.0));
        approx(physical_to_logical(100, 200, f64::INFINITY), (100.0, 200.0));
    }

    #[test]
    fn round_trips() {
        let (lx, ly) = physical_to_logical(3456, 2160, 2.0);
        approx(logical_to_physical(lx, ly, 2.0), (3456.0, 2160.0));
    }
}
