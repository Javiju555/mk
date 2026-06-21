# Manual de Usuario y Tutorial de `mk`

`mk` es una potente herramienta de automatización para simular entradas de teclado y atajos en interfaces gráficas de Linux (tanto bajo Wayland como X11). Está diseñada especialmente para simplificar el flujo de trabajo con Modelos de Lenguaje (LLMs) como Claude o Gemini, permitiéndote inyectar contexto de archivos y comandos de consola de forma directa y automática.

---

## Índice
1. [El Daemon (`mk-daemon`) y Seguridad](#1-el-daemon-mk-daemon-y-seguridad)
2. [Uso Directo desde la Terminal (Comandos Simples)](#2-uso-directo-desde-la-terminal-comandos-simples)
3. [Sintaxis de Scripts (`.mk`)](#3-sintaxis-de-scripts-mk)
4. [Referencia Completa de Comandos](#4-referencia-completa-de-comandos)
5. [Ejemplos Prácticos para LLMs](#5-ejemplos-prácticos-para-llms)
6. [Planificación de Tareas](#6-planificación-de-tareas)
7. [Depuración y Registro de Actividad (Logs)](#7-depuración-y-registro-de-actividad-logs)
8. [Configuración Avanzada y Acceso sin Contraseña (Udev)](#8-configuración-avanzada-y-acceso-sin-contraseña-udev)
9. [Resolución de Problemas Comunes (Troubleshooting)](#9-resolución-de-problemas-comunes-troubleshooting)

---

## 1. El Daemon (`mk-daemon`) y Seguridad

Bajo Wayland, los entornos de escritorio modernos restringen el envío de teclas virtuales por seguridad. `mk` resuelve esto utilizando un teclado virtual a nivel de kernel mediante `/dev/uinput` a través de su daemon de fondo.

### Iniciar el Daemon de forma segura
Puedes iniciarlo de dos formas:
* **Como usuario normal** (si perteneces al grupo `input` o tienes reglas `udev` configuradas):
  ```bash
  mk-daemon &
  ```
* **Con permisos de administrador (Sudo)**:
  ```bash
  sudo mk daemon start
  ```

> [!IMPORTANT]
> **Seguridad mejorada:** Al iniciar el daemon (incluso con `sudo`), el socket de conexión `/tmp/mk-daemon.sock` se asignará automáticamente a tu usuario original con permisos `srw-------` (`0o600`). Esto garantiza que **ningún otro usuario del equipo pueda enviar teclas a tu pantalla**.

### Comprobar el estado
```bash
mk daemon status
```

---

## 2. Uso Directo desde la Terminal (Comandos Simples)

Cualquier comando básico de `mk` se puede ejecutar directamente en tu consola sin necesidad de escribir un archivo de script. Esto es ideal para automatizaciones rápidas de una sola línea o alias de terminal:

* **Escribir texto:**
  ```bash
  mk text "Hola mundo"
  ```
* **Pulsar Enter:**
  ```bash
  mk enter
  ```
* **Enviar combinaciones de teclas (atajos):**
  ```bash
  mk key "ctrl+alt+t"
  ```
* **Esperar un tiempo:**
  ```bash
  mk wait "2s"
  ```
* **Pegar texto (vía portapapeles):**
  ```bash
  mk paste "Texto especial con tildes y eñes"
  ```
* **Pegar archivos y carpetas directamente:**
  Puedes pegar archivos completos o la estructura de un directorio en tu navegador simplemente ejecutando:
  ```bash
  mk paste-file Cargo.toml
  mk paste-dir src
  ```
* **Ejecutar comandos programados en consola:**
  Puedes programar un comando simple desde la consola usando `in` o `at`:
  ```bash
  mk in "5s" enter
  mk at "23:59" paste-file "informe.txt"
  ```

---

## 3. Sintaxis de Scripts (`.mk`)

Los scripts son simples archivos de texto plano con extensión `.mk`.
* **Comentarios:** Cualquier texto precedido por `#` es ignorado.
* **Comentarios Inline:** Puedes poner comentarios al final de tus líneas de código (ej: `enter # Pulsar enter`). El parser es lo suficientemente inteligente como para no borrar los `#` que estén dentro de textos entrecomillados.
* **Variables:** Puedes definir variables con `set` y usarlas con `${mi_variable}`.

---

## 4. Referencia Completa de Comandos

### Simulación de Teclado y Portapapeles

| Comando | Descripción | Ejemplo |
| :--- | :--- | :--- |
| `text "<mensaje>"` | Escribe el mensaje letra a letra. (Recomendado solo para ASCII básico). | `text "hola"` |
| `enter` | Pulsa la tecla Enter. | `enter` |
| `key "<combinación>"` | Envía combinaciones de teclas complejas. | `key "ctrl+alt+t"` |
| `paste "<mensaje>"` | Copia al portapapeles y hace un pegado virtual (Ctrl+V). Evita errores de distribución de teclado y soporta tildes o eñes. | `paste "Hola, ¿qué tal?"` |

### Comandos Especiales para LLMs

#### `paste-file "<ruta>"`
Lee el archivo indicado, lo envuelve en un bloque de código Markdown autodetectando la extensión del archivo y lo pega en el chat activo.
* *Ejemplo:*
  ```bash
  paste-file "src/main.rs"
  ```
  *(Pegará en el navegador:*
  *Archivo: src/main.rs*
  *\`\`\`rust*
  *... contenido ...*
  *\`\`\`)*

#### `paste-dir "<ruta>"`
Recorre de forma recursiva la carpeta indicada, extrae el texto de todos los archivos de código y texto, y los pega de golpe en un solo mensaje de Markdown con sus nombres relativos.
* Omite automáticamente carpetas pesadas o de configuración (`.git`, `node_modules`, `target`, `dist`, `venv`, etc.) y archivos binarios.
* *Ejemplo:*
  ```bash
  paste-dir "src"
  ```

#### `exec <nombre_variable> "<comando>"`
Ejecuta un comando de consola en tu terminal de Linux y guarda la salida (`stdout`) en la variable especificada para que puedas pegarla más tarde en tus prompts.
* *Ejemplo:*
  ```bash
  exec ultimos_logs "git log -n 3 --oneline"
  paste "Estos son los últimos commits:"
  enter
  paste "${ultimos_logs}"
  enter
  ```

### Bucles e Inclusiones

#### `repeat N { ... }`
Repite el bloque de comandos `N` veces. Soporta anidación.
* *Ejemplo:*
  ```bash
  repeat 3 {
      text "hola"
      enter
  }
  ```

#### `include "<archivo.mk>"`
Inserta e interpreta los comandos de otro archivo `.mk` de forma modular.
* *Ejemplo:* `include "cabecera.mk"`

### Tiempos y Esperas

#### `wait "<duración>"`
Pausa el script. Soporta milisegundos (`ms`), segundos (`s`), minutos (`m`) y horas (`h`), tanto de forma simple como compuestas (separadas por espacio u opcionales).
* *Ejemplo:* `wait "500ms"`, `wait "2s"`, `wait "1h"`, `wait "1h 53m"`

#### `keep-awake "<intervalo>"`
Presiona la tecla invisible F15 de forma periódica en segundo plano para evitar que la pantalla se apague o el equipo se suspenda mientras se ejecutan tareas. **No bloquea el resto del script.**
* *Ejemplo:* `keep-awake "2m"`

---

## 5. Ejemplos Prácticos para LLMs

### Ejemplo A: Pasar un archivo de código y pedir feedback
Crea un script `pedir_ayuda.mk`:
```bash
# Escribir el prompt inicial en el chat activo
paste "Por favor, analiza este archivo de código en Rust y dime si ves algún posible bug:"
enter
wait "500ms"

# Pegar el archivo
paste-file "src/main.rs"
enter
```

### Ejemplo B: Consultar logs de Git y enviarlos al chat
Crea un script `consultar_git.mk`:
```bash
# Ejecutar comando de git
exec diff "git diff HEAD~1"

paste "Hola. Acabo de hacer estos cambios en mi repositorio. ¿Ves algún error de diseño?"
enter
wait "500ms"

# Pegar la salida del comando exec
paste "${diff}"
enter
```

---

## 6. Planificación de Tareas

Puedes programar scripts para que se ejecuten a horas exactas (utilizando tu zona horaria local).

### `at "<HH:MM>" { ... }`
Ejecuta el bloque interno a la hora local indicada.
* *Ejemplo:*
  ```bash
  at "04:33" {
      paste "Mensaje de auditoría nocturna"
      enter
  }
  ```

### `in "<duración>" { ... }`
Ejecuta el bloque tras una espera.
* *Ejemplo:*
  ```bash
  in "5h" {
      paste "Sesión iniciada"
      enter
  }
  ```

### Ejecutar scripts de forma independiente
Para lanzar tu script en segundo plano y poder cerrar la consola tranquilamente mientras se espera la hora planificada:
```bash
nohup mk run mi_script.mk > /dev/null 2>&1 &
```

---


## 6. Depuración y Registro de Actividad (Logs)

Para asegurar que un script funciona como deseas antes de dejarlo corriendo solo en segundo plano, `mk` ofrece herramientas de depuración:

### El Modo de Simulación (`--dry-run`)
Permite verificar la lógica del script (esperas, bucles, variables y carga de archivos) por la terminal sin llegar a pulsar ninguna tecla física en la pantalla de tu ordenador:
```bash
mk --dry-run run mi_script.mk
```
*Esto mostrará en tiempo real por la terminal cada acción que `mk` ejecutaría de manera simulada.*

### Registro de Actividad (Logs)
Puedes guardar un historial detallado de todas las pulsaciones de teclado y comandos simulados en un archivo de logs:
```bash
mk --log historial.log run mi_script.mk
```
Cada línea del archivo `historial.log` registrará la fecha y hora exacta (`timestamp`), el tipo de acción y si se ejecutó con éxito.

---

## 7. Configuración Avanzada y Acceso sin Contraseña (Udev)

Por defecto, para abrir `/dev/uinput` y crear el teclado virtual, Linux requiere privilegios de superusuario (`sudo`). Si quieres poder ejecutar `mk-daemon` como usuario normal sin que te solicite nunca la contraseña `sudo`, sigue estos pasos:

1. **Crear la regla de Udev:**
   Crea un archivo de configuración en `/etc/udev/rules.d/99-uinput.rules`:
   ```bash
   sudo nano /etc/udev/rules.d/99-uinput.rules
   ```
2. **Escribir el permiso:**
   Introduce el siguiente contenido para que el grupo del sistema `input` tenga acceso de lectura/escritura al dispositivo virtual:
   ```text
   KERNEL=="uinput", GROUP="input", MODE="0660", OPTIONS+="static_node=uinput"
   ```
3. **Aplicar la configuración:**
   Recarga las reglas de udev para que el sistema las reconozca inmediatamente:
   ```bash
   sudo udevadm control --reload-rules && sudo udevadm trigger
   ```
4. **Unirte al grupo `input`:**
   Asegúrate de que tu usuario pertenece al grupo `input` de tu sistema:
   ```bash
   sudo usermod -aG input $USER
   ```
   *(Nota: Deberás cerrar sesión en tu escritorio de Linux y volver a iniciarla para que tu grupo de usuario se actualice).*

Una vez hecho esto, podrás arrancar el daemon simplemente llamando a `mk-daemon &` como un usuario normal.

---

## 8. Resolución de Problemas Comunes (Troubleshooting)

### El teclado escribe caracteres incorrectos o se come las tildes/eñes
* **Causa:** El comando `text` simula pulsaciones de teclas basadas en la distribución física de EE.UU. Si tu teclado está configurado en español u otro idioma, los caracteres diferirán.
* **Solución:** Utiliza el comando **`paste`** (o `paste-file` / `paste-dir`). Al usar el portapapeles y simular un Ctrl+V, evita por completo incompatibilidades de diseño de teclas.

### Los atajos no se ejecutan en mi aplicación (como cambiar de pestaña)
* **Causa:** El entorno de escritorio a veces necesita un pequeño instante para cambiar el foco a la ventana activa antes de recibir combinaciones de teclas.
* **Solución:** Introduce un pequeño tiempo de espera de seguridad antes del atajo. Ej:
  ```bash
  wait "200ms"
  key "ctrl+tab"
  ```

### El script planificado a las 4:33 AM no se ejecutó de noche
* **Causa:** Si tu ordenador se suspendió (entró en Sleep) o la pantalla de bloqueo de sesión se activó, el sistema operativo pausa los procesos de usuario o restringe la entrada de teclado por seguridad.
* **Solución:** Desactiva temporalmente el bloqueo de pantalla y la suspensión automática en los ajustes de energía de tu sistema si planeas dejar una tarea planificada por la noche.
