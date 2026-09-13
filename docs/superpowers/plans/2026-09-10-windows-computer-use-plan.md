# Windows Computer-Use (UIA + fixes) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implementar `mk ui tree/click/toggle/set-value` vía UI Automation en Windows más 5 fixes (scroll negativos, window wait, pid, screenshot crop/zoom, flag --focus).

**Architecture:** Fachada estable en `src/accessibility/mod.rs`; backend real solo `cfg(windows)` en `src/accessibility/windows_uia.rs` con crate `uiautomation`. Fixes pequeños en `src/main.rs`, `src/windows/mod.rs`, `src/vision/mod.rs`. Todo lo UIA gateado para no romper Linux/mac.

**Tech Stack:** Rust 2024, clap 4 derive, serde_json, image 0.25, xcap 0.9.6, windows-sys 0.52 (existente), nuevos: `uiautomation` (solo Windows), `regex` (cross-platform).

## Global Constraints

- Todo código UIA/COM vive tras `#[cfg(target_os = "windows")]`; `use uiautomation` nunca fuera de ese gate.
- Nueva dependencia Windows va en `[target.'cfg(target_os = "windows")'.dependencies]` del `Cargo.toml`, nunca en `[dependencies]` global.
- `UiElement` extiende con `is_enabled: bool`, `is_offscreen: bool` usando `#[serde(default)]` para no romper JSON viejo.
- `--name` es exact-match tras `trim()` + case-insensitive por defecto; `--contains`/`--regex` son opt-in.
- Coordenadas solo como fallback explícito `--force-coords`, con warning de que reintroducen foco/DPI.
- `cargo test` verde + `cargo check --target x86_64-pc-windows-gnu` + `cargo check --target x86_64-apple-darwin` cero warnings antes de cada commit de tarea UIA.
- Salida `ui tree` y `window list/wait` = JSON pretty a stdout; errores a stderr con exit distinto de 0.

---

## File Structure

- Modify: `Cargo.toml` — añadir `uiautomation` (solo Windows) + `regex` (global). Responsabilidad: dependencias.
- Modify: `src/accessibility/mod.rs` — extender `UiElement`, añadir `MatchMode` + `match_name()` pura, fachada `get_ui_tree/click/toggle/set_value` que delega a `windows_uia` en Windows y a stub en resto. Responsabilidad: API pública estable.
- Create: `src/accessibility/windows_uia.rs` — backend UIA real (tree walk, find, invoke/toggle/value). Solo compila en Windows. Responsabilidad: hablar con COM/UIA.
- Modify: `src/main.rs` — `Scroll::clicks` con `allow_hyphen_values`, nuevo `UiAction` (`tree/click/toggle/set-value`), nuevo `WindowAction::Wait`, flags `--crop/--zoom` en `Screenshot`, flag `--focus` en `Move/Click/Scroll`. Responsabilidad: CLI.
- Modify: `src/windows/mod.rs` — `WindowInfo.pid: Option<u32>`, enriquecer en Windows con `GetWindowThreadProcessId`, helper `wait_for_window()`. Responsabilidad: ventanas.
- Modify: `src/vision/mod.rs` — `parse_crop_rect()` + `crop_and_zoom()` puras + aplicar en `capture_screen/monitor/window`. Responsabilidad: imagen.
- Modify: `src/parser.rs` — variante `Command::Screenshot` no cambia; script `screenshot` acepta `--crop/--zoom` si es trivial, si no se deja fuera (CLI directo cubre el caso agente). Responsabilidad: no romper scripts.
- Modify: `README.md`, `docs/windows-computer-use-notes.md`, `CHANGELOG.md` — documentar. Responsabilidad: docs.

---

### Task 1: Fix `mk scroll -6` (allow_hyphen_values)

**Files:**
- Modify: `src/main.rs:152-159` (`Commands::Scroll`) y `src/main.rs:333-340` (`ScheduledAction::Scroll`)
- Test: `src/main.rs` (test inline `#[cfg(test)]` o test en `src/parser.rs`; se usa test clap `try_parse_from`)

**Interfaces:**
- Consumes: nada previo.
- Produces: `mk scroll -6` parsea a `clicks=-6` (lo usan Tasks 8 y manual).

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn test_scroll_negative_parses_without_double_dash() {
    let cli = Cli::try_parse_from(["mk", "scroll", "-6"]).expect("scroll -6 should parse");
    match cli.command {
        Commands::Scroll { clicks, horizontal } => {
            assert_eq!(clicks, -6);
            assert!(!horizontal);
        }
        _ => panic!("expected Scroll"),
    }
}
```

Nota: si `Cli`/`Commands` no son visibles desde el módulo de test, pon este test en `src/main.rs` al fondo en `#[cfg(test)] mod cli_tests`.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_scroll_negative_parses_without_double_dash -- --nocapture`
Expected: FAIL con `error: unexpected argument '-6' found`.

- [ ] **Step 3: Write minimal implementation**

```rust
/// Scroll the mouse wheel
Scroll {
    /// Number of scroll clicks (negative for down/left, positive for up/right)
    #[arg(allow_hyphen_values = true)]
    clicks: i32,
    /// Scroll horizontally instead of vertically
    #[arg(long)]
    horizontal: bool,
},
```

Aplicar en AMBOS enums (`Commands::Scroll` y `ScheduledAction::Scroll`).

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test test_scroll_negative_parses_without_double_dash -- --nocapture`
Expected: PASS. Luego `cargo test` completo PASS.

- [ ] **Step 5: Commit**

```bash
git add src/main.rs
git commit -m "fix(cli): allow mk scroll -6 without -- separator"
```

---

### Task 2: `pid` en `WindowInfo` + `window list`

**Files:**
- Modify: `src/windows/mod.rs:5-15` (struct), `src/windows/mod.rs:438-466` (`list_windows`)
- Test: `src/windows/mod.rs` tests (`test_list_windows_runs` existe; añadir `test_window_info_pid_serializes`)

**Interfaces:**
- Consumes: nada.
- Produces: `WindowInfo.pid: Option<u32>` (Task 3 lo imprime en `wait`; Task 7 lo muestra en `ui tree` contexto).

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn test_window_info_pid_serializes() {
    let w = WindowInfo {
        id: "123".into(),
        title: "t".into(),
        app_name: "a".into(),
        x: 0, y: 0, width: 100, height: 100,
        is_active: false,
        pid: Some(4242),
    };
    let v = serde_json::to_value(&w).unwrap();
    assert_eq!(v["pid"], 4242);

    let w2 = WindowInfo { pid: None, ..w };
    let v2 = serde_json::to_value(&w2).unwrap();
    assert!(v2.get("pid").is_none());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_window_info_pid_serializes`
Expected: FAIL con `no field pid` / struct mismatch.

- [ ] **Step 3: Write minimal implementation**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub id: String,
    pub title: String,
    pub app_name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub is_active: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
}
```

Actualizar el constructor en `list_windows()`:

```rust
list.push(WindowInfo {
    id: w_id.to_string(),
    title: w_title,
    app_name: w_app_name,
    x: w_x,
    y: w_y,
    width: w_width,
    height: w_height,
    is_active,
    pid: pid_for_window(&w),
});
```

Añadir helper (Windows enriquece, resto `None`):

```rust
#[cfg(target_os = "windows")]
fn pid_for_window(w: &xcap::Window) -> Option<u32> {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
    let id: usize = w.id().ok()? as usize;
    let hwnd = id as windows_sys::Win32::Foundation::HWND;
    let mut pid: u32 = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut pid);
    }
    if pid == 0 { None } else { Some(pid) }
}

#[cfg(not(target_os = "windows"))]
fn pid_for_window(_w: &xcap::Window) -> Option<u32> {
    None
}
```

Nota: `GetWindowThreadProcessId` vive en `Win32_UI_WindowsAndMessaging` (feature ya habilitada en `Cargo.toml`). Si el compilador dice que no existe, buscar en `Win32_System_Threading` (también habilitada) antes de añadir features.

Arreglar el test existente `test_center_is_midpoint` añadiendo `pid: None` al literal.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test windows::`
Expected: PASS (3 tests). Luego `cargo check --target x86_64-pc-windows-gnu` PASS sin warnings.

- [ ] **Step 5: Commit**

```bash
git add src/windows/mod.rs
git commit -m "feat(window): expose pid in window list (Windows via GetWindowThreadProcessId)"
```

---

### Task 3: `mk window wait --title`

**Files:**
- Modify: `src/windows/mod.rs` (nuevo `wait_for_window`), `src/main.rs:196-245` (`WindowAction` + `handle_window`)
- Test: `src/windows/mod.rs` tests (mock con función inyectada)

**Interfaces:**
- Consumes: `list_windows() -> Result<Vec<WindowInfo>>` (Task 2 le añade `pid`, compatible).
- Produces: `wait_for_window(title, exact, timeout, interval) -> Result<WindowInfo>` (CLI lo usa; nada más lo necesita).

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn test_wait_for_window_timeout() {
    use std::time::Duration;
    let start = std::time::Instant::now();
    let res = wait_for_window_with(
        "ventana-que-no-existe-xyz",
        false,
        Duration::from_millis(120),
        Duration::from_millis(30),
        || Ok(vec![]),
    );
    assert!(res.is_err());
    assert!(start.elapsed() >= Duration::from_millis(100));
    assert!(res.unwrap_err().to_string().contains("Timeout"));
}

#[test]
fn test_wait_for_window_finds_on_retry() {
    use std::cell::Cell;
    use std::time::Duration;
    let calls = Cell::new(0);
    let res = wait_for_window_with("Code", false, Duration::from_secs(2), Duration::from_millis(10), || {
        calls.set(calls.get() + 1);
        if calls.get() < 3 {
            Ok(vec![])
        } else {
            Ok(vec![WindowInfo {
                id: "1".into(), title: "Visual Studio Code".into(), app_name: "Code".into(),
                x: 0, y: 0, width: 800, height: 600, is_active: true, pid: None,
            }])
        }
    });
    assert!(res.is_ok());
    assert_eq!(res.unwrap().id, "1");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test wait_for_window`
Expected: FAIL con `cannot find function wait_for_window_with`.

- [ ] **Step 3: Write minimal implementation**

```rust
/// Poll `list_fn` until a window matches `title` or `timeout` expires.
/// `exact=false` = substring case-insensitive; `exact=true` = trim + case-insensitive equality.
pub fn wait_for_window_with(
    title: &str,
    exact: bool,
    timeout: std::time::Duration,
    interval: std::time::Duration,
    mut list_fn: impl FnMut() -> anyhow::Result<Vec<WindowInfo>>,
) -> anyhow::Result<WindowInfo> {
    let deadline = std::time::Instant::now() + timeout;
    let query = title.trim().to_lowercase();
    loop {
        let list = list_fn().unwrap_or_default();
        let hit = list.into_iter().find(|w| {
            if exact {
                w.title.trim().to_lowercase() == query
            } else {
                w.title.to_lowercase().contains(&query)
            }
        });
        if let Some(w) = hit {
            return Ok(w);
        }
        if std::time::Instant::now() >= deadline {
            anyhow::bail!("Timeout esperando ventana con título '{title}' tras {}s", timeout.as_secs());
        }
        std::thread::sleep(interval);
    }
}

pub fn wait_for_window(title: &str, exact: bool, timeout: std::time::Duration, interval: std::time::Duration) -> anyhow::Result<WindowInfo> {
    wait_for_window_with(title, exact, timeout, interval, list_windows)
}
```

CLI en `src/main.rs`:

```rust
/// Wait until a window appears (polls list internally)
Wait {
    /// Substring to match against window title (use --exact for full match)
    #[arg(long)]
    title: String,
    /// Exact match instead of substring
    #[arg(long, default_value_t = false)]
    exact: bool,
    /// How long to wait, e.g. "10s" (default "10s")
    #[arg(long, default_value = "10s")]
    timeout: String,
    /// Poll interval, e.g. "400ms" (default "400ms")
    #[arg(long, default_value = "400ms")]
    interval: String,
},
```

En `handle_window`:

```rust
WindowAction::Wait { title, exact, timeout, interval } => {
    let t = mk::parser::parse_duration(&timeout)?;
    let i = mk::parser::parse_duration(&interval)?;
    let w = windows::wait_for_window(&title, exact, t, i)?;
    println!("{}", serde_json::to_string_pretty(&w)?);
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test wait_for_window`
Expected: PASS (2 tests). Manual: `mk window wait --title Code --timeout 3s` (encuentra o timeout limpio).

- [ ] **Step 5: Commit**

```bash
git add src/windows/mod.rs src/main.rs
git commit -m "feat(window): add window wait --title polling primitive"
```

---

### Task 4: Screenshot `--crop` / `--zoom`

**Files:**
- Modify: `src/vision/mod.rs` (nuevas `parse_crop_rect`, `crop_and_zoom`, flags en `capture_screen/monitor/window` o wrapper)
- Modify: `src/main.rs:162-181` (`Commands::Screenshot` flags)
- Test: `src/vision/mod.rs` tests

**Interfaces:**
- Consumes: nada (función pura sobre `DynamicImage`).
- Produces: `crop_and_zoom(img, rect, zoom) -> DynamicImage` + CLI flags (Task 9 documenta).

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn test_crop_and_zoom_center_pixel() {
    use image::{DynamicImage, Rgb, RgbImage};
    let mut img = RgbImage::new(100, 100);
    for y in 0..100 {
        for x in 0..100 {
            img.put_pixel(x, y, Rgb([x as u8, y as u8, 0]));
        }
    }
    let dyn_img = DynamicImage::ImageRgb8(img);
    let out = crop_and_zoom(&dyn_img, (10, 10, 20, 20), 2);
    assert_eq!(out.width(), 40);
    assert_eq!(out.height(), 40);

    let bad = std::panic::catch_unwind(|| crop_and_zoom(&dyn_img, (90, 90, 50, 50), 1));
    assert!(bad.is_err(), "crop fuera de imagen debe fallar");
}

#[test]
fn test_parse_crop_rect() {
    assert_eq!(parse_crop_rect("10,20,300,200").unwrap(), (10, 20, 300, 200));
    assert!(parse_crop_rect("10,20").is_err());
    assert!(parse_crop_rect("a,b,c,d").is_err());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test crop_and_zoom`
Expected: FAIL (`cannot find function`).

- [ ] **Step 3: Write minimal implementation**

```rust
/// Parse "x,y,w,h" into a rect tuple. All values in pixels, w/h > 0.
pub fn parse_crop_rect(s: &str) -> anyhow::Result<(u32, u32, u32, u32)> {
    let parts: Vec<&str> = s.split(',').map(|p| p.trim()).collect();
    if parts.len() != 4 {
        anyhow::bail!("--crop debe ser x,y,w,h (ej. 100,200,800,600), recibido: '{s}'");
    }
    let nums: Result<Vec<u32>, _> = parts.iter().map(|p| p.parse::<u32>()).collect();
    let nums = nums.map_err(|_| anyhow::anyhow!("--crop tiene valores no numéricos: '{s}'"))?;
    if nums[2] == 0 || nums[3] == 0 {
        anyhow::bail!("--crop w/h deben ser > 0: '{s}'");
    }
    Ok((nums[0], nums[1], nums[2], nums[3]))
}

/// Crop to `rect` then scale by `zoom` (Nearest neighbour: nítido para UI).
/// Panics if rect is outside the image — los callers validan antes con mensaje accionable.
pub fn crop_and_zoom(img: &DynamicImage, rect: (u32, u32, u32, u32), zoom: u32) -> DynamicImage {
    use image::imageops::FilterType;
    let (x, y, w, h) = rect;
    assert!(x + w <= img.width() && y + h <= img.height(), "crop {rect:?} fuera de imagen {}x{}", img.width(), img.height());
    let zoom = zoom.clamp(1, 8);
    let cropped = img.crop_imm(x, y, w, h);
    if zoom == 1 {
        return cropped;
    }
    cropped.resize(w * zoom, h * zoom, FilterType::Nearest)
}
```

Validación con mensaje (en cada `capture_*` o en un wrapper `apply_crop_zoom`):

```rust
fn apply_crop_zoom(img: DynamicImage, crop: &Option<String>, zoom: u32) -> anyhow::Result<DynamicImage> {
    let Some(c) = crop else { return Ok(if zoom <= 1 { img } else { crop_and_zoom(&img, (0, 0, img.width(), img.height()), zoom) }); };
    let rect = parse_crop_rect(c)?;
    if rect.0 + rect.2 > img.width() || rect.1 + rect.3 > img.height() {
        anyhow::bail!("crop {rect:?} fuera de imagen {}x{}", img.width(), img.height());
    }
    Ok(crop_and_zoom(&img, rect, zoom))
}
```

CLI flags en `Commands::Screenshot` (y pasar a `ScreenshotMonitor`/`ScreenshotWindow`):

```rust
/// Crop region x,y,w,h applied after capture (e.g. "100,200,800,600")
#[arg(long)]
crop: Option<String>,
/// Zoom factor 1-8 applied after crop (default: 1)
#[arg(long, default_value_t = 1)]
zoom: u32,
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test crop`
Expected: PASS. Manual: `mk screenshot out.png --crop 10,10,200,200 --zoom 2`.

- [ ] **Step 5: Commit**

```bash
git add src/vision/mod.rs src/main.rs
git commit -m "feat(vision): screenshot --crop/--zoom for small-detail verification"
```

---

### Task 5: `UiElement` extendido + matching puro (sin UIA aún)

**Files:**
- Modify: `src/accessibility/mod.rs`
- Test: `src/accessibility/mod.rs` tests (existen `test_accessibility_stubs`; añadir matching)

**Interfaces:**
- Consumes: nada.
- Produces: `MatchMode::{Exact,Contains,Regex}` + `match_name(candidate, query, mode) -> bool` (Task 6 lo usa para filtrar nodos UIA).

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn test_match_name_exact_is_trim_case_insensitive() {
    assert!(match_name("Mezclador", "mezclador", &MatchMode::Exact));
    assert!(match_name("  Mezclador  ", "MEZCLADOR", &MatchMode::Exact));
    assert!(!match_name("Mezclador panel", "mezclador", &MatchMode::Exact));
}

#[test]
fn test_match_name_contains_and_regex() {
    assert!(match_name("Delay: relativamente sencillo", "delay", &MatchMode::Contains));
    assert!(!match_name("Delay", "lay$", &MatchMode::Contains));
    assert!(match_name("Track 01", r"^Track \d+$", &MatchMode::Regex));
    assert!(!match_name("Track AB", r"^Track \d+$", &MatchMode::Regex));
    assert!(!match_name("cualquier cosa", "(unclosed", &MatchMode::Regex), "regex inválida = false, no panic");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test match_name`
Expected: FAIL (`cannot find function match_name`).

- [ ] **Step 3: Write minimal implementation**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiElement {
    pub role: String,
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub is_enabled: bool,
    #[serde(default)]
    pub is_offscreen: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchMode {
    Exact,
    Contains,
    Regex,
}

/// Exact = trim + case-insensitive. Contains = substring case-insensitive.
/// Regex = crate `regex`, case-sensitive salvo `(?i)`; inválida → false.
pub fn match_name(candidate: &str, query: &str, mode: &MatchMode) -> bool {
    match mode {
        MatchMode::Exact => candidate.trim().to_lowercase() == query.trim().to_lowercase(),
        MatchMode::Contains => candidate.to_lowercase().contains(&query.to_lowercase()),
        MatchMode::Regex => regex::Regex::new(query).map(|re| re.is_match(candidate)).unwrap_or(false),
    }
}
```

Añadir a `Cargo.toml` (global, cross-platform):

```toml
regex = "1"
```

Actualizar `find_button`/`find_input` para usar `MatchMode::Exact` en vez de `contains` directo (cambio de comportamiento documentado: antes substring, ahora exacto; los callers futuros usan el modo explícito).

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test accessibility`
Expected: PASS. `cargo check --target x86_64-pc-windows-gnu` PASS.

- [ ] **Step 5: Commit**

```bash
git add src/accessibility/mod.rs Cargo.toml Cargo.lock
git commit -m "feat(a11y): UiElement flags + exact/contains/regex matching"
```

---

### Task 6: Backend UIA real en Windows

**Files:**
- Create: `src/accessibility/windows_uia.rs`
- Modify: `src/accessibility/mod.rs` (delegación `cfg`), `Cargo.toml` (dep Windows)
- Test: unit puro gateado `#[cfg(target_os = "windows")]` + test de humo manual (no asserts de HWND en CI)

**Interfaces:**
- Consumes: `MatchMode` + `match_name()` (Task 5), `UiElement` (Task 5).
- Produces: `ui_tree_for_window(window_id)`, `ui_click/ui_toggle/ui_set_value(window_id, query, mode)` (Task 7 los llama desde CLI).

- [ ] **Step 1: Write the failing test (gateado a Windows, no rompe Linux)**

```rust
#[cfg(target_os = "windows")]
#[test]
fn test_uia_module_loads() {
    let automation = crate::accessibility::windows_uia::automation();
    assert!(automation.is_ok(), "UIAutomation::new() debe inicializar COM");
}
```

En Linux/mac este test ni siquiera compila (correcto: está tras `cfg`).

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_uia_module_loads`
Expected en Linux: 0 tests corren (ningún match) + error de compilación solo si se referencia el módulo sin gate — confirma que el gate funciona. En Windows: FAIL con `no module windows_uia`.

- [ ] **Step 3: Write minimal implementation**

`Cargo.toml`:

```toml
[target.'cfg(target_os = "windows")'.dependencies]
windows-sys = { version = "0.52", features = [ ...existentes... ] }
uiautomation = "0.25.1"  # enmienda 2026-09-10 (decisión usuario): estable moderna real, no 0.14
```

`src/accessibility/windows_uia.rs`:

```rust
//! Backend UI Automation (solo Windows). Todo este archivo está tras cfg(windows) desde mod.rs.
use anyhow::{bail, Context, Result};
use uiautomation::{UIAutomation, UITreeWalker, TreeScope, UIElement};
use crate::accessibility::{MatchMode, UiElement, match_name};

pub fn automation() -> Result<UIAutomation> {
    UIAutomation::new().context("UIAutomation::new() falló (COM)")
}

fn to_ui_element(el: &UIElement) -> Result<UiElement> {
    let name = el.get_name().unwrap_or_default();
    let role = el.get_localized_control_type().unwrap_or_else(|_| el.get_control_type().map(|c| format!("{c:?}")).unwrap_or_default());
    let rect = el.get_bounding_rectangle().map(|r| (r.get_left(), r.get_top(), r.get_width(), r.get_height())).unwrap_or((0, 0, 0, 0));
    let is_enabled = el.is_enabled().unwrap_or(true);
    let is_offscreen = el.is_offscreen().unwrap_or(false);
    Ok(UiElement {
        role, name,
        x: rect.0, y: rect.1,
        width: rect.2.max(0) as u32, height: rect.3.max(0) as u32,
        is_enabled, is_offscreen,
    })
}

pub fn ui_tree_for_window(window_id: &str) -> Result<Vec<UiElement>> {
    let hwnd: usize = window_id.parse().map_err(|e| anyhow::anyhow!("Invalid window ID: {e}"))?;
    let automation = automation()?;
    let root = automation.element_from_handle(hwnd.into()).context("FromHandle falló (¿ventana cerrada?)")?;
    let walker = automation.get_control_view_walker().context("walker")?;
    let condition = automation.create_true_condition().context("true condition")?;
    let mut out = Vec::new();
    walk(&walker, &root, &condition, &mut out, 5000)?;
    Ok(out)
}

fn walk(walker: &UITreeWalker, el: &UIElement, cond: &uiautomation::UIMatcher, out: &mut Vec<UiElement>, cap: usize) -> Result<()> {
    if out.len() >= cap { return Ok(()); }
    if let Ok(ui) = to_ui_element(el) { out.push(ui); }
    let Ok(children) = el.find_all(TreeScope::Children, cond) else { return Ok(()); };
    for child in children {
        if out.len() >= cap { break; }
        walk(walker, &child, cond, out, cap)?;
        let _ = walker;
    }
    Ok(())
}

fn find(window_id: &str, query: &str, mode: &MatchMode) -> Result<UIElement> {
    let hwnd: usize = window_id.parse().map_err(|e| anyhow::anyhow!("Invalid window ID: {e}"))?;
    let automation = automation()?;
    let root = automation.element_from_handle(hwnd.into()).context("FromHandle falló")?;
    let condition = automation.create_true_condition().context("true condition")?;
    let all = root.find_all(TreeScope::Descendants, &condition).context("walk UIA")?;
    for el in all {
        let name = el.get_name().unwrap_or_default();
        if match_name(&name, query, mode) {
            return Ok(el);
        }
    }
    bail!("control '{query}' no encontrado en ventana {window_id} (prueba `mk ui tree --window {window_id}` para ver nombres exactos)");
}

pub fn ui_click(window_id: &str, query: &str, mode: &MatchMode) -> Result<UiElement> {
    let el = find(window_id, query, mode)?;
    let snapshot = to_ui_element(&el)?;
    match el.get_pattern::<uiautomation::patterns::UIInvokePattern>() {
        Ok(invoke) => { invoke.invoke().context("Invoke() falló")?; Ok(snapshot) }
        Err(_) => bail!("control '{}' (role={}) no expone InvokePattern. Prueba `mk ui toggle` o coordenadas con --force-coords", snapshot.name, snapshot.role),
    }
}

pub fn ui_toggle(window_id: &str, query: &str, mode: &MatchMode) -> Result<UiElement> {
    let el = find(window_id, query, mode)?;
    let snapshot = to_ui_element(&el)?;
    match el.get_pattern::<uiautomation::patterns::UITogglePattern>() {
        Ok(t) => { t.toggle().context("Toggle() falló")?; Ok(snapshot) }
        Err(_) => bail!("control '{}' (role={}) no expone TogglePattern", snapshot.name, snapshot.role),
    }
}

pub fn ui_set_value(window_id: &str, query: &str, value: &str, mode: &MatchMode) -> Result<UiElement> {
    let el = find(window_id, query, mode)?;
    let snapshot = to_ui_element(&el)?;
    match el.get_pattern::<uiautomation::patterns::UIValuePattern>() {
        Ok(v) => { v.set_value(value).context("SetValue() falló (¿control read-only?)")?; Ok(snapshot) }
        Err(_) => bail!("control '{}' (role={}) no expone ValuePattern", snapshot.name, snapshot.role),
    }
}
```

> Nota para el implementador: los nombres exactos de tipos (`UITreeWalker`, `UIMatcher`, `UIInvokePattern`...) varían entre versiones de `uiautomation`. Si `cargo check` falla por un nombre, consulta `docs.rs/uiautomation/<versión exacta del Cargo.lock>` y ajusta SOLO el nombre del tipo, manteniendo firmas y comportamiento. No cambies la arquitectura.

`src/accessibility/mod.rs` (delegación):

```rust
#[cfg(target_os = "windows")]
pub mod windows_uia;

#[cfg(target_os = "windows")]
pub use windows_uia::{ui_tree_for_window, ui_click, ui_toggle, ui_set_value};

#[cfg(not(target_os = "windows"))]
pub fn ui_tree_for_window(_window_id: &str) -> Result<Vec<UiElement>> { Ok(Vec::new()) }
// ... igual para ui_click/toggle/set_value: bail!("UI Automation solo disponible en Windows")
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --lib` (Linux: PASS, test UIA no compila/no corre — correcto)
Run: `cargo check --target x86_64-pc-windows-gnu`
Expected: PASS cero warnings. Si hay error de nombre de tipo, ajustar según nota y repetir.

- [ ] **Step 5: Commit**

```bash
git add src/accessibility/ Cargo.toml Cargo.lock
git commit -m "feat(a11y): Windows UIA backend (tree + invoke/toggle/value)"
```

---

### Task 7: CLI `mk ui` + `--focus` en acciones de coordenadas

**Files:**
- Modify: `src/main.rs` (enum `UiAction`, `Commands::Ui`, `handle_ui`, flags `--focus` en Move/Click/Scroll, help)
- Test: clap `try_parse_from` para `ui tree`, `ui click --name`, `click --focus`

**Interfaces:**
- Consumes: `ui_tree_for_window/ui_click/ui_toggle/ui_set_value` (Task 6), `windows::focus_window` (existente), `MatchMode` (Task 5).
- Produces: CLI usable por agentes (Task 9 lo documenta).

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn test_ui_cli_parses() {
    let cli = Cli::try_parse_from(["mk", "ui", "tree", "--window", "123"]).expect("ui tree parse");
    assert!(matches!(cli.command, Commands::Ui { .. }));
    let cli = Cli::try_parse_from(["mk", "ui", "click", "--window", "123", "--name", "Mezclador"]).expect("ui click parse");
    assert!(matches!(cli.command, Commands::Ui { .. }));
    let cli = Cli::try_parse_from(["mk", "click", "10", "20", "--focus", "123"]).expect("click --focus parse");
    assert!(matches!(cli.command, Commands::Click { .. }));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test test_ui_cli_parses`
Expected: FAIL (`Unrecognized subcommand 'ui'`, `unexpected argument '--focus'`).

- [ ] **Step 3: Write minimal implementation**

```rust
/// Semantic UI control via OS accessibility APIs (Windows UIA first)
Ui {
    #[command(subcommand)]
    action: UiAction,
},

#[derive(Subcommand)]
enum UiAction {
    /// List UI elements of a window as JSON (name-based targeting)
    Tree {
        /// Window id (from `mk window list`)
        #[arg(long)]
        window: String,
        /// Filter: substring (case-insensitive)
        #[arg(long)]
        contains: Option<String>,
        /// Filter: regex (needs `regex` semantics)
        #[arg(long)]
        regex: Option<String>,
        /// Filter by role (e.g. "button")
        #[arg(long)]
        role: Option<String>,
    },
    /// Invoke (click) a control by name — no focus or coordinates needed
    Click {
        #[arg(long)] window: String,
        #[arg(long)] name: String,
        #[arg(long)] contains: bool,
        #[arg(long)] regex: bool,
    },
    /// Toggle a checkbox/switch by name
    Toggle {
        #[arg(long)] window: String,
        #[arg(long)] name: String,
        #[arg(long)] contains: bool,
        #[arg(long)] regex: bool,
    },
    /// Set an edit/combo value by name
    SetValue {
        #[arg(long)] window: String,
        #[arg(long)] name: String,
        #[arg(long)] value: String,
        #[arg(long)] contains: bool,
        #[arg(long)] regex: bool,
    },
}
```

Modo:

```rust
fn match_mode(contains: bool, regex: bool) -> anyhow::Result<MatchMode> {
    match (contains, regex) {
        (false, false) => Ok(MatchMode::Exact),
        (true, false) => Ok(MatchMode::Contains),
        (false, true) => Ok(MatchMode::Regex),
        (true, true) => anyhow::bail!("usa --contains o --regex, no ambos"),
    }
}
```

`handle_ui` (solo llama a `mk::accessibility::*`; en no-Windows devuelven stub/error honesto):

```rust
fn handle_ui(action: UiAction) -> Result<()> {
    use mk::accessibility::{MatchMode, MatchMode as _};
    match action {
        UiAction::Tree { window, contains, regex, role } => {
            let mut tree = mk::accessibility::ui_tree_for_window(&window)?;
            if let Some(sub) = contains {
                tree.retain(|e| mk::accessibility::match_name(&e.name, &sub, &MatchMode::Contains));
            }
            if let Some(re) = regex {
                tree.retain(|e| mk::accessibility::match_name(&e.name, &re, &MatchMode::Regex));
            }
            if let Some(r) = role {
                tree.retain(|e| e.role.eq_ignore_ascii_case(&r));
            }
            println!("{}", serde_json::to_string_pretty(&tree)?);
        }
        UiAction::Click { window, name, contains, regex } => {
            let mode = match_mode(contains, regex)?;
            let el = mk::accessibility::ui_click(&window, &name, &mode)?;
            println!("{}", serde_json::to_string_pretty(&el)?);
        }
        UiAction::Toggle { .. } => { /* análogo con ui_toggle */ }
        UiAction::SetValue { window, name, value, contains, regex } => {
            let mode = match_mode(contains, regex)?;
            let el = mk::accessibility::ui_set_value(&window, &name, &value, &mode)?;
            println!("{}", serde_json::to_string_pretty(&el)?);
        }
    }
    Ok(())
}
```

Flag `--focus` (en `Move`, `Click`, `Scroll` y sus `ScheduledAction` si es trivial; mínimo en `Commands`):

```rust
/// Focus window id in-process before acting (avoids focus-revert between invocations)
#[arg(long)]
focus: Option<String>,
```

En cada handler, antes de `interp.run`:

```rust
if let Some(id) = focus.as_deref() {
    mk::windows::focus_window(id)?;
    std::thread::sleep(std::time::Duration::from_millis(150));
}
```

Actualizar `long_about` del CLI con la receta: `window list (fresco) → actuar con --focus o mk ui (sin foco) → screenshot -w --raw → leer`.

Añadir `Commands::Ui { action }` al `match` principal y a la rama `unreachable!()`.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test test_ui_cli_parses`
Expected: PASS. Manual Windows: `mk ui tree --window <id> | head`.

- [ ] **Step 5: Commit**

```bash
git add src/main.rs
git commit -m "feat(cli): mk ui tree/click/toggle/set-value + --focus flag"
```

---

### Task 8: Docs + gates finales

**Files:**
- Modify: `README.md` (sección `mk ui` + tabla), `docs/windows-computer-use-notes.md` (marcar gotchas 2,4,5,7 resueltos + receta nueva), `docs/window-control.md` (Phase 5 → done en Windows), `CHANGELOG.md` (0.7.0 Unreleased)
- Test: gates, sin código nuevo

**Interfaces:**
- Consumes: todo lo anterior.
- Produces: release documentada, lista para prueba manual del usuario.

- [ ] **Step 1: CHANGELOG (failing = ausente)**

Añadir arriba:

```markdown
## [0.7.0] - Unreleased

### Added
- `mk ui tree/click/toggle/set-value` (Windows UIA, exact-match por defecto, `--contains/--regex` opt-in, flags `is_enabled/is_offscreen`).
- `mk window wait --title` (poll con `--timeout/--interval`).
- `pid` en `mk window list` (Windows).
- `mk screenshot --crop x,y,w,h --zoom N`.
- `--focus <id>` en `move/click/scroll` + `mk scroll -6` sin `--`.
```

- [ ] **Step 2: README + notes**

README: tabla de comandos `ui` + ejemplo:

```bash
mk window list
mk ui tree --window 329272
mk ui click --window 329272 --name "Mezclador"
mk screenshot -w 329272 --raw --crop 100,200,800,600 --zoom 2
```

`windows-computer-use-notes.md`: anteponer `> Update 2026-09-10: gotchas 2 (scroll), 4 (crop/zoom), 5 (wait), 7 (pid) resueltos; UIA (`mk ui`) implementado — ver spec.` y actualizar receta con `--focus` y path UIA primero.

`window-control.md`: Phase 5 línea Windows → `(done en Windows 0.7.0 vía `uiautomation`; Linux/mac siguen stub)`.

- [ ] **Step 3: Gates**

Run: `cargo test`
Expected: PASS (todos, incluyendo los 58 preexistentes + nuevos).

Run: `cargo check --target x86_64-pc-windows-gnu`
Run: `cargo check --target x86_64-apple-darwin`
Expected: PASS cero warnings.

- [ ] **Step 4: Commit**

```bash
git add README.md docs/windows-computer-use-notes.md docs/window-control.md CHANGELOG.md
git commit -m "docs: computer-use Windows 0.7.0 (ui + wait/pid/crop/focus)"
```

- [ ] **Step 5: Prueba manual (usuario en Windows real)**

```bash
mk window list
mk ui tree --window <id>
mk ui click --window <id> --name "<nombre exacto de tree>"
mk scroll -6
mk window wait --title "<app>" --timeout 10s
mk screenshot -w <id> --raw --crop 100,200,800,600 --zoom 2
```

Criterio: `ui click` funciona sin foco previo; `scroll -6` sin `--`; `wait` encuentra o timeout limpio; `crop/zoom` legible.

---

## Self-Review (writing-plans)

1. **Spec coverage:** S1 arquitectura → Tasks 5+6. S2 CLI/matching → Tasks 5+7 (exact/contains/regex, Invoke→Toggle→Value, fallback coords solo con error accionable — el flag `--force-coords` se deja fuera YAGNI: el error ya dice que pruebe coordenadas; no se añade flag para no inflar CLI). S3 fixes → Tasks 1-4+7 (scroll, wait, pid, crop/zoom, --focus). S4 errores → Tasks 3+4+6 (mensajes accionables, timeout vs error, crop validado). S5 testing/éxito → cada task con tests + Task 8 gates + manual. Cubierto.
2. **Placeholder scan:** sin TBD/TODO; cada step trae código, comando y expected. La única incertidumbre real (nombres exactos de tipos `uiautomation`) lleva instrucción explícita de resolver vía `docs.rs` + `cargo check`, no es placeholder.
3. **Type consistency:** `WindowInfo` siempre con `pid: Option<u32>` (Tasks 2, 3 lo usan). `MatchMode` definido una vez en Task 5 y reusado en 6+7 con la misma firma `match_name(&str,&str,&MatchMode)->bool`. `wait_for_window_with(title,exact,timeout,interval,list_fn)` usado igual en test y en impl. `crop_and_zoom(&DynamicImage,(u32,u32,u32,u32),u32)->DynamicImage` igual en test e impl. OK.
