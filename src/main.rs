use mk::input;
use mk::doctor;
use mk::parser;
use mk::scheduler;


use anyhow::{bail, Result};
#[cfg(target_os = "linux")]
use anyhow::Context;
use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use mk::input::{Backend, DryRunBackend};
use mk::parser::{Interpreter, Logger};

#[derive(Parser)]
#[command(name = "mk", about = "Automate keyboard input on Linux", version)]
struct Cli {
    /// Print actions without executing them
    #[arg(long, global = true)]
    dry_run: bool,

    /// Log actions to a file with timestamps
    #[arg(short, long, global = true)]
    log: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Type a text message
    Text {
        /// The message to type
        message: String,
    },
    /// Press Enter
    Enter,
    /// Press a key combination
    Key {
        /// Key to press, e.g. "ctrl+s", "enter", "alt+tab"
        key: String,
    },
    /// Wait for a duration
    Wait {
        /// Duration: "10s", "5m", "2h", "250ms"
        duration: String,
    },
    /// Copy text to clipboard and paste
    Paste {
        /// Text to paste
        text: String,
        /// Shortcut key combination to trigger paste (default: ctrl+v)
        #[arg(short, long, default_value = "ctrl+v")]
        shortcut: String,
    },
    /// Execute an action after a delay
    In {
        /// Duration to wait: "10s", "5m", "250ms"
        duration: String,
        #[command(subcommand)]
        action: ScheduledAction,
    },
    /// Execute an action at a specific time (HH:MM local)
    At {
        /// Time in HH:MM format
        time: String,
        #[command(subcommand)]
        action: ScheduledAction,
    },
    /// Keep the session awake by pressing a key periodically
    KeepAwake {
        /// Interval: "4m", "30s" (default: 4m)
        #[arg(default_value = "4m")]
        interval: String,
        /// Key to press (default: F15)
        #[arg(short, long, default_value = "F15")]
        key: String,
    },
    /// Run a script file
    Run {
        /// Path to the .mk script file
        file: String,
    },
    /// Copy file content to clipboard and paste with formatting
    PasteFile {
        /// Path to the file
        path: String,
    },
    /// Copy directory contents recursively to clipboard and paste with formatting
    PasteDir {
        /// Path to the directory
        path: String,
    },
    /// Move mouse cursor to coordinates
    Move {
        /// Destination X coordinate (pixels)
        x: i32,
        /// Destination Y coordinate (pixels)
        y: i32,
        /// Duration of progressive slide (e.g. "500ms")
        #[arg(short, long)]
        duration: Option<String>,
    },
    /// Click a mouse button at coordinates
    Click {
        /// X coordinate (pixels)
        x: i32,
        /// Y coordinate (pixels)
        y: i32,
        /// Button to click: left, right, middle
        #[arg(short, long, default_value = "left")]
        button: String,
        /// Duration of progressive slide before clicking (e.g. "500ms")
        #[arg(short, long)]
        duration: Option<String>,
    },
    /// Drag the mouse from start to end coordinates
    Drag {
        /// Start X coordinate
        x1: i32,
        /// Start Y coordinate
        y1: i32,
        /// End X coordinate
        x2: i32,
        /// End Y coordinate
        y2: i32,
        /// Duration of slide (e.g. "500ms", default "500ms")
        #[arg(short, long)]
        duration: Option<String>,
    },
    /// Press and hold a mouse button
    MouseDown {
        /// Button to press: left, right, middle (default: left)
        #[arg(default_value = "left")]
        button: String,
    },
    /// Release a mouse button
    MouseUp {
        /// Button to release: left, right, middle (default: left)
        #[arg(default_value = "left")]
        button: String,
    },
    /// Scroll the mouse wheel
    Scroll {
        /// Number of scroll clicks (negative for down/left, positive for up/right)
        clicks: i32,
        /// Scroll horizontally instead of vertically
        #[arg(short, long)]
        horizontal: bool,
    },
    /// Take a screenshot
    Screenshot {
        /// Path to save the PNG image
        path: String,
    },
    /// Manage the mk-daemon service
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
    /// Check system dependencies and display diagnostics
    Doctor,
    /// List, inspect, or focus on-screen windows
    Window {
        #[command(subcommand)]
        action: WindowAction,
    },
}

#[derive(Subcommand)]
enum WindowAction {
    /// List all on-screen windows (JSON): id, title, app, geometry, is_active
    List,
    /// Print the currently focused window as JSON
    Active,
    /// Raise a window to the foreground by its id (best-effort, per-OS)
    Focus {
        /// Window id (as reported by `mk window list`)
        id: String,
    },
}

#[derive(Subcommand)]
enum DaemonAction {
    /// Start the daemon (requires root)
    Start,
    /// Stop the running daemon
    Stop,
    /// Check if daemon is running
    Status,
}

#[derive(Subcommand, Clone)]
enum ScheduledAction {
    /// Type a text message
    Text {
        /// The message to type
        message: String,
    },
    /// Press Enter
    Enter,
    /// Press a key combination
    Key {
        /// Key to press
        key: String,
    },
    /// Copy text to clipboard and paste
    Paste {
        /// Text to paste
        text: String,
        /// Shortcut key combination to trigger paste (default: ctrl+v)
        #[arg(short, long, default_value = "ctrl+v")]
        shortcut: String,
    },
    /// Wait for a duration
    Wait {
        /// Duration: "10s", "5m", "250ms"
        duration: String,
    },
    /// Move mouse cursor to coordinates
    Move {
        /// Destination X coordinate
        x: i32,
        /// Destination Y coordinate
        y: i32,
        /// Duration of progressive slide
        #[arg(short, long)]
        duration: Option<String>,
    },
    /// Click a mouse button at coordinates
    Click {
        /// X coordinate
        x: i32,
        /// Y coordinate
        y: i32,
        /// Button to click
        #[arg(short, long, default_value = "left")]
        button: String,
        /// Duration of progressive slide
        #[arg(short, long)]
        duration: Option<String>,
    },
    /// Drag the mouse
    Drag {
        /// Start X coordinate
        x1: i32,
        /// Start Y coordinate
        y1: i32,
        /// End X coordinate
        x2: i32,
        /// End Y coordinate
        y2: i32,
        /// Duration of slide
        #[arg(short, long)]
        duration: Option<String>,
    },
    /// Press and hold a mouse button
    MouseDown {
        /// Button to press (default: left)
        #[arg(default_value = "left")]
        button: String,
    },
    /// Release a mouse button
    MouseUp {
        /// Button to release (default: left)
        #[arg(default_value = "left")]
        button: String,
    },
    /// Scroll the mouse wheel
    Scroll {
        /// Number of scroll clicks
        clicks: i32,
        /// Scroll horizontally instead of vertically
        #[arg(short, long)]
        horizontal: bool,
    },
    /// Take a screenshot
    Screenshot {
        /// Path to save the PNG image
        path: String,
    },
}

impl ScheduledAction {
    fn to_command(&self) -> parser::Command {
        match self {
            ScheduledAction::Text { message } => parser::Command::Text(message.clone()),
            ScheduledAction::Enter => parser::Command::Enter,
            ScheduledAction::Key { key } => parser::Command::Key(key.clone()),
            ScheduledAction::Paste { text, shortcut } => parser::Command::Paste(text.clone(), shortcut.clone()),
            ScheduledAction::Wait { duration } => parser::Command::Wait(
                parser::parse_duration(duration).unwrap_or(Duration::from_secs(0)),
            ),
            ScheduledAction::Move { x, y, duration } => parser::Command::MouseMove(
                x.to_string(),
                y.to_string(),
                duration.clone().unwrap_or_else(|| "0s".to_string())
            ),
            ScheduledAction::Click { x, y, button, duration } => parser::Command::MouseClick(
                x.to_string(),
                y.to_string(),
                button.clone(),
                duration.clone().unwrap_or_else(|| "0s".to_string())
            ),
            ScheduledAction::Drag { x1, y1, x2, y2, duration } => parser::Command::MouseDrag(
                x1.to_string(),
                y1.to_string(),
                x2.to_string(),
                y2.to_string(),
                duration.clone().unwrap_or_else(|| "500ms".to_string())
            ),
            ScheduledAction::MouseDown { button } => parser::Command::MouseDown(button.clone()),
            ScheduledAction::MouseUp { button } => parser::Command::MouseUp(button.clone()),
            ScheduledAction::Scroll { clicks, horizontal } => parser::Command::MouseScroll(
                clicks.to_string(),
                horizontal.to_string()
            ),
            ScheduledAction::Screenshot { path } => parser::Command::Screenshot(path.clone()),
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Doctor => {
            return doctor::run();
        }
        Commands::Window { action } => {
            return handle_window(action);
        }
        Commands::Daemon { action } => {
            #[cfg(target_os = "linux")]
            {
                return match action {
                    DaemonAction::Start => daemon_start(),
                    DaemonAction::Stop => daemon_stop(),
                    DaemonAction::Status => daemon_status(),
                };
            }
            #[cfg(not(target_os = "linux"))]
            {
                let _ = action;
                bail!("Daemon operations are only supported on Linux.");
            }
        }
        _ => {}
    }

    let real_backend = input::detect_backend()?;

    let backend: Box<dyn Backend> = if cli.dry_run {
        Box::new(DryRunBackend)
    } else {
        real_backend
    };

    // Scheduled commands (`in`/`at`) log by default — so there's always a
    // persistent record of whether and when a delayed action actually fired,
    // even if the user forgot `--log`. An explicit `--log` still wins.
    let default_log = matches!(cli.command, Commands::In { .. } | Commands::At { .. })
        .then(default_scheduled_log_path);
    let log_path = cli.log.clone().or(default_log);
    let mut logger = log_path.as_deref().map(Logger::new).transpose()?;

    // Record the "armed" moment for scheduled commands; the fire itself is
    // logged by the interpreter when the action runs after the wait.
    if let Some(l) = logger.as_mut() {
        match &cli.command {
            Commands::In { duration, .. } => {
                let _ = l.log("scheduled", &format!("in {duration}"), "armed");
            }
            Commands::At { time, .. } => {
                let _ = l.log("scheduled", &format!("at {time}"), "armed");
            }
            _ => {}
        }
    }

    let mut interp = Interpreter::new(backend.as_ref(), cli.dry_run, logger.as_mut());

    match cli.command {
        Commands::Text { message } => {
            interp.run(&[parser::Command::Text(message)])?;
        }
        Commands::Enter => {
            interp.run(&[parser::Command::Enter])?;
        }
        Commands::Key { key } => {
            interp.run(&[parser::Command::Key(key)])?;
        }
        Commands::Wait { duration } => {
            let dur = parser::parse_duration(&duration)?;
            interp.run(&[parser::Command::Wait(dur)])?;
        }
        Commands::Paste { text, shortcut } => {
            interp.run(&[parser::Command::Paste(text, shortcut)])?;
        }
        Commands::In { duration, action } => {
            let dur = parser::parse_duration(&duration)?;
            let cmd = action.to_command();
            if cli.dry_run {
                println!("[dry-run] waiting {dur:?} before executing 1 action(s)");
                interp.run(&[cmd])?;
            } else {
                println!("Waiting {dur:?}...");
                std::thread::sleep(dur);
                interp.run(&[cmd])?;
            }
        }
        Commands::At { time, action } => {
            let delay = scheduler::delay_until_time(&time)?;
            let cmd = action.to_command();
            if cli.dry_run {
                println!("[dry-run] at {time} (wait {delay:?}, 1 action(s))");
                interp.run(&[cmd])?;
            } else {
                println!("Waiting until {time} ({delay:?})...");
                std::thread::sleep(delay);
                interp.run(&[cmd])?;
            }
        }
        Commands::KeepAwake { interval, key } => {
            let dur = parser::parse_duration(&interval)?;
            scheduler::keep_awake_loop(dur, &key, backend.as_ref(), cli.dry_run)?;
        }
        Commands::Run { file } => {
            let content = std::fs::read_to_string(&file)
                .map_err(|e| anyhow::anyhow!("Failed to read script: {e}"))?;
            let base_dir = Path::new(&file)
                .parent()
                .unwrap_or(Path::new("."));

            let vars = HashMap::new();
            let commands = parser::parse_script_with_vars(&content, &vars, Some(base_dir))?;
            let commands = parser::resolve_includes(&commands, base_dir, &vars)?;

            interp.run(&commands)?;
        }
        Commands::PasteFile { path } => {
            interp.run(&[parser::Command::PasteFile(path)])?;
        }
        Commands::PasteDir { path } => {
            interp.run(&[parser::Command::PasteDir(path)])?;
        }
        Commands::Move { x, y, duration } => {
            let dur = duration.unwrap_or_else(|| "0s".to_string());
            interp.run(&[parser::Command::MouseMove(x.to_string(), y.to_string(), dur)])?;
        }
        Commands::Click { x, y, button, duration } => {
            let dur = duration.unwrap_or_else(|| "0s".to_string());
            interp.run(&[parser::Command::MouseClick(x.to_string(), y.to_string(), button, dur)])?;
        }
        Commands::Drag { x1, y1, x2, y2, duration } => {
            let dur = duration.unwrap_or_else(|| "500ms".to_string());
            interp.run(&[parser::Command::MouseDrag(x1.to_string(), y1.to_string(), x2.to_string(), y2.to_string(), dur)])?;
        }
        Commands::MouseDown { button } => {
            interp.run(&[parser::Command::MouseDown(button)])?;
        }
        Commands::MouseUp { button } => {
            interp.run(&[parser::Command::MouseUp(button)])?;
        }
        Commands::Scroll { clicks, horizontal } => {
            interp.run(&[parser::Command::MouseScroll(clicks.to_string(), horizontal.to_string())])?;
        }
        Commands::Screenshot { path } => {
            interp.run(&[parser::Command::Screenshot(path)])?;
        }
        Commands::Daemon { .. } | Commands::Doctor | Commands::Window { .. } => unreachable!(),
    }

    Ok(())
}

/// Default log for scheduled (`in`/`at`) actions: `~/.local/share/mk/scheduled.log`
/// (XDG_DATA_HOME-aware), created on demand. Append-only, so the trail accrues.
fn default_scheduled_log_path() -> String {
    let base = std::env::var("XDG_DATA_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            format!("{home}/.local/share")
        });
    let dir = format!("{base}/mk");
    let _ = std::fs::create_dir_all(&dir);
    format!("{dir}/scheduled.log")
}

/// Handle `mk window {list,active,focus}` — printing JSON for list/active so
/// the output is machine-consumable (e.g. by an agent picking a click target).
fn handle_window(action: WindowAction) -> Result<()> {
    use mk::windows;
    match action {
        WindowAction::List => {
            let list = windows::list_windows()?;
            println!("{}", serde_json::to_string_pretty(&list)?);
        }
        WindowAction::Active => {
            let active = windows::active_window()?;
            println!("{}", serde_json::to_string_pretty(&active)?);
        }
        WindowAction::Focus { id } => {
            windows::focus_window(&id)?;
            println!("Focused window {id}");
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn daemon_start() -> Result<()> {
    use std::process::Command;

    // Check if already running
    if input::daemon::daemon_is_running() {
        println!("mk-daemon is already running.");
        return Ok(());
    }

    println!("Starting mk-daemon (requires root)...");

    // Find the mk-daemon binary in the same directory as mk
    let mk_path = std::env::current_exe()?;
    let mk_dir = mk_path.parent().unwrap_or(Path::new("."));
    let daemon_path = mk_dir.join("mk-daemon");

    let status = Command::new("sudo")
        .arg(&daemon_path)
        .arg("--foreground")
        .status()
        .context("Failed to start mk-daemon. Is sudo available?")?;

    if !status.success() {
        bail!("mk-daemon exited with status: {status}");
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn daemon_stop() -> Result<()> {
    use std::process::Command;

    let output = Command::new("pkill")
        .arg("-f")
        .arg("mk-daemon")
        .output()
        .context("Failed to run pkill")?;

    if output.status.success() {
        println!("mk-daemon stopped.");
    } else {
        println!("mk-daemon was not running.");
    }

    // Clean up socket
    let _ = std::fs::remove_file("/tmp/mk-daemon.sock");

    Ok(())
}

#[cfg(target_os = "linux")]
fn daemon_status() -> Result<()> {
    if input::daemon::daemon_is_running() {
        println!("mk-daemon: running (socket /tmp/mk-daemon.sock)");
        match input::daemon::ping_daemon() {
            Ok(()) => println!("  Response: OK"),
            Err(e) => println!("  Ping failed: {e}"),
        }
    } else {
        println!("mk-daemon: not running");
        println!();
        println!("To start: sudo mk-daemon");
        println!("  or:     sudo mk daemon start");
    }
    Ok(())
}
