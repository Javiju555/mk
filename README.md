# mk

CLI para automatizar escritura y teclas en interfaces gráficas de Linux.

Detecta automáticamente si estás en Wayland o X11 y selecciona el backend adecuado.

## Backends soportados

| Backend    | Wayland | X11 |
|------------|---------|-----|
| `wtype`    | ✅      | ✅  |
| `xdotool`  | ❌      | ✅  |
| `ydotool`  | ✅      | ❌  |

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

### Duraciones soportadas

- `Ns` — segundos (ej: `5s`)
- `Nm` — minutos (ej: `10m`)
- `Nh` — horas (ej: `2h`)

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

