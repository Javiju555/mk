//! Example embedder: drives `mk::engine` the way a macro-deck app would.
//!
//! It injects a **fake backend**, so it never touches the real keyboard,
//! mouse or screen — safe to run anywhere. It demonstrates the three things
//! an embedder needs:
//!
//!   1. run several macros at once,
//!   2. cancel one mid-flight (e.g. the user released a key),
//!   3. receive events *while* the macro runs (e.g. to light up a button).
//!
//! Run with: `cargo run --example embed`

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use mk::engine::{Engine, ErrorPolicy, Outcome, RunOptions};
use mk::input::Backend;

/// Stand-in for the real input backends: counts what it was asked to do so
/// the example can prove the engine drove it, without injecting anything.
struct CountingBackend {
    calls: Arc<AtomicUsize>,
    label: &'static str,
}

impl Backend for CountingBackend {
    fn type_text(&self, text: &str) -> Result<()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        println!("      [{}] type_text {text:?}", self.label);
        Ok(())
    }
    fn press_key(&self, key: &str) -> Result<()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        println!("      [{}] press_key {key:?}", self.label);
        Ok(())
    }
    fn display_name(&self) -> &str {
        "counting (fake)"
    }
}

fn main() -> Result<()> {
    let total = Arc::new(AtomicUsize::new(0));

    // One shared backend, three concurrent macros: exactly how a deck app
    // would wire it (one engine, many pressed keys).
    let engine = Engine::new(Arc::new(CountingBackend {
        calls: total.clone(),
        label: "shared",
    }));
    println!("backend: {}", engine.backend_name());

    let opts = RunOptions {
        policy: ErrorPolicy::Continue,
        deadline: Some(Duration::from_secs(10)),
    };

    // 1) A macro that finishes normally.
    let quick = engine.parse("text \"hola\"\nkey \"ctrl+s\"")?;
    let (quick_h, quick_events) = engine.spawn_channel(quick, &opts);

    // 2) A long macro we will cancel halfway through.
    let long = engine.parse("text \"largo\"\nwait \"30s\"\ntext \"nunca\"")?;
    let (long_h, long_events) = engine.spawn_channel(long, &opts);

    // 3) A macro built in code instead of parsed from text.
    let built = engine.program(vec![
        mk::parser::Command::Set("saludo".into(), "hola".into()),
        mk::parser::Command::Text("${saludo} mundo".into()),
    ]);
    let (built_h, built_events) = engine.spawn_channel(built, &opts);

    // Events arrive as they happen — paint keys here in a real app.
    let t0 = Instant::now();
    let mut seen = 0usize;
    loop {
        // `try_recv` so we can also watch the clock below.
        let mut idle = true;
        for rx in [&quick_events, &long_events, &built_events] {
            while let Ok(ev) = rx.try_recv() {
                idle = false;
                seen += 1;
                let mark = match ev.outcome {
                    Outcome::Ok => "ok  ",
                    Outcome::Failed(_) => "FAIL",
                };
                println!("  [{:>6.0}ms] {mark} step {} {:<12} {}", t0.elapsed().as_millis(), ev.step, ev.action, ev.detail);
            }
        }
        if seen >= 2 && long_h.is_cancelled() {
            break;
        }
        if t0.elapsed() > Duration::from_millis(300) && seen >= 2 {
            break;
        }
        if idle {
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    // The user let go of the key: stop that macro, and only that one.
    println!("\n-- cancelando la macro larga --");
    long_h.cancel();
    let cancel_at = Instant::now();
    let long_report = long_h.wait()?;

    let quick_report = quick_h.wait()?;
    let built_report = built_h.wait()?;

    println!("\n-- informes --");
    println!("rápida : {quick_report:?}");
    println!("larga  : {long_report:?} (cancelada en {:?}", cancel_at.elapsed());
    println!("código : {built_report:?}");
    println!("\nllamadas totales al backend: {}", total.load(Ordering::SeqCst));

    // The cancelled macro must not have reached its last step.
    assert!(long_report.cancelled, "la macro larga debía quedar cancelada");
    assert!(
        long_report.steps < 3,
        "la macro larga no debía ejecutar el paso final, {:?}",
        long_report.steps
    );
    assert_eq!(quick_report.failed, 0);
    assert_eq!(built_report.failed, 0);

    println!("\nOK: concurrencia, cancelación y streaming verificados (sin tocar el escritorio).");
    Ok(())
}
