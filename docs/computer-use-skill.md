# Computer-use with mk — practical skill notes

> Extracted from a live autonomous session on GNOME/Wayland (2026-07-07),
> per the user's goal: *"lo mejor para sacar las skills es que un agente itere
> sobre cómo se usa"*. These are the concrete, learned-the-hard-way rules for
> driving a desktop through `mk` as an AI agent.

## The core loop (non-negotiable discipline)

```
move mouse  →  screenshot  →  READ & validate cursor/target  →  click/adjust  →  screenshot
```

One interaction per step. Never chain blind clicks. Every mouse action is
followed by a screenshot you actually read before the next action. This is
slow on purpose — a wrong click can close the window that hosts your own
session.

## Coordinates

- `mk screenshot <path>` writes a PNG at the **native** resolution (here
  2880×1920). Click coordinates are **1:1** with that PNG — a target at pixel
  (x,y) in the screenshot is clicked with `mk click x y`. No scaling.
- Screenshots do **not** capture the cursor, so `mk move x y` can't be
  visually confirmed by position — confirm by clicking and observing the
  result instead, and keep targets well inside safe regions.
- Take the first screenshot before doing anything and re-check the reported
  image dimensions each session; don't assume last session's resolution.

## Gotchas that cost real attempts

1. **Scheduled `mk` typing lands in whatever window has focus.** A
   `mk in/at ... paste "..." && mk enter` types into the *currently focused*
   window when it fires — not into any particular app. If you switch focus
   away and it fires, the message is corrupted / lands in the wrong app.
   Rule: if a scheduled message is pending (or mid-typing), **do not change
   window focus**; let it complete first. (Observed: a note was being typed
   character-by-character into the agent's own input while navigating — any
   window switch would have split it.)

2. **GNOME overview click ≠ reliable window switch.** `mk key super` opens
   Activities, but clicking a window thumbnail is timing- and
   coordinate-fragile: the overview can close before the click registers, so
   the click lands on whatever is underneath (often the wrong app). Expect to
   retry and re-screenshot.

3. **`mk window list` is blind on GNOME/Wayland.** It enumerates via XCB and
   sees only XWayland surfaces — the real apps (browsers, IDEs, PWAs) are
   Wayland-native and invisible. Do **not** rely on it to find a window's
   coordinates on Wayland. (It *is* reliable on X11, Windows, macOS.) See
   `docs/window-control.md`.

4. **Never click these:** the GNOME-overview per-window close **✗** (top-right
   of the hovered window), and an app's own window-control buttons. In
   overview the ✗ appears over the *hovered/primary* window — verify which
   window it's on before clicking anywhere near a top-right corner.

## Working recipes (GNOME/Wayland)

- **See the whole desktop / find a window visually:** `mk key super` → wait
  ~700ms → `mk screenshot`. Read the thumbnails to locate the target app.
- **Switch focus (when overview-click is flaky):** `mk key alt+tab` (hold/
  repeat to cycle). More reliable than clicking overview thumbnails for the
  *previous* window; for a specific window, overview + careful click.
- **Type into a specific input:** first click the input to focus it
  (screenshot to confirm the caret/highlight), then `mk text "..."` or
  `mk paste "..."`. Confirm with a screenshot before pressing `mk enter`.
- **Schedule a wake-up into a specific session:** leave that session's input
  focused, then `mk at HH:MM paste "..." && mk enter` (or `mk in <dur> ...`).
  Because focus decides the destination, don't touch the GUI afterwards.

## What mk can and can't do here (summary)

| Want to…                    | GNOME/Wayland | How |
|-----------------------------|:-------------:|-----|
| Screenshot the screen       | ✅            | `mk screenshot` (portal/PipeWire) |
| Move/click/scroll/type      | ✅            | `mk move/click/scroll/text/paste` (daemon) |
| Cycle window focus          | ✅ (blind)    | `mk key alt+tab` / `super` + click |
| List windows + geometry     | ❌            | XCB sees only XWayland; use screenshots |
| Focus a specific window by id | ❌          | not a Wayland operation; use overview/alt-tab |
| Read UI element tree (a11y) | ❌ (stub)     | `accessibility` module is a stub today |

On Windows/macOS most of the ❌ rows become ✅ — see `docs/window-control.md`.
