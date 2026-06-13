use super::Backend;
use anyhow::{bail, Context, Result};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;

const SOCKET_PATH: &str = "/tmp/mk-daemon.sock";

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
