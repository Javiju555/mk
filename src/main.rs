mod backend;
mod clipboard;
mod doctor;
mod parser;
mod paste;
mod scheduler;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use crate::backend::{Backend, DryRunBackend};
use crate::parser::{Interpreter, Logger};

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
    /// Copy text to clipboard and paste with Ctrl+V
    Paste {
        /// Text to paste
        text: String,
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
    /// Manage the mk-daemon service
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
    /// Check system dependencies and display diagnostics
    Doctor,
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
    /// Copy text to clipboard and paste with Ctrl+V
    Paste {
        /// Text to paste
        text: String,
    },
    /// Wait for a duration
    Wait {
        /// Duration: "10s", "5m", "250ms"
        duration: String,
    },
}

impl ScheduledAction {
    fn to_command(&self) -> parser::Command {
        match self {
            ScheduledAction::Text { message } => parser::Command::Text(message.clone()),
            ScheduledAction::Enter => parser::Command::Enter,
            ScheduledAction::Key { key } => parser::Command::Key(key.clone()),
            ScheduledAction::Paste { text } => parser::Command::Paste(text.clone()),
            ScheduledAction::Wait { duration } => parser::Command::Wait(
                parser::parse_duration(duration).unwrap_or(Duration::from_secs(0)),
            ),
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Doctor => {
            return doctor::run();
        }
        Commands::Daemon { action } => {
            return match action {
                DaemonAction::Start => daemon_start(),
                DaemonAction::Stop => daemon_stop(),
                DaemonAction::Status => daemon_status(),
            };
        }
        _ => {}
    }

    let real_backend = backend::detect_backend()?;

    let backend: Box<dyn Backend> = if cli.dry_run {
        Box::new(DryRunBackend)
    } else {
        real_backend
    };

    let mut logger = cli.log.as_deref().map(Logger::new).transpose()?;
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
        Commands::Paste { text } => {
            interp.run(&[parser::Command::Paste(text)])?;
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
        Commands::Daemon { .. } | Commands::Doctor => unreachable!(),
    }

    Ok(())
}

fn daemon_start() -> Result<()> {
    use std::process::Command;

    // Check if already running
    if backend::daemon::daemon_is_running() {
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

fn daemon_status() -> Result<()> {
    if backend::daemon::daemon_is_running() {
        println!("mk-daemon: running (socket /tmp/mk-daemon.sock)");
        match backend::daemon::ping_daemon() {
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
