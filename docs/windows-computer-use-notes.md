# Computer-use with mk on Windows — practical notes from a live session

> Update 2026-09-10: gotchas 2 (scroll), 4 (crop/zoom), 5 (wait), 7 (pid) resueltos; UIA (`mk ui`) implementado — ver spec.

> Extracted from a live session (2026-07-20) driving a real Windows desktop
> app: a Tauri/WebView2 GUI (FenixMixer) with no accessibility-tree tooling
> available up front. Companion to `docs/computer-use-skill.md` (GNOME/
> Wayland) and `docs/window-control.md` (architecture) — this doc is the
> Windows-specific field report those anticipated but didn't have yet.

## TL;DR

On Windows, `mk window list` + `mk move`/`click`/`scroll` are internally
consistent and worked correctly once the window's on-screen offset was
accounted for (no HiDPI scaling bug reproduced on this machine, unlike the
1.667× GNOME case in `computer-use-skill.md`). The real friction wasn't
coordinates-vs-screenshot — it was **coordinates vs. focus, and coordinates
from a second, independent tool** (Windows UI Automation, used directly via
PowerShell because mk's `accessibility` module is still a stub). Name-based
UIA `Invoke()` clicks succeeded every single time; raw coordinate clicks
failed repeatedly for reasons that had nothing to do with mk's own math. This
is the strongest field evidence yet for prioritizing `accessibility` (Phase 5
in `window-control.md`) on Windows specifically — see the dedicated section
below.

## What worked cleanly

- **`mk window list` → geometry → `mk move`/`click`/`scroll` at
  `window.x + local_x, window.y + local_y`**: correct and reliable, once I
  remembered the window is *not* at (0,0). `mk window focus <id>` immediately
  before the action (same shell invocation) made it land every time.
- **`mk scroll <clicks>` at the right absolute position**: precise enough to
  binary-search for a specific panel in a scrollable region (tried `-8`, then
  `-3`, then `+4` to bracket a target row). No HiDPI correction needed here.
- **`mk screenshot -w <window_id> --raw`**: captured exactly the target
  window's client area, cropped correctly to its reported geometry. `--raw`
  (full-res PNG) was worth it over the default JPEG for reading small text/
  knob values.

## Gotchas that cost real attempts

1. **Raw input (`move`/`click`/`scroll`/`key`) needs true OS foreground
   focus; splitting `focus` and the action across separate tool
   invocations is not safe.** Two coordinate/SendKeys attempts landed in my
   *own* terminal window instead of the target app — not because the
   coordinates were wrong, but because focus had reverted to the calling
   context between invocations (each shell call is a fresh process; nothing
   guarantees the previous `SetForegroundWindow` "stuck"). Fix: always issue
   `mk window focus <id>` and the action that depends on it in the *same*
   invocation, back-to-back, with minimal delay. mk already supports this
   (`mk window focus` is a real subcommand); the gotcha is a discipline rule
   for the caller, not an mk bug — worth stating explicitly in the CLI help
   or a top-level doc so agents don't rediscover it the hard way.

2. **`mk scroll -6` fails to parse** (`error: unexpected argument '-6'
   found`) — clap reads the leading `-` as a flag. Workaround:
   `mk scroll -- -6`. This is a common clap footgun for a signed positional
   argument; consider `#[arg(allow_hyphen_values = true)]` on `CLICKS` so
   `mk scroll -6` works without the `--` escape hatch. Small fix, real
   friction removed (I hit this on the very first scroll attempt).

3. **Absolute pixels are the core scaling problem on a multi-monitor
   desktop.** I read a UIA `GetClickablePoint()` of `x=3872` for a window mk
   reported at `x=1139,width=1162`, and initially filed it as an unexplained
   coordinate-space mismatch. **The machine owner identified it immediately:
   it's multi-monitor.** Their third display starts at x≈3441, so `3872` was
   a perfectly correct absolute coordinate — the window had simply been moved
   there, while mk's cached geometry described an earlier position on
   monitor 1. Nothing was wrong with either tool's math.
   The real lesson is the one that motivates the next section: on a 3-monitor
   desktop, **absolute pixels are a poor targeting primitive**. To click a
   button you must first discover which monitor the window is on, capture
   that monitor (or all of them), and locate the control visually — a
   multi-step guess before every interaction. Practical rules meanwhile:
   re-read `mk window list` immediately before acting (never reuse geometry
   across steps — windows move), and never mix coordinates from two
   different tools/APIs in one action.

4. **No built-in way to zoom/crop a screenshot to read a small detail.**
   A UI detail (a ~2px routing-cable indicator dot) was invisible at normal
   screenshot resolution; I had to pipe the PNG through an external Python +
   Pillow script to crop a region and 2× it before it was legible. A native
   `mk screenshot --crop x,y,w,h` and/or `--zoom <factor>` (or a separate
   `mk vision crop <in.png> <out.png> --region ...`) would remove an external
   dependency for what is probably a very common verification need — "did
   that small icon/state actually change" — for any agent driving a GUI.

5. **No polling primitive for "wait until this window exists."** After
   launching the app's binary directly (`Start-Process`), I fell back to a
   fixed `Start-Sleep` guess before the first `mk window list`/`screenshot`,
   which is exactly the fragile pattern this project's own docs warn against
   elsewhere. A `mk window wait --title <substring> --timeout <dur>`
   (poll `window list` internally) would replace sleep-and-hope with a real
   readiness check — same value as `condition-based-waiting` patterns used
   elsewhere in this toolchain.

6. **`mk screenshot --cursor` didn't visibly draw the crosshair** in one
   capture where the cursor should have been within the captured window's
   bounds (moved there immediately before, via `mk move`). Not confirmed as
   a bug — could be timing (cursor visually updates after the move call
   returns, screenshot raced it) or the crosshair color blending with a dark
   theme. Worth an independent repro on a plain light background before
   treating this as a real defect.

7. **Window disambiguation by `app_name` substring is fragile if you
   relaunch the same app.** I relaunched the target app's process twice in
   one session and got two different window ids (`329272`, then `1049278`)
   from `mk window list`, and had to re-grep by `app_name` each time. That
   worked here because only one instance was ever running at once, but there
   was no way to correlate a specific `Start-Process`-returned PID to a
   `mk window list` entry directly. If the JSON doesn't already include a
   `pid` field, adding one would let callers disambiguate deterministically
   instead of by title/name substring matching.

## The accessibility module is the highest-leverage next investment (Windows)

> **This is the machine owner's explicit ask**, stated after watching this
> session: *"me refería más a cosas como [the UIA snippet below], que suenan
> muy precisas de 'tocar este botón en concreto' y quiero que mk consiga,
> porque si no tienes que tirar screenshot de las 3 pantallas o usar otras
> cosas para saber dónde está y adivinar px absolutos, para exprimir al
> máximo las herramientas de automatización que win32 da."*
>
> The concrete shape to replace — this worked first try, every time, with no
> coordinates and no knowledge of which monitor the window was on:
>
> ```powershell
> Add-Type -AssemblyName UIAutomationClient
> Add-Type -AssemblyName UIAutomationTypes
> $p    = Get-Process fenix-mixer
> $root = [System.Windows.Automation.AutomationElement]::FromHandle($p.MainWindowHandle)
> $cond = New-Object System.Windows.Automation.PropertyCondition(
>           [System.Windows.Automation.AutomationElement]::NameProperty, "Mezclador")
> $el   = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $cond)
> $el.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
> ```
>
> Desired mk equivalent, roughly:
> `mk ui click --window <id> --name "Mezclador"` (and `--role`, plus
> `mk ui tree --window <id>` to enumerate). Note `Invoke()` does **not**
> require the window to be focused or even frontmost, which sidesteps
> gotcha 1 (focus) and gotcha 3 (multi-monitor absolute pixels) at once.

### Design caveats learned the hard way (for whoever implements this)

- **Match exactly by default, substring only on request.** I searched the
  tree with a substring/regex match for `"DELAY"` to check whether an effect
  panel had appeared, and got a confident false positive: the match landed on
  a *help-text* node ("Delay: relativamente sencillo, muy testeable…") that
  merely contained the word, not the panel title. A substring search over a
  whole UI tree will happily hit tooltips, help panes and docs text. So:
  `--name` should be exact-match by default, with an explicit `--contains` /
  `--regex` opt-in, and `mk ui tree` output should include enough context
  (role, parent, bounds) to tell a title apart from a paragraph.
- **Report *visibility*, not just presence.** In a WebView2 app, panels
  hidden with `display:none` do drop out of the tree, but the tree still
  contains inactive tab content and off-screen nodes. `UiElement` would be
  much more useful with an `is_offscreen` / `is_enabled` flag
  (UIA exposes `IsOffscreen` and `IsEnabled` directly) so a caller can tell
  "exists but not shown" from "actually on screen".
- **Prefer the control pattern over a synthetic click.** `InvokePattern`,
  `TogglePattern` and `ValuePattern` act on the control directly; falling
  back to `GetClickablePoint()` + a real mouse click reintroduces every
  coordinate/focus problem this feature exists to avoid. Expose the patterns
  first and treat coordinate clicking as the last resort.
- **WebView2/Chromium apps do expose a usable UIA tree** (this app: ~240
  elements, tab buttons invokable by name) — so this approach is not limited
  to native Win32 widgets, which matters given how many desktop apps are
  Electron/Tauri/WebView shells today.

`src/accessibility/mod.rs` is currently a real stub — `get_ui_tree()` returns
`Ok(Vec::new())` unconditionally, and `find_button`/`find_input` are ready
but structurally unreachable until it's backed by a real OS API. This
session is a concrete data point for finishing it on Windows first:

- Every coordinate-based interaction attempt (raw `mouse_event`/`SendKeys`,
  and even careful `mk move`+`scroll`) required correct window geometry,
  correct focus, and — critically — **the target had to be scrolled into
  view first**, adding a whole extra "find it" loop (see gotchas 1–3 above).
- Every name-based interaction — done via raw PowerShell
  `AutomationElement.FindFirst(..., NameProperty == "Efectos")` +
  `InvokePattern.Invoke()` — worked on the **first try**, needed no
  coordinates, no focus (UIA can invoke controls in a background window),
  and no DPI reasoning at all. It also let me *search the whole UI tree for
  a name* to confirm whether an element existed at all before trying to
  interact with it — the single most useful debugging step in the whole
  session (it caught a real "this panel never renders" bug independent of
  any click).
- Windows has a first-class, stable, already-idiomatic-in-Rust path for
  this: the `uiautomation` crate (wraps `IUIAutomation`/UIA COM interfaces)
  or raw `windows-rs` bindings to the same. `UiElement{role,name,x,y,width,
  height}` already matches what UIA exposes almost 1:1 (`ControlType` →
  `role`, `Name` → `name`, `BoundingRectangle` → `x/y/width/height`), and
  `GetClickablePoint()` + `InvokePattern`/`TogglePattern`/`ValuePattern`
  cover click/toggle/set-value without any coordinate math or focus
  handling — the caller gets `find_button("Efectos")` → click, or
  `find_input("Preset name")` → set value, exactly as the stub's signature
  already promises.

Recommendation: implement the Windows backend of `accessibility` next (UIA
via `uiautomation` or `windows-rs`), ahead of investing further in
coordinate-precision tooling for Windows — the coordinate path already works
well enough (see "What worked cleanly"), while the *lack* of a tree-search/
name-click path was the actual bottleneck this session, repeatedly.

## Working recipe (Windows, 0.7.0+ — UIA first, coordinates as fallback)

```
mk window list                                             # find target window id (+ pid) + geometry
mk window wait --title "<app>" --timeout 10s               # readiness check instead of sleep-and-hope
mk ui tree --window <id>                                   # enumerate controls, exact names
mk ui click --window <id> --name "Mezclador"               # no focus / no coordinates needed
mk screenshot -w <id> --raw --crop 100,200,800,600 --zoom 2 # verify small details, read before next action
```

Coordinate fallback (same-invocation focus or `--focus`):

```
mk move <win.x + local_x> <win.y + local_y> --focus <id>   # absolute screen coords
mk scroll -6 --focus <id>                                  # negative = down; no `--` needed since 0.7.0
mk screenshot -w <id> --raw                                # verify, read it before the next action
```

Repeat move→scroll→screenshot with small click counts to bracket a target
inside a scrollable panel — this reliably found a specific row without
needing pixel-perfect target coordinates up front. Prefer the UIA path
whenever the control has a name: it sidesteps focus (gotcha 1) and
multi-monitor absolute pixels (gotcha 3) at once.
