use anyhow::{bail, Context, Result};
use std::thread;
use std::time::Duration;
use chrono::Timelike;

use crate::backend::Backend;

pub fn delay_until_time(time_str: &str) -> Result<Duration> {
    let time_str = time_str.trim();
    let parts: Vec<&str> = time_str.split(':').collect();
    if parts.len() != 2 {
        bail!("Time format must be HH:MM, got: {time_str}");
    }
    let target_h: u32 = parts[0]
        .parse()
        .context("Invalid hour in time")?;
    let target_m: u32 = parts[1]
        .parse()
        .context("Invalid minute in time")?;
    if target_h > 23 || target_m > 59 {
        bail!("Invalid time: {time_str} (hour 0-23, minute 0-59)");
    }

    let now = chrono::Local::now();
    let target = now
        .with_hour(target_h)
        .and_then(|dt| dt.with_minute(target_m))
        .and_then(|dt| dt.with_second(0))
        .and_then(|dt| dt.with_nanosecond(0))
        .context("Failed to construct target datetime")?;

    let target = if target <= now {
        target + chrono::Duration::days(1)
    } else {
        target
    };

    let delay = target.signed_duration_since(now);
    let mut delay_secs = delay.num_seconds();

    if delay_secs <= 0 {
        delay_secs = 86400;
    }

    Ok(Duration::from_secs(delay_secs as u64))
}

pub fn keep_awake_background(interval: Duration, key: String) {
    println!("Keep-awake background process active: pressing {key} every {interval:?}.");
    std::thread::spawn(move || {
        if let Ok(backend) = crate::backend::detect_backend() {
            loop {
                let _ = backend.press_key(&key);
                std::thread::sleep(interval);
            }
        }
    });
}


pub fn keep_awake_loop(
    interval: Duration,
    key: &str,
    backend: &dyn Backend,
    dry_run: bool,
) -> Result<()> {
    println!("Keep-awake active: pressing {key} every {interval:?}. Press Ctrl+C to stop.");

    if dry_run {
        println!("[dry-run] would press \"{key}\" every {interval:?}");
        return Ok(());
    }

    loop {
        match backend.press_key(key) {
            Ok(()) => {
                // silent
            }
            Err(e) => {
                eprintln!("keep-awake: failed to press {key}: {e}");
            }
        }
        thread::sleep(interval);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delay_until_time_future() {
        let d = delay_until_time("23:59").unwrap();
        assert!(d > Duration::from_secs(0));
        assert!(d <= Duration::from_secs(86400));
    }

    #[test]
    fn test_delay_until_time_invalid_format() {
        assert!(delay_until_time("invalid").is_err());
        assert!(delay_until_time("25:00").is_err());
        assert!(delay_until_time("12:60").is_err());
    }

    #[test]
    fn test_delay_until_time_midnight() {
        let d = delay_until_time("00:00").unwrap();
        assert!(d > Duration::from_secs(0));
        assert!(d <= Duration::from_secs(86400));
    }
}
