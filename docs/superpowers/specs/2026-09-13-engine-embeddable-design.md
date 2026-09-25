# Diseño: `mk::engine` — superficie embebible para motores de macros — 2026-09-13

> Estado: aprobado por el usuario (2026-09-13). Objetivo declarado: que **otro
> binario** (el "deck" del usuario, hardware propio) use mk como motor de
> macros. mk NO implementa el dispositivo ni el transporte.

## 1. Objetivo y no-objetivos

**Objetivo:** que `mk` (que ya es `[lib]`) tenga una superficie pública, estable
y *runtime-neutra* para que código Rust de terceros:
- ejecute macros concurrentemente,
- cancele una en curso,
- reciba eventos estructurados en streaming (p. ej. para pintar teclas),
- controle la política de errores y el deadline.

**No-objetivos:** implementar USB/HID, el dispositivo, la app del deck, ni
cualquier transporte. Tampoco引入 runtime async (decisión: hilos + canales).

## 2. Decisiones clave (y por qué)

| Decisión | Motivo |
|---|---|
| **Hilos + canales, no async/tokio** | Las acciones de mk son syscalls bloqueantes: `async` no las acelera. Añadir tokio a una lib sin runtime impone runtime y ralentiza la compilación del CLI. Un embedder async lo envuelve con `spawn_blocking` o un puente canal→stream. |
| **Un solo ejecutor** | El bucle de pasos se mueve a `engine::runner`; `Interpreter` (CLI) pasa a ser un adaptador sobre él. Sin dos implementaciones que divergan. Los 74 tests existentes son la red. |
| **Cancelación/desadline por paso, `wait` troceado** | Un "suelta el botón" no debe esperar a que acabe un `wait "3s"`: los sleeps se trocean en franjas de 50 ms comprobando cancel/deadline. |
| **`ErrorPolicy` explícita** | Hoy un fallo aborta la macro entera (`?`). El engine ofrece `Continue` (registra y sigue) y `Stop` (el CLI conserva abortar). |
| **Backend `Send + Sync`, compartido por `Arc`** | Varias macros concurrentes sobre el mismo backend, sin estado compartido mutable. Los backends actuales son unit structs, así que no hay fricción real. |
| **Sin `async`, sin deps nuevas** | Cero regresión de compilación para el CLI y para consumidores. |

## 3. Superficie pública (`mk::engine`)

```rust
pub struct Engine;                                   // Clone; sostiene Arc<dyn Backend>
pub struct Program { /* steps */ }                    // Clone + Debug

pub enum ErrorPolicy { Stop, Continue }
pub struct RunOptions { pub policy: ErrorPolicy, pub deadline: Option<Duration> }

pub enum Outcome { Ok, Failed(String), Skipped(String) }
pub struct Event { pub step: usize, pub action: String, pub detail: String, pub outcome: Outcome }
pub struct RunReport { pub steps: usize, pub failed: usize, pub cancelled: bool, pub duration: Duration }

pub trait EventSink: Send { fn emit(&mut self, ev: Event); }   // blanket impl para FnMut(Event)
pub struct MacroHandle { /* cancel + join */ }

impl Engine {
    pub fn new(backend: Arc<dyn Backend>) -> Self;
    pub fn parse(&self, src: &str) -> Result<Program>;
    pub fn program(&self, cmds: Vec<Command>) -> Program;
    pub fn run_blocking(&self, p: &Program, sink: &mut dyn EventSink, opts: &RunOptions) -> Result<RunReport>;
    pub fn spawn(&self, p: Program, sink: Arc<Mutex<dyn EventSink>>, opts: RunOptions) -> MacroHandle;
    pub fn spawn_channel(&self, p: Program, opts: RunOptions) -> (MacroHandle, Receiver<Event>);
}
impl MacroHandle {
    pub fn cancel(&self);
    pub fn is_cancelled(&self) -> bool;
    pub fn wait(self) -> Result<RunReport>;
}
```

## 4. Componentes

- `src/engine/mod.rs`: tipos públicos, `Engine`, `backends` (detectores por SO).
- `src/engine/runner.rs`: el bucle de pasos (única implementación). Maneja
  `Set/Repeat/In/At/Wait/...`, vars por macro, cancel, deadline, política.
- `src/parser.rs`: `Interpreter` se reduce a adaptador (sink → `Logger`/stdout,
  `ErrorPolicy::Stop`, y devuelve `Err` con el primer fallo, preservando el CLI).
- `src/input/mod.rs`: `InputBackend: Send + Sync` (supertraits, sin romper backends).
- `examples/embed.rs`: embedder de demostración con backend falso.

## 5. Verificación (sin GUI)

- `examples/embed.rs` lanza 3 macros concurrentes, cancela una a mitad, y
  comprueba (con backend falso, cero inyección real) que las otras dos acaban
  bien y la cancelada reporta `cancelled: true`.
- Tests: `Continue` vs `Stop`; deadline agota; cancelación durante `wait` largo
  (reactiva <200ms); aislamiento de variables entre macros; evento emitido antes
  de completar la macro; `parse` usa el parser existente sin cambios.
- Gates: `cargo test` + `cargo check` host/gnu/darwin sin warnings; suite actual
  intacta tras el refactor de `Interpreter`.

## 6. Riesgos

- **Refactor de `Interpreter`**: toca el camino CLI/scripts ya usado. Mitigación:
  es el último task, la suite completa como red, y commit propio para revertir.
- **Supertraits `Send + Sync`**: algún backend podría no cumplir. Hoy todos son
  unit structs; si compilara falla, se revisa ese backend, no la API.
- **Cancelación no interrumpe un paso en vuelo** (p. ej. un UIA colgado): se
  documenta; el deadline es la red de seguridad.
