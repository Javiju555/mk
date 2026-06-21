# mk

CLI multiplataforma para automatizar escritura, pulsaciones de teclas, simulaciones de ratón y capturas de pantalla en Linux, Windows y macOS.

## Sistemas Operativos y Backends soportados

- **Linux**: Detecta automáticamente si estás en Wayland o X11 para seleccionar el backend adecuado (`wtype`, `xdotool`, `ydotool` o el daemon virtual uinput `mk-daemon`).
- **Windows**: Automatización nativa utilizando la API Win32 (`SendInput` y `SetCursorPos`). No requiere dependencias adicionales ni daemons externos.
- **macOS**: Automatización nativa utilizando el framework Core Graphics (`CGEvent`). No requiere dependencias adicionales ni daemons externos (se requieren permisos de Accesibilidad en Preferencias del Sistema).

| Backend / SO | Linux (Wayland) | Linux (X11) | Windows | macOS |
|--------------|-----------------|-------------|---------|-------|
| `wtype`      | ✅              | ✅          | ❌      | ❌    |
| `xdotool`    | ❌              | ✅          | ❌      | ❌    |
| `ydotool`    | ✅              | ❌          | ❌      | ❌    |
| `mk-daemon`  | ✅ (uinput)     | ✅ (uinput) | ❌      | ❌    |
| `Win32 API`  | ❌              | ❌          | ✅      | ❌    |
| `CoreGraph`  | ❌              | ❌          | ❌      | ✅    |

## Instalación

```bash
cargo install --path .
```

Los backends deben estar instalados en el sistema:

```bash
# Arch Linux
sudo pacman -S wtype xdotool

# Ubuntu/Debian
sudo apt install wtype xdotool
```

## Uso

### Escribir texto

```bash
mk text "Hola mundo"
```

### Presionar Enter

```bash
mk enter
```

### Combinaciones de teclas

```bash
mk key "ctrl+s"
mk key "ctrl+c"
mk key "alt+tab"
mk key "ctrl+alt+Delete"
```

### Esperar un tiempo

```bash
mk wait "5s"
mk wait "10m"
mk wait "2h"
```

### Pegar texto (portapapeles + Ctrl+V)

```bash
mk paste "texto copiado"
```

En Wayland usa `wl-copy`, en X11 usa `xclip` o `xsel`. Si no hay herramienta de portapapeles, usa `type_text` como fallback.

### Simulación de ratón

- **En Linux**: Se requiere que `mk-daemon` esté activo para abrir el dispositivo táctil absoluto en `/dev/uinput` y poder interactuar de forma segura en Wayland y X11.
- **En Windows y macOS**: Las acciones de ratón funcionan directamente de forma nativa e instantánea sin necesidad de ejecutar ningún daemon ni requerir permisos adicionales más allá de Accesibilidad (en macOS).

```bash
# Mover cursor (de forma progresiva en 500ms o instantánea si se omite la duración)
mk move 500 500 --duration "500ms"

# Hacer clic (botón izquierdo por defecto, o custom con -b)
mk click 500 500 --button right --duration "200ms"

# Arrastrar (presiona botón izquierdo, se desplaza y luego lo suelta)
mk drag 100 100 800 800 --duration "1s"

# Presionar y soltar botones de forma persistente
mk mouse-down left
mk mouse-up left

# Hacer scroll (clicks positivos hacia arriba/derecha, negativos hacia abajo/izquierda)
mk scroll 3
mk scroll --horizontal -- -2
```

Las coordenadas de pantalla `(X, Y)` en píxeles se detectan de forma automática respecto a tu monitor principal y se escalan transparentemente a la tableta absoluta.

### Captura de pantalla (Screenshots)

Toma una captura de pantalla del monitor primario y la guarda en la ruta indicada de forma nativa:

```bash
mk screenshot ruta/de/mi_imagen.png
```

### Ejecutar un script

```bash
mk run mi_script.mk
```

### Dry-run (sin ejecutar)

```bash
mk --dry-run run mi_script.mk
mk --dry-run text "prueba"
```

### Logging

```bash
mk --log acciones.log run mi_script.mk
```

Cada acción registra timestamp, nombre y resultado.

### Diagnóstico del sistema

```bash
mk doctor
```

Muestra sesión, backends disponibles, herramientas de portapapeles y recomendaciones.

## Formato de scripts

Los scripts `.mk` son archivos de texto con un comando por línea:

```bash
# Comentarios con #
text "Hola mundo"
wait "1s"
key "ctrl+a"
text "texto seleccionado"
enter
wait "500ms"
key "ctrl+s"
```

### Comandos disponibles

| Comando      | Descripción                         | Ejemplo                          |
|--------------|-------------------------------------|----------------------------------|
| `text`       | Escribir texto                      | `text "Hola"`                    |
| `enter`      | Presionar Enter                     | `enter`                          |
| `key`        | Presionar tecla                     | `key "ctrl+s"`                   |
| `wait`       | Esperar                             | `wait "5s"`                      |
| `paste`      | Copiar y pegar via portapapeles     | `paste "texto"`                  |
| `paste-file` | Pegar archivo formateado (Markdown) | `paste-file "src/main.rs"`       |
| `paste-dir`  | Pegar dir de código recursivo       | `paste-dir "src"`                |
| `exec`       | Ejecutar comando y guardar en var   | `exec var "cargo test"`          |
| `set`        | Definir variable                    | `set nombre "Claude"`            |
| `repeat`     | Repetir bloque N veces              | `repeat 3 { ... }`               |
| `include`    | Incluir otro archivo                | `include "common.mk"`            |
| `move`       | Mover cursor (opcional duración)    | `move 500 500 "500ms"`           |
| `click`      | Clic en coordenadas                 | `click 500 500 "left" "200ms"`   |
| `drag`       | Arrastre de cursor                  | `drag 10 10 100 100 "1s"`        |
| `mouse-down` | Presionar botón                     | `mouse-down "left"`              |
| `mouse-up`   | Soltar botón                        | `mouse-up "left"`                |
| `scroll`     | Desplazar rueda del ratón           | `scroll -3 "false"`              |
| `screenshot` | Captura de pantalla                 | `screenshot "foto.png"`          |

### Duraciones soportadas

Se admiten duraciones simples y compuestas (separadas opcionalmente por espacios):
- `Nms` — milisegundos (ej: `250ms`)
- `Ns` — segundos (ej: `5s`)
- `Nm` — minutos (ej: `10m`)
- `Nh` — horas (ej: `2h`)

Ejemplos de duraciones compuestas válidas:
- `"1h 53m"`
- `"1h53m"`
- `"2h 30m 10s 500ms"`

### Variables

```bash
set name "Claude"
set lang "es"
text "Hola ${name}"
paste "Mi nombre es ${name}"
```

### Bloques repeat

```bash
repeat 3 {
    text "hola"
    enter
    wait "1s"
}
```

Soporta anidación:

```bash
repeat 2 {
    repeat 3 {
        text "x"
    }
}
```

### Include

`common.mk`:
```bash
text "común"
enter
```

`main.mk`:
```bash
text "antes"
include "common.mk"
text "después"
```

Los paths son relativos al archivo que contiene el `include`.

## Ejemplo completo

```bash
# Abrir una terminal, escribir un comando y ejecutarlo
mk run abrir_terminal.mk
```

`abrir_terminal.mk`:
```bash
key "ctrl+alt+t"
wait "1s"
text "ls -la"
enter
wait "500ms"
text "exit"
enter
```

Script con variables y repeat:
```bash
set user "admin"
repeat 3 {
    text "login: ${user}"
    enter
    wait "500ms"
}
key "ctrl+d"
```

## Documentación y Manuales

Para un tutorial detallado con explicaciones paso a paso de cada función, la configuración segura del daemon y ejemplos prácticos para interactuar con LLMs (como Claude o Gemini) en español, consulta el archivo [TUTORIAL.md](file:///home/javiju/proyectos/mk/TUTORIAL.md).

