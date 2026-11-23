# Roadmap

**English** · [Español](#español)

Where Rayo is going and why. Ordered by real impact, not by feature count. Rayo stays
Windows-first, keeps AI optional and doesn't trade speed for looks.

**Positioning:** the native Windows terminal for PowerShell and AI-CLI workflows. Command blocks
without an account, a Ctrl+C that really cleans up (Job Objects), one light `.exe`.

## Phase 1 — Stability and security ✅ (unreleased, see CHANGELOG)

- SHA-256 verification in the updater and installer, `--locked` releases, provenance attestation.
- Locks survive a pane thread panic; panics go to `crash.log`.
- Invalid `config.toml` is backed up, never overwritten; atomic, debounced saves.
- Confirm before closing running commands; shells that die on start stay visible.
- Bounded history file, `ignorespace`, paste at cursor, agent detection fix, i18n gaps.
- End-to-end ConPTY tests in CI.

## Phase 2 — Before announcing (distribution and trust)

| Item | Why | Files |
|---|---|---|
| Code signing (SignPath OSS / Azure Trusted Signing) | SmartScreen warning blocks first-time users; closes the account-compromise gap | `release.yml` |
| WinGet manifest (`portable` installer) | `winget install Rayo` is how Windows devs install tools | new PR to `microsoft/winget-pkgs`, `wingetcreate` step in `release.yml` |
| Scoop bucket | Cheap, popular with devs | separate repo |
| README GIF + screenshots | Nobody stars what they can't see | `README*.md`, `docs/` |
| Website matches the app | The mockup shows a status bar and per-block clock the app doesn't have; "macOS & Linux soon" is a promise | site repo |
| Repo topics, Discussions, `good first issue` labels | Discoverability, a place for questions | GitHub settings |

## Phase 3 — Product (highest value first)

| Feature | Value | Effort | Perf. impact | Differentiation |
|---|---|---|---|---|
| Ctrl+F search in output / blocks | High | Medium | None | Low (expected) |
| Ctrl+R fuzzy history search | High | Low | None | Medium |
| Notify when a long command ends or an agent waits for input | High | Low | None | **High** |
| Blocks for bash/zsh (WSL, Git Bash) via OSC 133 | High | Medium | None | High |
| Profiles + shell menu on "+" | High | Medium | None | Low |
| Any installed font / Nerd Fonts, CJK fallback | Medium-high | Medium | Low | Low |
| Export a block (Markdown / text) | Medium | Low | None | Medium |
| Jump between / fold blocks (Ctrl+↑/↓) | Medium | Low | None | Medium |
| Reorder tabs by dragging | Medium | Low | None | Low |
| Import themes (Windows Terminal / iTerm JSON) | Medium | Low | None | Low |
| Cmdlet / git / npm completion | High | High | Low | Medium |
| Keep PSReadLine features (Ctrl+R, predictions) or replace them | High | High | Low | Medium |

## Phase 4 — Performance and maintainability

- **Measure** frame time under heavy output (`cat` of 50 MB, `cargo build`). If the UI stalls,
  switch `Shared` to `alacritty_terminal::sync::FairMutex` or parse in smaller slices
  (`session.rs`). No change without a measurement.
- Memory budget for frozen blocks (1000 blocks × up to 10k rows today).
- Nonce in the OSC 133 / 9;9 marks so program output can't fake a block end or cwd.
- Split `main.rs` (3k lines) into `layout.rs`, `settings.rs`, `editor.rs`: a mechanical move,
  no behaviour change, easier first PRs.
- Busy detection for non-PowerShell panes (job processes besides the shell) for close confirmation.

## Not planned

Built-in AI, accounts or cloud sync, a plugin system, an SSH manager, Lua config, a custom GPU
renderer, macOS/Linux before Windows is polished. Default terminal of Windows 11 (COM handoff)
is long term only.

---

## Español

Hacia dónde va Rayo y por qué. Ordenado por impacto real, no por número de funciones. Rayo
sigue centrado en Windows, con la IA siempre opcional y sin sacrificar velocidad por estética.

**Posicionamiento:** la terminal nativa de Windows para PowerShell y para trabajar con CLIs de IA.
Bloques sin cuenta, un Ctrl+C que de verdad limpia (Job Objects), un único `.exe` ligero.

## Fase 1 — Estabilidad y seguridad ✅ (sin publicar, ver CHANGELOG)

- Verificación SHA-256 en el actualizador y el instalador, releases con `--locked`, atestación de procedencia.
- Los bloqueos aguantan un panic de un panel; los panics van a `crash.log`.
- Un `config.toml` inválido se guarda aparte y nunca se sobrescribe; guardado atómico y con debounce.
- Confirmación antes de cerrar comandos en marcha; los shells que mueren al arrancar siguen visibles.
- Historial acotado, `ignorespace`, pegar en el cursor, detección de agentes, huecos de traducción.
- Tests de extremo a extremo sobre ConPTY en la CI.

## Fase 2 — Antes de anunciar (distribución y confianza)

| Tarea | Por qué | Archivos |
|---|---|---|
| Firma de código (SignPath OSS / Azure Trusted Signing) | El aviso de SmartScreen frena a quien lo prueba por primera vez; cierra el hueco de una cuenta comprometida | `release.yml` |
| Manifiesto de WinGet (instalador `portable`) | `winget install Rayo` es como los devs de Windows instalan herramientas | PR a `microsoft/winget-pkgs`, paso `wingetcreate` en `release.yml` |
| Bucket de Scoop | Barato y popular entre devs | repo aparte |
| GIF y capturas en el README | Nadie da estrella a lo que no ve | `README*.md`, `docs/` |
| La web refleja la app | El mockup enseña una barra de estado y un reloj por bloque que la app no tiene; "macOS & Linux soon" es una promesa | repo de la web |
| Topics, Discussions, etiquetas `good first issue` | Descubrimiento y un sitio para preguntas | ajustes de GitHub |

## Fase 3 — Producto (primero lo de más valor)

| Función | Valor | Esfuerzo | Impacto en rendimiento | Diferenciación |
|---|---|---|---|---|
| Ctrl+F en la salida / bloques | Alto | Medio | Nulo | Baja (se espera) |
| Ctrl+R: búsqueda difusa en el historial | Alto | Bajo | Nulo | Media |
| Aviso al acabar un comando largo o cuando un agente espera | Alto | Bajo | Nulo | **Alta** |
| Bloques en bash/zsh (WSL, Git Bash) con OSC 133 | Alto | Medio | Nulo | Alta |
| Perfiles y menú de shells en "+" | Alto | Medio | Nulo | Baja |
| Cualquier fuente instalada / Nerd Fonts, fallback CJK | Medio-alto | Medio | Bajo | Baja |
| Exportar un bloque (Markdown / texto) | Medio | Bajo | Nulo | Media |
| Saltar entre bloques / plegarlos (Ctrl+↑/↓) | Medio | Bajo | Nulo | Media |
| Reordenar pestañas arrastrando | Medio | Bajo | Nulo | Baja |
| Importar temas (JSON de Windows Terminal / iTerm) | Medio | Bajo | Nulo | Baja |
| Autocompletado de cmdlets / git / npm | Alto | Alto | Bajo | Media |
| Conservar lo de PSReadLine (Ctrl+R, predicciones) o sustituirlo | Alto | Alto | Bajo | Media |

## Fase 4 — Rendimiento y mantenibilidad

- **Medir** el tiempo de frame con mucha salida (`cat` de 50 MB, `cargo build`). Si la UI se
  atasca, pasar `Shared` a `alacritty_terminal::sync::FairMutex` o procesar en trozos más
  pequeños (`session.rs`). Ningún cambio sin medición.
- Presupuesto de memoria para los bloques terminados (hoy 1000 bloques × hasta 10k filas).
- Nonce en las marcas OSC 133 / 9;9 para que la salida de un programa no pueda falsear un fin de bloque o el cwd.
- Dividir `main.rs` (3k líneas) en `layout.rs`, `settings.rs`, `editor.rs`: movimiento mecánico,
  sin cambiar comportamiento, primeros PRs más fáciles.
- Detectar comandos en marcha en paneles que no son PowerShell (procesos del job aparte del shell) para la confirmación al cerrar.

## Fuera de planes

IA integrada, cuentas o sincronización en la nube, sistema de plugins, gestor de SSH, config en
Lua, renderizador GPU propio, macOS/Linux antes de pulir Windows. Ser la terminal por defecto de
Windows 11 (traspaso COM) queda solo a largo plazo.
