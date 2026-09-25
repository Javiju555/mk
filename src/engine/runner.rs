//! The step loop: the single implementation of "run a macro".
//!
//! `Interpreter` (CLI/scripts) is a thin adapter over `Runner`, so the two
//! can never drift. Everything the runner needs to be embeddable lives here:
//! structured events, per-step error policy, cancellation and deadline — with
//! waits sliced so a cancel does not wait out a long sleep.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::Result;

use crate::input::Backend;
use crate::output::paste;
use crate::parser::{self, Command, expand_vars, unquote};

/// Expand variables and strip quotes, returning an owned String (avoids
/// borrowing a temporary).
fn clean(s: &str, vars: &HashMap<String, String>) -> String {
    unquote(&expand_vars(s, vars)).to_string()
}

use super::{ErrorPolicy, Event, EventSink, Outcome, RunReport, RunOptions};

/// Shared cancellation flag. Cheap to clone, checked between steps and inside
/// sliced sleeps.
#[derive(Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Wait slice: how often a sleep re-checks cancel/deadline.
const SLICE: Duration = Duration::from_millis(50);

pub struct Runner<'a> {
    backend: &'a dyn Backend,
    dry_run: bool,
    policy: ErrorPolicy,
    deadline: Option<Instant>,
    cancel: &'a Cancel,
    /// One event per step, emitted as it happens (streaming, not buffered).
    sink: &'a mut dyn EventSink,
    pub vars: HashMap<String, String>,
    /// Number of top-level steps attempted; used for `Report::steps`.
    counter: usize,
    report: RunReport,
}

impl<'a> Runner<'a> {
    pub fn new(
        backend: &'a dyn Backend,
        dry_run: bool,
        sink: &'a mut dyn EventSink,
        opts: &RunOptions,
        cancel: &'a Cancel,
    ) -> Self {
        Self {
            backend,
            dry_run,
            policy: opts.policy,
            deadline: opts.deadline.map(|d| Instant::now() + d),
            cancel,
            sink,
            vars: HashMap::new(),
            counter: 0,
            report: RunReport::default(),
        }
    }

    pub fn report(&self) -> &RunReport {
        &self.report
    }

    /// Sleep that honours cancel + deadline, in slices.
    fn sleep(&mut self, mut total: Duration) -> bool {
        while !total.is_zero() {
            if self.cancel.is_cancelled() {
                self.report.cancelled = true;
                return false;
            }
            if let Some(d) = self.deadline {
                if Instant::now() >= d {
                    return false;
                }
            }
            let chunk = total.min(SLICE);
            std::thread::sleep(chunk);
            total -= chunk;
        }
        true
    }

    /// Sleep honouring cancel + deadline. A wait cut short reports *why* in
    /// its event (so a UI can paint the key red) instead of claiming success.
    fn sleep_or_cut(&mut self, total: Duration) -> Result<()> {
        if self.sleep(total) {
            Ok(())
        } else if self.cancel.is_cancelled() {
            anyhow::bail!("cancelled during wait")
        } else {
            anyhow::bail!("deadline exceeded during wait")
        }
    }

    fn expired(&mut self) -> bool {
        if self.cancel.is_cancelled() {
            self.report.cancelled = true;
            return true;
        }
        if let Some(d) = self.deadline {
            if Instant::now() >= d {
                return true;
            }
        }
        false
    }

    /// Run a list of commands, honouring policy/cancel/deadline throughout.
    pub fn run(&mut self, commands: &[Command]) -> Result<()> {
        for cmd in commands {
            if self.expired() {
                return Ok(());
            }
            self.step(cmd)?;
        }
        Ok(())
    }

    /// Execute one command, recording exactly one event for it (nested
    /// blocks report their own inner steps, not the block itself).
    fn step(&mut self, cmd: &Command) -> Result<()> {
        let index = self.counter;
        self.counter += 1;
        let (action, detail) = describe(cmd, &self.vars);
        let result = self.perform(cmd);

        match result {
            Ok(()) => {
                self.report.steps += 1;
                self.sink.emit(Event {
                    step: index,
                    action: action.to_string(),
                    detail: detail.to_string(),
                    outcome: Outcome::Ok,
                });
                Ok(())
            }
            Err(e) => {
                let msg = e.to_string();
                self.report.steps += 1;
                self.report.failed += 1;
                self.sink.emit(Event {
                    step: index,
                    action: action.to_string(),
                    detail: detail.to_string(),
                    outcome: Outcome::Failed(msg.clone()),
                });
                match self.policy {
                    // The CLI keeps the old abort-on-first-failure contract;
                    // embedders can opt into running the rest of the macro.
                    ErrorPolicy::Stop => Err(e),
                    ErrorPolicy::Continue => Ok(()),
                }
            }
        }
    }

    /// The actual effect of a command. Errors are returned (not logged) so
    /// `step` can apply the policy uniformly.
    fn perform(&mut self, cmd: &Command) -> Result<()> {
        match cmd {
            Command::Set(name, value) => {
                let expanded = expand_vars(value, &self.vars);
                self.vars.insert(name.clone(), expanded);
                Ok(())
            }
            Command::Include(_) => Ok(()), // resolved before execution
            Command::Repeat(count, inner) => {
                for _ in 0..*count {
                    if self.expired() {
                        break;
                    }
                    self.run(inner)?;
                }
                Ok(())
            }
            Command::In(dur, inner) => {
                if !self.sleep(*dur) {
                    return Ok(());
                }
                self.run(inner)
            }
            Command::At(time, inner) => {
                let delay = crate::scheduler::delay_until_time(time)?;
                if !self.sleep(delay) {
                    return Ok(());
                }
                self.run(inner)
            }
            Command::KeepAwake(dur) => {
                if self.dry_run {
                    Ok(())
                } else {
                    crate::scheduler::keep_awake_background(*dur, "F15".to_string());
                    Ok(())
                }
            }
            Command::Text(text) => {
                let text = expand_vars(text, &self.vars);
                if self.dry_run {
                    Ok(())
                } else {
                    self.backend.type_text(&text)
                }
            }
            Command::Enter => self.press("enter"),
            Command::Key(key) => {
                let key = expand_vars(key, &self.vars);
                self.press(&key)
            }
            Command::Wait(dur) => self.sleep_or_cut(*dur),
            Command::Paste(text, shortcut) => {
                let text = expand_vars(text, &self.vars);
                let shortcut = expand_vars(shortcut, &self.vars);
                if self.dry_run {
                    Ok(())
                } else {
                    paste::paste(&text, &shortcut, self.backend)
                }
            }
            Command::PasteFile(path) => {
                let path = expand_vars(path, &self.vars);
                let file_path = std::path::Path::new(&path);
                let content = std::fs::read_to_string(file_path)
                    .map_err(|e| anyhow::anyhow!("Failed to read paste-file: {path}: {e}"))?;
                let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
                let formatted = format!("Archivo: {path}\n```{ext}\n{content}\n```\n");
                if self.dry_run {
                    Ok(())
                } else {
                    paste::paste(&formatted, "ctrl+v", self.backend)
                }
            }
            Command::PasteDir(path) => {
                let path = expand_vars(path, &self.vars);
                if self.dry_run {
                    Ok(())
                } else {
                    let formatted = collect_dir_contents(std::path::Path::new(&path))?;
                    paste::paste(&formatted, "ctrl+v", self.backend)
                }
            }
            Command::Exec(var_name, command_str) => {
                let command_str = expand_vars(command_str, &self.vars);
                if self.dry_run {
                    self.vars.insert(
                        var_name.clone(),
                        format!("[dry-run output of {command_str}]"),
                    );
                } else {
                    let output = std::process::Command::new("sh")
                        .arg("-c")
                        .arg(&command_str)
                        .output();
                    let out = match output {
                        Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
                        Err(e) => format!("Error executing command: {e}"),
                    };
                    self.vars.insert(var_name.clone(), out);
                }
                Ok(())
            }
            Command::Focus(id) => {
                let id = clean(id, &self.vars);
                if self.dry_run {
                    Ok(())
                } else {
                    crate::windows::focus_window(&id)
                }
            }
            Command::MouseMove(x, y, dur) => {
                let (x, y, ms) = self.coords(x, y, dur)?;
                if self.dry_run {
                    Ok(())
                } else {
                    self.backend.mouse_move(x, y, ms)
                }
            }
            Command::MouseClick(x, y, button, dur) => {
                let (x, y, ms) = self.coords(x, y, dur)?;
                let button = clean(button, &self.vars);
                if self.dry_run {
                    Ok(())
                } else {
                    self.backend.mouse_click(x, y, &button, ms)
                }
            }
            Command::MouseDrag(x1, y1, x2, y2, dur) => {
                let (x1, y1, _) = self.coords(x1, y1, "0s")?;
                let (x2, y2, ms) = self.coords(x2, y2, dur)?;
                if self.dry_run {
                    Ok(())
                } else {
                    self.backend.mouse_drag(x1, y1, x2, y2, ms)
                }
            }
            Command::MouseDown(button) => {
                let button = clean(button, &self.vars);
                if self.dry_run {
                    Ok(())
                } else {
                    self.backend.mouse_down(&button)
                }
            }
            Command::MouseUp(button) => {
                let button = clean(button, &self.vars);
                if self.dry_run {
                    Ok(())
                } else {
                    self.backend.mouse_up(&button)
                }
            }
            Command::MouseScroll(clicks, horizontal) => {
                let clicks: i32 = clean(clicks, &self.vars)
                    .parse()
                    .map_err(|_| anyhow::anyhow!("scroll clicks must be a number"))?;
                let h = clean(horizontal, &self.vars).to_lowercase();
                let horizontal = h == "horizontal" || h == "true" || h == "h";
                if self.dry_run {
                    Ok(())
                } else {
                    self.backend.mouse_scroll(clicks, horizontal)
                }
            }
            Command::Screenshot(path, raw, quality) => {
                let path = clean(path, &self.vars);
                if self.dry_run {
                    Ok(())
                } else {
                    let fmt = if *raw {
                        crate::vision::ScreenshotFormat::Raw
                    } else {
                        crate::vision::ScreenshotFormat::Compressed
                    };
                    crate::vision::capture_screen(&path, fmt, *quality)
                }
            }
            Command::ScreenshotWindow(window_id, path, raw, quality) => {
                let id = clean(window_id, &self.vars);
                let path = clean(path, &self.vars);
                if self.dry_run {
                    Ok(())
                } else {
                    let fmt = if *raw {
                        crate::vision::ScreenshotFormat::Raw
                    } else {
                        crate::vision::ScreenshotFormat::Compressed
                    };
                    crate::vision::capture_window(&id, &path, fmt, *quality)
                }
            }
            Command::ScreenshotMonitor(idx, path, raw, quality) => {
                let path = clean(path, &self.vars);
                if self.dry_run {
                    Ok(())
                } else {
                    let fmt = if *raw {
                        crate::vision::ScreenshotFormat::Raw
                    } else {
                        crate::vision::ScreenshotFormat::Compressed
                    };
                    crate::vision::capture_monitor(*idx, &path, fmt, *quality)
                }
            }
        }
    }

    fn press(&mut self, key: &str) -> Result<()> {
        if self.dry_run {
            Ok(())
        } else {
            self.backend.press_key(key)
        }
    }

    fn coords(&self, x: &str, y: &str, dur: &str) -> Result<(i32, i32, u64)> {
        let x: i32 = clean(x, &self.vars)
            .parse()
            .map_err(|_| anyhow::anyhow!("x coordinate must be a number"))?;
        let y: i32 = clean(y, &self.vars)
            .parse()
            .map_err(|_| anyhow::anyhow!("y coordinate must be a number"))?;
        let dur = parser::parse_duration(&clean(dur, &self.vars))?;
        Ok((x, y, dur.as_millis() as u64))
    }
}

/// Short (action, detail) label for an event — mirrors the strings the CLI
/// logger has always written, so old logs stay readable.
fn describe(cmd: &Command, vars: &HashMap<String, String>) -> (&'static str, String) {
    match cmd {
        Command::Set(name, _) => ("set", format!("{name} = ...")),
        Command::Include(_) => ("include", "resolved".into()),
        Command::Repeat(n, _) => ("repeat", format!("{n} time(s)")),
        Command::In(dur, _) => ("in", format!("{dur:?}")),
        Command::At(time, _) => ("at", time.clone()),
        Command::KeepAwake(dur) => ("keep-awake", format!("{dur:?}")),
        Command::Text(t) => ("type_text", expand_vars(t, vars)),
        Command::Enter => ("press_key", "enter".into()),
        Command::Key(k) => ("press_key", expand_vars(k, vars)),
        Command::Wait(d) => ("wait", format!("{d:?}")),
        Command::Paste(t, _) => ("paste", expand_vars(t, vars)),
        Command::PasteFile(p) => ("paste-file", expand_vars(p, vars)),
        Command::PasteDir(p) => ("paste-dir", expand_vars(p, vars)),
        Command::Exec(v, _) => ("exec", format!("{v} = ...")),
        Command::Focus(id) => ("focus_window", clean(id, vars)),
        Command::MouseMove(x, y, _) => (
            "mouse_move",
            format!(
                "({}, {})",
                clean(x, vars),
                clean(y, vars)
            ),
        ),
        Command::MouseClick(x, y, b, _) => (
            "mouse_click",
            format!(
                "button {} at ({}, {})",
                clean(b, vars),
                clean(x, vars),
                clean(y, vars)
            ),
        ),
        Command::MouseDrag(x1, y1, x2, y2, _) => (
            "mouse_drag",
            format!(
                "({}, {}) -> ({}, {})",
                clean(x1, vars),
                clean(y1, vars),
                clean(x2, vars),
                clean(y2, vars)
            ),
        ),
        Command::MouseDown(b) => ("mouse_down", clean(b, vars)),
        Command::MouseUp(b) => ("mouse_up", clean(b, vars)),
        Command::MouseScroll(c, h) => (
            "mouse_scroll",
            format!("clicks {}, horizontal {h}", clean(c, vars)),
        ),
        Command::Screenshot(p, _, _) => ("screenshot", clean(p, vars)),
        Command::ScreenshotWindow(w, p, _, _) => (
            "screenshot_window",
            format!("window={} path={}", clean(w, vars), clean(p, vars)),
        ),
        Command::ScreenshotMonitor(i, p, _, _) => (
            "screenshot_monitor",
            format!("monitor={i} path={}", clean(p, vars)),
        ),
    }
}

fn collect_dir_contents(dir: &std::path::Path) -> Result<String> {
    let mut combined = String::new();
    collect_dir_recursive(dir, dir, &mut combined)?;
    if combined.is_empty() {
        anyhow::bail!("No text files found in directory: {}", dir.display());
    }
    Ok(combined)
}

fn collect_dir_recursive(
    root: &std::path::Path,
    current: &std::path::Path,
    combined: &mut String,
) -> Result<()> {
    if current.is_file() {
        if let Ok(content) = std::fs::read_to_string(current) {
            let rel = current.strip_prefix(root).unwrap_or(current);
            let ext = current.extension().and_then(|e| e.to_str()).unwrap_or("");
            combined.push_str(&format!(
                "Archivo: {}\n```{}\n{}\n```\n\n",
                rel.display(),
                ext,
                content
            ));
        }
        return Ok(());
    }
    if current.is_dir() {
        let name = current.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with('.')
            || name == "node_modules"
            || name == "target"
            || name == "build"
            || name == "dist"
            || name == "venv"
        {
            return Ok(());
        }
        for entry in std::fs::read_dir(current)? {
            let entry = entry?;
            collect_dir_recursive(root, &entry.path(), combined)?;
        }
    }
    Ok(())
}
