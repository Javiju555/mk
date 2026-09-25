//! `mk::engine` — embeddable macro engine.
//!
//! mk is a library first: this module is the stable surface a third-party
//! binary (e.g. a macro deck) uses to run macros in-process, concurrently,
//! with cancellation and streaming events. Runtime-neutral by design: the
//! engine is plain threads + channels, so an async host can wrap it with
//! `spawn_blocking` or bridge the receiver into a stream.
//!
//! ```no_run
//! use std::sync::{Arc, Mutex};
//! let engine = mk::engine::Engine::new(mk::engine::backends::detect()?);
//! let prog = engine.parse("text \"hola\"\nkey \"ctrl+s\"")?;
//! let (handle, events) = engine.spawn_channel(prog, &Default::default());
//! while let Ok(ev) = events.recv() { /* paint a key, log, ... */ }
//! let report = handle.wait()?;
//! # Ok::<(), anyhow::Error>(())
//! ```

pub mod runner;

use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;

pub use runner::Cancel;
use runner::Runner;

use crate::input::Backend;
use crate::parser::Command;

/// What to do when a step fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorPolicy {
    /// Abort the macro (what the CLI has always done).
    Stop,
    /// Record the failure, keep going (better for UI macros: one dead key
    /// shouldn't kill the rest).
    Continue,
}

#[derive(Debug, Clone)]
pub struct RunOptions {
    pub policy: ErrorPolicy,
    /// Wall-clock cap for the whole macro. Steps that would start after it
    /// are skipped.
    pub deadline: Option<Duration>,
}

impl Default for RunOptions {
    fn default() -> Self {
        // Embedders default to resilient; the CLI passes Stop explicitly.
        Self {
            policy: ErrorPolicy::Continue,
            deadline: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct Event {
    /// Zero-based index of the step within the macro run.
    pub step: usize,
    /// Stable action name ("type_text", "press_key", "mouse_click", ...).
    pub action: String,
    /// Human-readable detail, mirroring the CLI log strings.
    pub detail: String,
    pub outcome: Outcome,
}

impl Event {
    pub fn is_ok(&self) -> bool {
        self.outcome == Outcome::Ok
    }
}

#[derive(Debug, Clone, Default)]
pub struct RunReport {
    pub steps: usize,
    pub failed: usize,
    pub cancelled: bool,
    pub duration: Duration,
}

/// Receives events as they happen. Implement it, wrap a closure with
/// [`FnSink`], or collect into [`CollectSink`].
pub trait EventSink: Send {
    fn emit(&mut self, ev: Event);
}

/// Adapts a closure into a sink: `FnSink(|ev| println!("{ev:?}"))`.
pub struct FnSink<F>(pub F);

impl<F: FnMut(Event) + Send> EventSink for FnSink<F> {
    fn emit(&mut self, ev: Event) {
        (self.0)(ev)
    }
}

/// Collects every event in memory — handy in tests and for a dry audit.
#[derive(Default)]
pub struct CollectSink(pub Vec<Event>);

impl EventSink for CollectSink {
    fn emit(&mut self, ev: Event) {
        self.0.push(ev)
    }
}

/// A parsed macro. Cheap to clone; `Command` payloads are strings, so cloning
/// a program is not free but is predictable.
#[derive(Debug, Clone)]
pub struct Program {
    pub(crate) steps: Vec<Command>,
}

impl Program {
    pub fn len(&self) -> usize {
        self.steps.len()
    }
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
    pub fn steps(&self) -> &[Command] {
        &self.steps
    }
}

/// A running (or finished) macro.
pub struct MacroHandle {
    cancel: Cancel,
    join: Option<std::thread::JoinHandle<Result<RunReport>>>,
}

impl MacroHandle {
    /// Request cancellation. The macro stops at the next step boundary, or
    /// within ~50ms if it is sleeping.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    /// Block until the macro finishes and get its report.
    pub fn wait(mut self) -> Result<RunReport> {
        match self.join.take() {
            Some(h) => h
                .join()
                .map_err(|_| anyhow::anyhow!("macro thread panicked"))?,
            None => anyhow::bail!("macro already waited"),
        }
    }
}

#[derive(Clone)]
pub struct Engine {
    backend: Arc<dyn Backend>,
}

impl Engine {
    /// Wrap any backend — mk's own, or one the embedder supplies.
    pub fn new(backend: Arc<dyn Backend>) -> Self {
        Self { backend }
    }

    /// Use mk's auto-detected backend for this machine.
    pub fn detect() -> Result<Self> {
        Ok(Self::new(backends::detect()?))
    }

    pub fn backend_name(&self) -> String {
        self.backend.display_name().to_string()
    }

    /// Parse `.mk` source into a program (variables left for run time).
    pub fn parse(&self, src: &str) -> Result<Program> {
        let vars = std::collections::HashMap::new();
        let steps = crate::parser::parse_script_with_vars(src, &vars, None)?;
        Ok(Program { steps })
    }

    /// Build a program from commands the caller assembled in code.
    pub fn program(&self, steps: Vec<Command>) -> Program {
        Program { steps }
    }

    /// Run on the current thread. Returns the report; a step failure with
    /// `ErrorPolicy::Stop` also surfaces as `Err`.
    pub fn run_blocking(
        &self,
        program: &Program,
        sink: &mut dyn EventSink,
        opts: &RunOptions,
    ) -> Result<RunReport> {
        self.run_with_cancel(program, sink, opts, &Cancel::new(), false)
    }

    /// Run on a background thread with a shared sink.
    pub fn spawn(
        &self,
        program: Program,
        sink: Arc<Mutex<dyn EventSink>>,
        opts: RunOptions,
    ) -> MacroHandle {
        let backend = self.backend.clone();
        let steps = program.steps;
        let cancel = Cancel::new();
        let c = cancel.clone();
        let join = std::thread::spawn(move || {
            let started = Instant::now();
            let mut guard = sink.lock().expect("event sink mutex poisoned");
            let mut runner = Runner::new(backend.as_ref(), false, &mut *guard, &opts, &c);
            let res = runner.run(&steps);
            let mut report = runner.report().clone();
            report.duration = started.elapsed();
            res.map(|()| report)
        });
        MacroHandle {
            cancel,
            join: Some(join),
        }
    }

    /// Like `spawn`, but events are also streamed over a channel so the
    /// caller can react while the macro is still running.
    pub fn spawn_channel(&self, program: Program, opts: &RunOptions) -> (MacroHandle, Receiver<Event>) {
        let (tx, rx) = channel();
        let forwarder = ChannelSink(tx);
        let handle = self.spawn(program, Arc::new(Mutex::new(forwarder)), opts.clone());
        (handle, rx)
    }

    /// Shared entry point for both `run_blocking` and the CLI adapter.
    pub(crate) fn run_with_cancel(
        &self,
        program: &Program,
        sink: &mut dyn EventSink,
        opts: &RunOptions,
        cancel: &Cancel,
        dry_run: bool,
    ) -> Result<RunReport> {
        let started = Instant::now();
        let mut runner = Runner::new(self.backend.as_ref(), dry_run, sink, opts, cancel);
        let res = runner.run(&program.steps);
        let mut report = runner.report().clone();
        report.duration = started.elapsed();
        res.map(|()| report)
    }
}

struct ChannelSink(Sender<Event>);

impl EventSink for ChannelSink {
    fn emit(&mut self, ev: Event) {
        // A dropped receiver just means nobody is listening; not an error.
        let _ = self.0.send(ev);
    }
}

/// Platform backend constructors, for embedders that want to be explicit
/// instead of using `Engine::detect()`.
pub mod backends {
    use anyhow::Result;

    pub fn detect() -> Result<std::sync::Arc<dyn crate::input::Backend>> {
        Ok(crate::input::detect_backend()?.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Fake backend: counts calls, can be made to fail. Never touches the OS.
    struct Fake {
        calls: Arc<AtomicUsize>,
        fail: bool,
    }

    impl Backend for Fake {
        fn type_text(&self, _text: &str) -> Result<()> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                anyhow::bail!("fake backend failure")
            } else {
                Ok(())
            }
        }
        fn press_key(&self, _key: &str) -> Result<()> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                anyhow::bail!("fake backend failure")
            } else {
                Ok(())
            }
        }
        fn display_name(&self) -> &str {
            "fake"
        }
    }

    fn fake() -> (Arc<AtomicUsize>, Engine) {
        let calls = Arc::new(AtomicUsize::new(0));
        let backend = Arc::new(Fake {
            calls: calls.clone(),
            fail: false,
        });
        (calls, Engine::new(backend))
    }

    #[test]
    fn test_parse_and_run_ok() {
        let (calls, engine) = fake();
        let prog = engine.parse("text \"hola\"\nkey \"ctrl+s\"\nwait \"10ms\"").unwrap();
        let mut sink = CollectSink::default();
        let report = engine
            .run_blocking(&prog, &mut sink, &RunOptions::default())
            .unwrap();
        assert_eq!(report.steps, 3);
        assert_eq!(report.failed, 0);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(sink.0.len(), 3);
        assert!(sink.0.iter().all(|e| e.is_ok()));
        assert_eq!(sink.0[0].action, "type_text");
        assert_eq!(sink.0[1].action, "press_key");
    }

    #[test]
    fn test_error_policy_continue_vs_stop() {
        let calls = Arc::new(AtomicUsize::new(0));
        let engine = Engine::new(Arc::new(Fake {
            calls: calls.clone(),
            fail: true,
        }));
        let prog = engine.parse("text \"a\"\ntext \"b\"").unwrap();

        let mut sink = CollectSink::default();
        let report = engine
            .run_blocking(
                &prog,
                &mut sink,
                &RunOptions {
                    policy: ErrorPolicy::Continue,
                    deadline: None,
                },
            )
            .unwrap();
        assert_eq!(report.steps, 2, "both steps attempted");
        assert_eq!(report.failed, 2);
        assert_eq!(calls.load(Ordering::SeqCst), 2, "kept going after failure");

        let mut sink = CollectSink::default();
        let res = engine.run_blocking(
            &prog,
            &mut sink,
            &RunOptions {
                policy: ErrorPolicy::Stop,
                deadline: None,
            },
        );
        assert!(res.is_err(), "Stop surfaces the error");
        assert_eq!(sink.0.len(), 1, "aborted after first failure");
    }

    #[test]
    fn test_deadline_skips_remaining_steps() {
        let (calls, engine) = fake();
        let prog = engine.parse("wait \"30s\"\ntext \"nope\"\ntext \"nope2\"").unwrap();
        let (handle, events) = engine.spawn_channel(
            prog,
            &RunOptions {
                policy: ErrorPolicy::Continue,
                // Generous enough that the first step always starts, even on a
                // loaded machine: we are testing the cut, not the race.
                deadline: Some(Duration::from_millis(400)),
            },
        );
        let report = handle.wait().unwrap();
        let events: Vec<_> = events.into_iter().collect();
        // The wait was attempted and cut short: reported as a failure so a UI
        // can react, and the steps after it never ran.
        assert_eq!(report.steps, 1);
        assert_eq!(report.failed, 1);
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0].outcome, Outcome::Failed(m) if m.contains("deadline")));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_cancel_is_prompt_during_long_wait() {
        let (calls, engine) = fake();
        let prog = engine.parse("wait \"30s\"\ntext \"nope\"").unwrap();
        let (handle, _rx) = engine.spawn_channel(prog, &RunOptions::default());
        std::thread::sleep(Duration::from_millis(100));
        let start = Instant::now();
        handle.cancel();
        let report = handle.wait().unwrap();
        assert!(start.elapsed() < Duration::from_secs(2), "cancel was prompt");
        assert!(report.cancelled, "report says cancelled");
        assert_eq!(calls.load(Ordering::SeqCst), 0, "later step never ran");
    }

    #[test]
    fn test_concurrent_macros_are_isolated() {
        let (calls, engine) = fake();
        let a = engine.program(vec![
            Command::Set("who".into(), "a".into()),
            Command::Text("${who}-1".into()),
        ]);
        let b = engine.program(vec![
            Command::Set("who".into(), "b".into()),
            Command::Text("${who}-1".into()),
        ]);
        let opts = RunOptions::default();
        let (ha, ea) = engine.spawn_channel(a, &opts);
        let (hb, eb) = engine.spawn_channel(b, &opts);
        ha.wait().unwrap();
        hb.wait().unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        let a_events: Vec<_> = ea.into_iter().collect();
        let b_events: Vec<_> = eb.into_iter().collect();
        assert_eq!(a_events.len(), 2);
        assert_eq!(b_events.len(), 2);
        // Each macro expanded its own `who`: no cross-talk on the shared backend.
        assert_eq!(a_events[1].detail, "a-1");
        assert_eq!(b_events[1].detail, "b-1");
    }

    #[test]
    fn test_events_stream_before_completion() {
        let (_calls, engine) = fake();
        let prog = engine.parse("text \"a\"\nwait \"300ms\"\ntext \"b\"").unwrap();
        let (handle, rx) = engine.spawn_channel(prog, &RunOptions::default());
        // First event must arrive while the macro is still sleeping.
        let ev = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(ev.action, "type_text");
        assert!(!handle.is_cancelled());
        // The wait event lands mid-macro: that is what "streaming" means for a
        // UI that wants to show progress before the macro finishes.
        let second = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(second.action, "wait");
        let report = handle.wait().unwrap();
        assert_eq!(report.steps, 3);
    }

    #[test]
    fn test_parse_reports_bad_syntax() {
        let (_calls, engine) = fake();
        assert!(engine.parse("comando-inexistente").is_err());
    }
}
