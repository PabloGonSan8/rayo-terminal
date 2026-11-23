# Security / Seguridad

**English** · [Español](#español)

Please **don't open a public issue** for security problems. Report them privately via
**Security → Report a vulnerability** on GitHub. You'll get an answer as soon as possible.
Only the latest release gets fixes.

## Verifying a download

Every release after v0.1.0 publishes `rayo.exe.sha256` next to `rayo.exe` and a build
provenance attestation:

```powershell
Get-FileHash rayo.exe -Algorithm SHA256          # must match rayo.exe.sha256
gh attestation verify rayo.exe -R PabloGonSan8/rayo-terminal
```

The in-app updater and `install.ps1` check the SHA-256 automatically. The checksum catches
corrupt or swapped downloads; it does not protect against a compromised maintainer account.
Code signing (Authenticode) is planned and will close that gap.

## What Rayo stores

Settings, session layout and command history live in `%APPDATA%\rayo` as plain text. History
can contain secrets you typed: start a command with a space to keep it out. Nothing is sent
anywhere except the update check (`api.github.com`, once per launch).

---

## Español

Por favor, **no abras un issue público** para problemas de seguridad. Infórmalos en privado en
**Security → Report a vulnerability** en GitHub. Recibirás respuesta lo antes posible.
Solo la última versión recibe arreglos.

## Verificar una descarga

Cada versión posterior a v0.1.0 publica `rayo.exe.sha256` junto a `rayo.exe` y una atestación de
procedencia del build:

```powershell
Get-FileHash rayo.exe -Algorithm SHA256          # tiene que coincidir con rayo.exe.sha256
gh attestation verify rayo.exe -R PabloGonSan8/rayo-terminal
```

El actualizador integrado e `install.ps1` comprueban el SHA-256 solos. La suma detecta descargas
corruptas o sustituidas; no protege si se compromete la cuenta del mantenedor. Está previsto firmar
el código (Authenticode), que cierra ese hueco.

## Qué guarda Rayo

Los ajustes, la disposición de la sesión y el historial están en `%APPDATA%\rayo` en texto plano.
El historial puede contener secretos que hayas escrito: empieza el comando con un espacio para que
no se guarde. No se envía nada a ningún sitio salvo la comprobación de actualizaciones
(`api.github.com`, una vez por arranque).
