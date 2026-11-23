# Changelog

All notable changes to Rayo. Format: [Keep a Changelog](https://keepachangelog.com/), versions: [SemVer](https://semver.org/).

**English** · [Español](#español)

## [Unreleased]

### Security

- The in-app updater verifies the SHA-256 of the downloaded exe against `rayo.exe.sha256` from the release, and refuses releases without it.
- `install.ps1` verifies the same checksum (it only warns for v0.1.0, which predates the file).
- Releases are built with `--locked` and carry a build provenance attestation (`gh attestation verify`).

### Fixed

- A crash in one pane's I/O thread no longer takes down the whole window: locks survive it and the pane is closed.
- A shell that exits right after starting (wrong program or arguments, broken profile) stays open with its error and a message, instead of vanishing.
- The last output of a shell that exits is no longer lost.
- An invalid `config.toml` is copied to `config.invalid.toml` before anything can overwrite it, and Rayo says so.
- `config.toml` and the session are written atomically; settings are saved once a change settles, not on every frame of a slider drag.
- `history.txt` no longer grows forever: it is trimmed back to the last 5000 commands.
- Ctrl+Shift+V in the editor pastes at the cursor, not at the end.
- Agent detection no longer matches commands like `cd claude-notes`.
- Claude Code usage is read line by line (big logs) and only one scan runs at a time.
- Several tooltips (window buttons, update, close pane, shortcuts) were Spanish-only.
- Fonts are looked up in `%WINDIR%\Fonts`; Git Bash is also found in per-user installs.

### Added

- Closing a pane, tab or the window while a command runs asks first (Settings → Terminal → Confirm on close).
- Commands starting with a space are not saved to the history.
- Panics are written to `%APPDATA%\rayo\crash.log`.
- The exe has version information (Explorer → Properties → Details).
- End-to-end tests over a real ConPTY (PowerShell block with exit code, shell exit).

## [0.1.0] — 2026-11-23

First public release. A fast, lightweight terminal for Windows 10 (1809+) / 11, written in Rust. No AI built in and no account needed.

### Install

```
irm https://raw.githubusercontent.com/PabloGonSan8/rayo-terminal/main/install.ps1 | iex
```

Or download `rayo.exe` below and run it. Windows may show a SmartScreen warning because the exe isn't code-signed yet: click *More info → Run anyway*.

### Features

- **Command blocks** (PowerShell / pwsh): each command with its folder, git branch, output, exit status and duration. Right click a block to copy or edit it again.
- **Built-in editor**: Shift+Enter for multi-line commands, persistent history, Tab path completion, inline suggestions.
- **Command highlighting**: known commands (`claude`, `git`, `node`, `Get-ChildItem`…) are colored as you type.
- **Tabs and split panes**, resizable by dragging, with focus and zoom shortcuts. Shortcuts can be changed in Settings.
- **6 themes** (Rayo, Grafito, Nord, Gruvbox Dark, Solarized Dark, Claro), with an optional day/night theme switch.
- **AI agents panel**: lists the agents running in your panes (Claude Code, Codex, Gemini CLI…) and shows Claude Code usage estimated from local logs (5 h session, last 7 days, per model). Nothing is sent anywhere.
- **Works with TUIs**: vim, less, fzf and Claude Code get a full terminal view. Shift+Enter inserts a new line in Claude Code.
- **Real Ctrl+C**: interrupting also kills orphaned child processes, and closing a pane closes everything it started.
- **Session restore**: tabs, splits and folders come back when you reopen Rayo.
- **English and Spanish UI**: follows the Windows language, or set it in Settings → Appearance → Language.
- **Windows integration**: `rayo` on the PATH (`rayo C:\project`, `rayo -d C:\project`), **Open in Rayo** on folder right click, uninstall from Settings → Apps.
- **Auto-update**: Rayo tells you when a new version is out and installs it with one click.
- **Light on resources**: OpenGL rendering, about 75 MB of RAM and 0 % CPU when idle.

### Known limitations

- Windows only. Linux and macOS are not supported yet.
- Rayo can't be chosen as Windows 11's default terminal application.
- Command blocks need PowerShell. cmd, WSL and Git Bash run as a classic terminal.

---

## Español

## [Sin publicar]

### Seguridad

- El actualizador comprueba el SHA-256 del exe descargado con el `rayo.exe.sha256` de la versión y rechaza las versiones que no lo traen.
- `install.ps1` comprueba la misma suma (con v0.1.0, que no la tiene, solo avisa).
- Las versiones se compilan con `--locked` y llevan una atestación de procedencia del build (`gh attestation verify`).

### Arreglado

- Un fallo en el hilo de E/S de un panel ya no cierra la ventana entera: los bloqueos lo aguantan y se cierra solo ese panel.
- Un shell que se cierra nada más arrancar (programa o argumentos erróneos, perfil roto) se queda abierto con su error y un aviso, en vez de desaparecer.
- Ya no se pierde la última salida de un shell que termina.
- Un `config.toml` inválido se copia a `config.invalid.toml` antes de que nada lo sobrescriba, y Rayo avisa.
- `config.toml` y la sesión se escriben de forma atómica; los ajustes se guardan cuando el cambio se estabiliza, no en cada frame al arrastrar un deslizador.
- `history.txt` ya no crece sin límite: se recorta a los últimos 5000 comandos.
- Ctrl+Shift+V en el editor pega en el cursor, no al final.
- La detección de agentes ya no reconoce comandos como `cd claude-notes`.
- El uso de Claude Code se lee línea a línea (registros grandes) y solo corre un cálculo a la vez.
- Varias ayudas emergentes (botones de ventana, actualizar, cerrar panel, atajos) solo estaban en español.
- Las fuentes se buscan en `%WINDIR%\Fonts`; Git Bash también se encuentra en instalaciones por usuario.

### Añadido

- Cerrar un panel, una pestaña o la ventana con un comando en marcha pide confirmación (Ajustes → Terminal → Confirmar al cerrar).
- Los comandos que empiezan por espacio no se guardan en el historial.
- Los errores fatales se escriben en `%APPDATA%\rayo\crash.log`.
- El exe lleva información de versión (Explorador → Propiedades → Detalles).
- Tests de extremo a extremo sobre ConPTY real (bloque de PowerShell con código de salida, salida del shell).

## [0.1.0] — 2026-11-23

Primera versión pública. Terminal rápida y ligera para Windows 10 (1809+) / 11, escrita en Rust. Sin IA integrada y sin cuentas.

### Instalar

```
irm https://raw.githubusercontent.com/PabloGonSan8/rayo-terminal/main/install.ps1 | iex
```

O descarga `rayo.exe` abajo y ábrelo. Windows puede mostrar un aviso de SmartScreen porque el exe todavía no está firmado: pulsa *Más información → Ejecutar de todas formas*.

### Novedades

- **Bloques de comandos** (PowerShell / pwsh): cada comando con su carpeta, rama de git, salida, estado y duración. Clic derecho en un bloque para copiarlo o editarlo de nuevo.
- **Editor propio**: Shift+Enter para comandos de varias líneas, historial que se guarda, Tab completa rutas y sugerencias mientras escribes.
- **Comandos en color**: los comandos conocidos (`claude`, `git`, `node`, `Get-ChildItem`…) se colorean mientras escribes.
- **Pestañas y paneles divididos**, que se redimensionan arrastrando, con atajos para mover el foco y maximizar. Los atajos se pueden cambiar en Ajustes.
- **6 temas** (Rayo, Grafito, Nord, Gruvbox Dark, Solarized Dark, Claro), con cambio opcional entre tema de día y de noche.
- **Panel de agentes de IA**: muestra los agentes que corren en tus paneles (Claude Code, Codex, Gemini CLI…) y el uso de Claude Code calculado a partir de los registros locales (sesión de 5 h, últimos 7 días, por modelo). No se envía nada a ningún sitio.
- **Funciona con programas de pantalla completa**: vim, less, fzf y Claude Code se ven en vista de terminal completa. Shift+Enter hace salto de línea en Claude Code.
- **Ctrl+C de verdad**: al interrumpir también se cierran los procesos que se quedan huérfanos, y al cerrar un panel se cierra todo lo que lanzó.
- **Restaurar sesión**: al volver a abrir Rayo vuelven las pestañas, las divisiones y las carpetas.
- **Interfaz en español e inglés**: sigue el idioma de Windows, o se elige en Ajustes → Apariencia → Idioma.
- **Integración con Windows**: `rayo` en el PATH (`rayo C:\proyecto`, `rayo -d C:\proyecto`), **Abrir en Rayo** en el clic derecho de las carpetas y desinstalación desde Configuración → Aplicaciones.
- **Actualización automática**: Rayo avisa cuando hay una versión nueva y la instala con un clic.
- **Consume poco**: renderizado OpenGL, unos 75 MB de RAM y 0 % de CPU en reposo.

### Limitaciones conocidas

- Solo Windows. Linux y macOS todavía no están soportados.
- Rayo no se puede elegir como aplicación de terminal predeterminada de Windows 11.
- Los bloques de comandos necesitan PowerShell. cmd, WSL y Git Bash funcionan como terminal clásica.

[0.1.0]: https://github.com/PabloGonSan8/rayo-terminal/releases/tag/v0.1.0
