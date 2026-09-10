# Diseño: mejoras computer-use Windows (UIA completo + fixes) — 2026-09-10

> Estado: diseño aprobado por usuario (2026-09-10). Enfoque elegido: A (`uiautomation` crate).
> Base: `origin/master` @ `4435777` (validado con `fetch + pull --rebase`; docs locales WIP preservados).
> Fuentes: `docs/windows-computer-use-notes.md` (gotchas 1-7 + ask explícito UIA), `docs/window-control.md` Phase 5, `docs/computer-use-skill.md`.

## 1. Objetivo y no-objetivos

**Objetivo (este ciclo):** que un agente de IA pueda automatizar una app Windows real sin adivinar píxeles absolutos ni pelearse con foco/monitores:
- `mk ui tree/click/toggle/set-value` por nombre vía UI Automation (exact-match por defecto).
- 4 fixes que ya costaron intentos reales: `scroll -6` sin `--`, `window wait`, `pid` en `window list`, `screenshot --crop/--zoom`.
- Disciplina de foco documentada + flag `--focus <id>` in-process para `click/move/scroll`.

**No-objetivos (ciclos futuros):** daemon Linux robusto, macOS AX, compositor backends (hyprctl/swaymsg), AT-SPI Linux, `focus-interactive` Wayland. Se dejan fuera a propósito (YAGNI / descomposición).

**Éxito:** `cargo test` verde + `cargo check --target x86_64-pc-windows-gnu` y `--target x86_64-apple-darwin` sin warnings (no repetir `4435777`) + prueba manual en Windows real (`ui tree/click`, `scroll -6`, `wait`, `crop`) por el usuario.

## 2. Arquitectura

```
mk ui ...
  └─ src/accessibility/mod.rs (fachada estable, API pública no rompe)
       ├─ cfg(windows): src/accessibility/windows_uia.rs (nuevo, dep `uiautomation`)
       └─ not(windows): stub actual (Ok(vec![])) — sin cambios
mk window {list,wait,...}
  └─ src/windows/mod.rs (+ pid, + wait helper)
mk screenshot ...
  └─ src/vision/mod.rs (+ crop/zoom puro con `image`, cross-platform)
CLI (src/main.rs, clap)
  └─ nuevo enum UiAction + flags en Scroll/Screenshot/Click/Move
```

Reglas duras:
- Todo lo UIA/COM vive tras `#[cfg(target_os = "windows")]`. Ningún `use uiautomation` fuera de ese gate. La dependencia va en `[target.'cfg(target_os = \"windows\")'.dependencies]` junto a `windows-sys`.
- `UiElement` se extiende de forma compatible: se añaden `is_enabled: bool`, `is_offscreen: bool` (con `#[serde(default)]` para no romper JSON viejo). `role/name/x/y/width/height` no cambian de significado.
- Límite de seguridad: tree walk con `TreeScope::Descendants`, cap ~5000 nodos + timeout interno, para no colgar en apps WebView2 grandes (~240 nodos en FenixMixer, pero Electron puede ser miles).

## 3. Componentes

### 3.1 `src/accessibility/windows_uia.rs` (nuevo, solo Windows)
- Dependencias nuevas (solo Windows + una cross-platform): `uiautomation` (estable reciente, versión exacta a fijar en `Cargo.toml` durante la implementación) y `regex` (para `--regex`; cross-platform, sin impacto OS).
- `UiaBackend`: un `uiautomation::UIAutomation` por proceso (`OnceLock` o creado por llamada; `uiautomation` gestiona COM).
- `ui_tree_for_window(hwnd: usize) -> Vec<UiElement>`: `FromHandle` → `get_root` → walk descendientes; por nodo lee `Name`, `LocalizedControlType` → `role`, `BoundingRectangle` → `x/y/w/h`, `IsEnabled`, `IsOffscreen`. Nodos sin nombre se conservan (name="") porque sirven como contexto padre.
- `find_element(window_id, name, mode: Exact|Contains|Regex) -> Option<UIElement>`: **Exact por defecto = comparación tras `trim()` + case-insensitive** (el agente ve el nombre exacto en `tree`, pero no se le castiga por mayúsculas). Contains = substring case-insensitive; Regex = `regex::Regex` case-sensitive salvo `(?i)`. Devuelve también `role/bounds` para desambiguar.
- `invoke/toggle/set_value(element)`: usa `get_pattern::<Invoke/Toggle/Value>`; si el patrón no existe → error accionable (`bail!(\"control 'X' no expone InvokePattern (role=Y). Prueba `mk ui toggle` / coordenadas\")`). Coordenadas (`GetClickablePoint` + `mk click`) solo como fallback explícito `--force-coords`, con warning de que reintroduce foco/DPI.
- No requiere foco ni primer plano (propiedad clave UIA que soluciona gotchas 1 y 3).

### 3.2 CLI `mk ui` (src/main.rs)
```
mk ui tree --window <id> [--contains SUB | --regex RE] [--role ROLE]
mk ui click --window <id> --name "Mezclador" [--contains | --regex RE]
mk ui toggle --window <id> --name "..." [...]
mk ui set-value --window <id> --name "..." --value "..." [...]
```
- Salida `tree` = JSON pretty array `UiElement` (máquina-consumible, como `window list`).
- `click/toggle/set-value` imprimen JSON del elemento actuado + patrón usado.
- Parser `.mk`: se añaden `ui-click/ui-toggle`? NO en este ciclo (YAGNI) — solo CLI directo; scripts pueden usar `exec`.

### 3.3 Fixes pequeños
1. **Scroll negativos:** `#[arg(allow_hyphen_values = true)]` en `Scroll::clicks` (CLI + ScheduledAction). Mantiene `--horizontal`. Test: `try_parse_from([\"mk\",\"scroll\",\"-6\"])` ok.
2. **Window wait:** `mk window wait --title SUB [--exact] [--timeout 10s] [--interval 400ms]` → poll `list_windows()` cada `interval` hasta match o `timeout`; imprime JSON del match; exit 0 si encuentra, exit 1 + `Timeout esperando ventana...` si no. Reemplaza `Start-Sleep` adivinatorio.
3. **pid en list:** `WindowInfo { pid: Option<u32> }` con `#[serde(default, skip_serializing_if = \"Option::is_none\")]`. Windows: `GetWindowThreadProcessId(HWND)` vía `windows-sys` (añadir feature `Win32_System_Threading` ya existe). Linux/mac: `None` (no inventar). Permite correlacionar `Start-Process -PassThru` PID ↔ window.
4. **Screenshot crop/zoom:** flags `--crop x,y,w,h --zoom 2` en `mk screenshot` (monitor y window). Implementación en `vision::crop_and_zoom(img, rect, zoom)` con `image::imageops::{crop_imm, resize(Nearest)}`; valida rect dentro de imagen (`bail!` si fuera). Cross-platform, sin deps nuevas.
5. **Foco:** flag `--focus <id>` en `move/click/scroll` que llama `windows::focus_window(id)` in-process justo antes de actuar (solución real a gotcha 1, no solo docs) + actualizar `long_about`/help con receta: `window list → window focus (mismo proceso vía --focus) → actuar → screenshot -w --raw`.

## 4. Flujo de datos

```
Agente: window list → (opcional) window wait --title → ui tree --window ID → ui click --name (Invoke, sin foco) → screenshot -w ID --raw [--crop/--zoom] → leer → siguiente
Fallback coordenado: window list (fresco, nunca cacheado) → click --focus ID x y → screenshot → ajustar
```

## 5. Manejo de errores (honesto, accionable)

- UIA: stdout solo JSON; errores a stderr + exit≠0. Mensajes dicen qué patrón falta y qué probar.
- `wait`: distingue `timeout` (exit 1, mensaje con título buscado + ventanas vistas) de `error de enumeración`.
- `crop`: `bail!(\"crop {rect} fuera de imagen {w}x{h}\")`.
- `pid`: si `GetWindowThreadProcessId` falla → `None`, no error.
- Wayland: `focus/wait` por id siguen devolviendo el error honesto existente (no se promete lo imposible).

## 6. Testing

- Units (corren en todas las plataformas): matching Exact/Contains/Regex (función pura, testeable sin UIA); `crop_and_zoom` con imagen sintética 100×100; parser `--crop "10,10,50,50"`; `wait` con `list_fn` mock y timeout 50ms; `try_parse scroll -6`.
- Integración Windows (manual, criterio de éxito): `ui tree` en app nativa + WebView2; `ui click --name` sin foco; `scroll -6`; `window wait`; `screenshot --crop/--zoom`; `click --focus`.
- Gates: `cargo test` (58 tests actuales deben seguir verdes), `cargo check --target x86_64-pc-windows-gnu`, `cargo check --target x86_64-apple-darwin`, cero warnings.
- Docs: actualizar `windows-computer-use-notes.md` (marcar gotchas resueltos), `README.md` (tabla `mk ui`), `CHANGELOG.md` (0.7.0 Unreleased), y este spec.

## 7. Riesgos y decisiones

- `uiautomation` vs `windows-rs` crudo: se elige `uiautomation` (menos unsafe, más rápido). Riesgo: nueva dependencia COM; mitigación: gate estricto + checks cruzados.
- Exact-match por defecto: evita el falso positivo `DELAY` documentado (help-text vs panel). Coste: el agente debe deletrear bien; mitigación: `tree` muestra nombres exactos + flags opt-in.
- `is_offscreen/is_enabled`: UIA los expone gratis; sin ellos `tree` miente en tabs/app inactivas.
- No se toca el daemon Linux ni macOS en este ciclo (descomposición explícita pedida por skill).
