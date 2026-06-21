use super::Backend;
use anyhow::{bail, Context, Result};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;

const SOCKET_PATH: &str = "/tmp/mk-daemon.sock";

fn get_screen_resolution() -> (i32, i32) {
    if let Ok(monitors) = xcap::Monitor::all() {
        if let Some(m) = monitors.first() {
            if let (Ok(w), Ok(h)) = (m.width(), m.height()) {
                return (w as i32, h as i32);
            }
        }
    }
    (1920, 1080)
}

fn scale_coords(x: i32, y: i32) -> (i32, i32) {
    let (w, h) = get_screen_resolution();
    let scaled_x = (x as i64 * 32767 / w as i64) as i32;
    let scaled_y = (y as i64 * 32767 / h as i64) as i32;
    (scaled_x.clamp(0, 32767), scaled_y.clamp(0, 32767))
}

pub struct DaemonBackend;

impl Backend for DaemonBackend {
    fn type_text(&self, text: &str) -> Result<()> {
        let mut stream = connect()?;
        send_command(&mut stream, &format!("TYPE:{text}"))?;
        Ok(())
    }

    fn press_key(&self, key: &str) -> Result<()> {
        let mut stream = connect()?;
        send_command(&mut stream, &format!("KEY:{key}"))?;
        Ok(())
    }

    fn mouse_move(&self, x: i32, y: i32, duration_ms: u64) -> Result<()> {
        let (sx, sy) = scale_coords(x, y);
        let mut stream = connect()?;
        if duration_ms > 0 {
            send_command(&mut stream, &format!("MOVE_SMOOTH:{sx}:{sy}:{duration_ms}"))?;
        } else {
            send_command(&mut stream, &format!("MOVE:{sx}:{sy}"))?;
        }
        Ok(())
    }

    fn mouse_click(&self, x: i32, y: i32, button: &str, duration_ms: u64) -> Result<()> {
        self.mouse_move(x, y, duration_ms)?;
        let mut stream = connect()?;
        send_command(&mut stream, &format!("CLICK:{button}"))?;
        Ok(())
    }

    fn mouse_drag(&self, x1: i32, y1: i32, x2: i32, y2: i32, duration_ms: u64) -> Result<()> {
        self.mouse_move(x1, y1, 0)?;
        {
            let mut stream = connect()?;
            send_command(&mut stream, "MOUSE_DOWN:left")?;
        }
        self.mouse_move(x2, y2, duration_ms)?;
        {
            let mut stream = connect()?;
            send_command(&mut stream, "MOUSE_UP:left")?;
        }
        Ok(())
    }

    fn mouse_down(&self, button: &str) -> Result<()> {
        let mut stream = connect()?;
        send_command(&mut stream, &format!("MOUSE_DOWN:{button}"))?;
        Ok(())
    }

    fn mouse_up(&self, button: &str) -> Result<()> {
        let mut stream = connect()?;
        send_command(&mut stream, &format!("MOUSE_UP:{button}"))?;
        Ok(())
    }

    fn mouse_scroll(&self, clicks: i32, horizontal: bool) -> Result<()> {
        let mut stream = connect()?;
        send_command(&mut stream, &format!("SCROLL:{clicks}:{horizontal}"))?;
        Ok(())
    }

    fn display_name(&self) -> &str {
        "mk-daemon"
    }
}

fn connect() -> Result<UnixStream> {
    if !Path::new(SOCKET_PATH).exists() {
        bail!(
            "mk-daemon is not running.\n\
             Start it with: sudo mk-daemon\n\
             Or run: sudo mk daemon start"
        );
    }
    UnixStream::connect(SOCKET_PATH).context("Failed to connect to mk-daemon socket")
}

fn send_command(stream: &mut UnixStream, cmd: &str) -> Result<String> {
    stream
        .write_all(format!("{cmd}\n").as_bytes())
        .context("Failed to send command to daemon")?;

    // The daemon keeps the connection open for further commands, so read a
    // single response line instead of reading to EOF (which would deadlock:
    // the client waiting for a close the daemon never sends).
    let mut response = String::new();
    BufReader::new(stream)
        .read_line(&mut response)
        .context("Failed to read daemon response")?;

    let response = response.trim();
    if response.starts_with("ERR:") {
        bail!("Daemon error: {}", &response[4..]);
    }

    Ok(response.to_string())
}

pub fn daemon_is_running() -> bool {
    UnixStream::connect(SOCKET_PATH).is_ok()
}

pub fn ping_daemon() -> Result<()> {
    let mut stream = connect()?;
    let response = send_command(&mut stream, "PING")?;
    if response == "OK" {
        Ok(())
    } else {
        bail!("Unexpected daemon response: {response}")
    }
}
