# Changelog

## [0.5.0] - 2026-06-21

### Added
- **Soporte Nativo Multiplataforma (Windows y macOS):**
  - Implementación del backend nativo de Windows (`WindowsBackend`) utilizando la API Win32 `SendInput` y `SetCursorPos` de forma directa y ultra ligera.
  - Implementación del backend nativo de macOS (`MacosBackend`) utilizando las APIs del framework Core Graphics (`CGEvent`) para inyectar eventos de entrada nativos de sistema de manera segura.
  - Detección compile-time y conditional compilation para empaquetar únicamente las dependencias de cada plataforma (`windows-sys` en Windows, `core-graphics` y `foreign-types` en macOS).
  - Portabilidad completa para los comandos de escritura (`text`), teclado (`key`), ratón (`move`, `click`, `drag`, `mouse-down`, `mouse-up`, `scroll`) y capturas de pantalla (`screenshot`) en los tres sistemas operativos principales.
- **Soporte de duraciones compuestas:**
  - El analizador de duraciones ahora acepta formatos compuestos y espaciados como `"1h 53m"`, `"1h53m"` o `"2h 30m 10s"`, sumando automáticamente sus componentes para mayor comodidad.

## [0.4.0] - 2026-06-21

### Added
- **Soporte de Ratón Virtual (uinput):**
  - Implementación de ejes absolutos (rango `0..32767`) y botones (`left`, `right`, `middle`) para simular interacciones táctiles y de puntero absolutas.
  - Soporte de scroll vertical y horizontal en el daemon (`SCROLL`).
  - Movimiento progresivo/suave de cursor interpolado en el tiempo (`MOVE_SMOOTH`).
- **Nuevos comandos de CLI y Scripting:**
  - `move <x> <y> [-d <duración>]`: Desplazar el cursor.
  - `click <x> <y> [-b <botón>] [-d <duración>]`: Clic de ratón.
  - `drag <x1> <y1> <x2> <y2> [-d <duración>]`: Arrastrar.
  - `mouse-down [<botón>]` y `mouse-up [<botón>]`: Presionar/soltar botones de ratón individualmente.
  - `scroll <clicks> [-h]`: Scroll de rueda de ratón (vertical/horizontal).
  - `screenshot <ruta>`: Captura de pantalla multiplataforma nativa.
- **Resolución dinámica:**
  - Detección automática de resolución de pantalla activa mediante `xcap` y escalado dinámico a las coordenadas absolutas de uinput.
- **Variables dinámicas en scripting:**
  - Los comandos del ratón guardan sus campos como textos para permitir la expansión tardía de variables en runtime (con `set` y `exec`).
