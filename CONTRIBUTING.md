# Contributing / Contribuir

**English** · [Español](#español)

Thanks for helping with Rayo!

1. Fork the repo and create a branch from `main`.
2. Make your change. Keep PRs small and focused.
3. Before pushing, run:
   ```
   cargo clippy --all-targets --locked -- -D warnings
   cargo test --locked
   ```
   Tip: `git config core.hooksPath .githooks` runs the tests automatically before every `git push`.
4. Open a pull request and fill in the template.

**How PRs get merged:** CI must pass (clippy + tests + release build on Windows) and the maintainer (@PabloGonSan8) must approve it. Nothing reaches `main` without both.

Guidelines:
- Rayo is Windows-only for now. Don't break Windows behaviour.
- New UI text is written in Spanish and gets its English translation in `src/i18n.rs`.
- Prefer the standard library over new dependencies.
- Add a test when you change logic (parsers, key handling, shell integration…).

By contributing you agree your code is released under the [MIT License](LICENSE) and you follow the [Code of Conduct](CODE_OF_CONDUCT.md).

---

## Español

¡Gracias por ayudar con Rayo!

1. Haz un fork y crea una rama desde `main`.
2. Haz tu cambio. PRs pequeños y concretos.
3. Antes de subir, ejecuta:
   ```
   cargo clippy --all-targets --locked -- -D warnings
   cargo test --locked
   ```
   Consejo: `git config core.hooksPath .githooks` corre los tests solo antes de cada `git push`.
4. Abre un pull request y rellena la plantilla.

**Cómo se aceptan los PRs:** la CI tiene que pasar (clippy + tests + build release en Windows) y el mantenedor (@PabloGonSan8) tiene que aprobarlo. Nada llega a `main` sin las dos cosas.

Normas:
- Rayo de momento es solo para Windows. No rompas el comportamiento en Windows.
- El texto nuevo de la interfaz se escribe en español y lleva su traducción al inglés en `src/i18n.rs`.
- Mejor la librería estándar que dependencias nuevas.
- Añade un test cuando cambies lógica (parsers, teclas, integración con el shell…).

Al contribuir aceptas que tu código se publica bajo la [licencia MIT](LICENSE) y que sigues el [Código de conducta](CODE_OF_CONDUCT.md).
