# Changelog

All notable changes to Rayo. Format: [Keep a Changelog](https://keepachangelog.com/), versions: [SemVer](https://semver.org/).

**English** · [Español](#español)

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
