# Rayo ⚡

[![CI](https://github.com/PabloGonSan8/rayo-terminal/actions/workflows/ci.yml/badge.svg)](https://github.com/PabloGonSan8/rayo-terminal/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/PabloGonSan8/rayo-terminal)](https://github.com/PabloGonSan8/rayo-terminal/releases/latest)
[![Licencia: MIT](https://img.shields.io/badge/licencia-MIT-blue.svg)](LICENSE)
![Windows 10/11](https://img.shields.io/badge/plataforma-Windows%2010%2F11-0078D6)

[English](README.md) · **Español**

Terminal nativa para Windows escrita en Rust, pensada para PowerShell y para trabajar con CLIs
de IA: bloques de comandos al estilo Warp sin cuenta, un Ctrl+C que de verdad limpia lo que deja
y un único `.exe`.

Sin IA integrada, sin login, sin telemetría. Si usas agentes como Claude Code, Codex o Gemini CLI,
Rayo los aloja bien (Shift+Enter, pegar imágenes, panel de agentes), pero nunca los necesita.

- Motor VT: `alacritty_terminal` (ConPTY en Windows)
- UI: `egui` / `eframe` (OpenGL, glow)
- Idioma: Ajustes → Apariencia → Idioma (Automático sigue el idioma de Windows).

> Plataformas: solo **Windows 10 (1809+) / 11**. Linux y macOS todavía no están soportados.

## Instalación

### Opción A — descargar (sin herramientas)

En PowerShell (sin admin). Instala en `%LOCALAPPDATA%\Programs\Rayo`, añade `rayo` al PATH, crea el acceso en el menú Inicio, añade **Abrir en Rayo** al clic derecho de las carpetas y lo registra en Configuración → Aplicaciones:

```
irm https://raw.githubusercontent.com/PabloGonSan8/rayo-terminal/main/install.ps1 | iex
```

O a mano:

1. Ve a [Releases](https://github.com/PabloGonSan8/rayo-terminal/releases/latest) y descarga `rayo.exe`.
2. Ponlo en cualquier carpeta (p. ej. `%LOCALAPPDATA%\Programs\Rayo`) y ábrelo.
3. Rayo avisa cuando hay una versión nueva. El actualizador integrado solo instala una versión cuyo SHA-256 coincide con el `rayo.exe.sha256` publicado con ella.

Para comprobar una descarga tú mismo: `Get-FileHash rayo.exe -Algorithm SHA256` tiene que coincidir
con `rayo.exe.sha256` de la misma versión, y `gh attestation verify rayo.exe -R PabloGonSan8/rayo-terminal`
comprueba que lo compiló el workflow de release de este repo. El exe todavía no está firmado, así que
SmartScreen puede avisar: *Más información → Ejecutar de todas formas*.

### Opción B — compilar desde el código

Requisitos: [Rust](https://rustup.rs) (stable) y las *Build Tools* de Visual Studio (C++). Luego:

```
git clone https://github.com/PabloGonSan8/rayo-terminal
cd rayo-terminal
powershell -ExecutionPolicy Bypass -File install.ps1
```

Compila, copia `rayo.exe` a `%LOCALAPPDATA%\Programs\Rayo` y crea el acceso "Rayo" en el menú Inicio (no requiere admin).
Para anclarlo: menú Inicio → Rayo → clic derecho → **Anclar a la barra de tareas**. Para actualizar: `git pull` y vuelve a ejecutar el script (aunque Rayo esté abierto), luego reinicia Rayo.

Solo ejecutar sin instalar: `cargo run --release` (el exe queda en `target\release\rayo.exe`).

### Desinstalar

Configuración → Aplicaciones → Rayo → Desinstalar (quita el exe, el PATH, el menú contextual y el acceso). Los ajustes y el historial se quedan en `%APPDATA%\rayo`; bórralo si no los quieres.

## Línea de comandos

```
rayo                 # abre Rayo (restaura la sesión)
rayo C:\proyecto     # ventana nueva en esa carpeta
rayo -d C:\proyecto  # igual, como Windows Terminal
```

## Desarrollo

- `cargo test` ejecuta los tests.
- La CI (`.github/workflows/ci.yml`) pasa clippy, los tests y un build release en Windows en cada PR y en cada push a `main`.
- Activa el hook que impide hacer `git push` si fallan los tests (una vez por clon):
  ```
  git config core.hooksPath .githooks
  ```
- Para que GitHub no deje fusionar a `main` sin tests verdes: Settings → Branches → regla para `main` → *Require status checks* → `test (windows-latest)`.

## Bloques de comandos (PowerShell / pwsh)

- Editor propio fijo abajo: Enter ejecuta, Shift+Enter nueva línea, ↑/↓ historial (persistente en `%APPDATA%\rayo\history.txt`), Tab completa rutas, → acepta la sugerencia gris.
- Cada comando es un bloque: directorio, rama git, comando, salida, estado (✓ / ✗ código / Interrumpido) y duración.
- Mientras un comando corre, el teclado va al proceso (Ctrl+C interrumpe). Apps a pantalla completa (vim, less, fzf…) pasan a vista de terminal completa y vuelven a bloques al salir.
- Clic derecho en un bloque: copiar comando / salida, o editarlo de nuevo. `cls` o Ctrl+L limpian los bloques.
- cmd, WSL y Git Bash funcionan como terminal clásica.

Cómo funciona: Rayo arranca PowerShell con un prompt propio que emite marcas OSC 133 (fin + código de salida, listo) y cada comando se envía envuelto para marcar el inicio de su salida. Un bucle de E/S propio sobre ConPTY captura cada bloque en el momento exacto en que termina.

## Ajustes

Icono de engranaje arriba a la derecha o `Ctrl + ,`. Tiene buscador: escribe "fuente", "cursor", "shell"…
Todo se guarda solo en `%APPDATA%\rayo\config.toml` (botón "Abrir archivo" para editarlo a mano).

## Atajos

| Acción | Atajo |
|---|---|
| Ajustes | `Ctrl + ,` |
| Nueva pestaña | `Ctrl + Shift + T` |
| Cerrar panel | `Ctrl + Shift + W` |
| Dividir derecha / abajo | `Ctrl + Shift + D` / `Ctrl + Shift + E` |
| Maximizar panel | `Ctrl + Shift + Enter` |
| Mover foco | `Ctrl + Alt + Flechas` |
| Cambiar pestaña | `Ctrl + Tab`, `Ctrl + 1…9` |
| Copiar / pegar | `Ctrl + C` (con selección) / `Ctrl + V`, clic derecho |
| Zoom de fuente | `Ctrl + =` / `Ctrl + -` / `Ctrl + 0` |
| Buscar en el historial (editor) | `Ctrl + R` |
| Buscar en la salida de los bloques | `Ctrl + F` |

Los separadores entre paneles se arrastran con el ratón para redimensionar.

## Novedades

- **Atajos editables**: Ajustes → Atajos. Clic en un atajo y pulsa la combinación nueva (Supr = sin asignar, Esc = cancelar). Si ya estaba en uso, se quita de la otra acción.
- **Tema según la hora**: Ajustes → Apariencia → "Tema según la hora" (tema de día/noche y horas de cambio).
- **Enlaces**: Ctrl + clic abre URLs (y enlaces OSC 8) en bloques y en vista terminal.
- **Selección en bloques**: arrastra para seleccionar; Ctrl+C o clic derecho copian.
- **Ctrl+C real**: cada panel corre en un Job Object de Windows; al interrumpir se terminan los procesos que el shell deja huérfanos (p. ej. `node` tras `pnpm dev`) y al cerrar un panel se cierra todo lo que lanzó.
- **Ligera en reposo**: renderizado OpenGL (glow), LTO; 0 % de CPU en reposo. RAM: el proceso de Rayo con una pestaña de PowerShell midió entre ~75 y ~140 MB de working set (`Get-Process rayo`, build release, Windows 11; varía según el estado de la ventana y la GPU).
- **Cierre seguro**: cerrar un panel, una pestaña o la ventana con un comando en marcha pide confirmación (Ajustes → Terminal → Confirmar al cerrar).
- **Historial privado**: un comando que empieza por espacio no se guarda en el historial (como `ignorespace` en bash).
- **Aviso al terminar**: la barra de tareas parpadea cuando termina un comando de 10 s o más con Rayo en segundo plano.
- **Registro de fallos**: si Rayo se cierra por un error, los detalles quedan en `%APPDATA%\rayo\crash.log`. Adjúntalo al reportar el bug.
- **Sesión**: al reabrir Rayo vuelven las pestañas, los splits y la carpeta de cada panel (Ajustes → Terminal → Restaurar sesión).
- **Autocompletado**: Tab abre la lista de carpetas/archivos (↑↓ elegir, Tab/Enter aceptar, Esc cerrar; se filtra al escribir).
- **Paneles**: Ctrl + Alt + flechas cambia de panel.
- **Barra «Ejecutando»**: oculta por defecto; el proceso ocupa todo el panel (Ajustes → Terminal).

## Contribuir

Se aceptan PRs: lee [CONTRIBUTING.md](CONTRIBUTING.md). Cada PR necesita la CI en verde y la aprobación del mantenedor.

## Licencia

[MIT](LICENSE)
