# Installs Rayo for the current user (no admin needed):
#   - %LOCALAPPDATA%\Programs\Rayo\rayo.exe, added to the user PATH (`rayo`, `rayo <folder>`)
#   - Start menu shortcut "Rayo"
#   - Explorer right click "Open in Rayo" on folders, folder background and drives
#   - Settings > Apps entry with an uninstaller
#
# Two ways to run it:
#   - Remote (no Rust needed), downloads the latest release:
#       irm https://raw.githubusercontent.com/PabloGonSan8/rayo-terminal/main/install.ps1 | iex
#   - From a clone, builds from source:
#       powershell -ExecutionPolicy Bypass -File install.ps1
# Run again to update (works even while Rayo is open).
# Wrapped in a script block so `irm | iex` doesn't leak settings into the caller's session
# and errors don't close their window.
& {
    $ErrorActionPreference = 'Stop'
    $ProgressPreference = 'SilentlyContinue'  # Invoke-WebRequest is very slow with the progress bar
    $es = (Get-UICulture).TwoLetterISOLanguageName -eq 'es'
    function Say($esText, $enText, $color = 'Gray') {
        Write-Host $(if ($es) { $esText } else { $enText }) -ForegroundColor $color
    }

    $repo = 'PabloGonSan8/rayo-terminal'
    $dir = Join-Path $env:LOCALAPPDATA 'Programs\Rayo'
    New-Item -ItemType Directory -Force $dir | Out-Null
    $exe = Join-Path $dir 'rayo.exe'
    $new = Join-Path $dir 'rayo.exe.new'

    $fromSource = $PSScriptRoot -and (Test-Path (Join-Path $PSScriptRoot 'Cargo.toml'))
    if ($fromSource) {
        Push-Location $PSScriptRoot
        try {
            cargo build --release
            if ($LASTEXITCODE -ne 0) { throw "cargo build failed ($LASTEXITCODE)" }
            Copy-Item target\release\rayo.exe $new -Force
            $version = (Select-String -Path Cargo.toml -Pattern '^version = "(.+)"' | Select-Object -First 1).Matches.Groups[1].Value
        } finally { Pop-Location }
    } else {
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        $url = "https://github.com/$repo/releases/latest/download/rayo.exe"
        Say "Descargando $url" "Downloading $url"
        Invoke-WebRequest $url -OutFile $new -UseBasicParsing
        try {
            $version = (Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest").tag_name.TrimStart('v')
        } catch { $version = '' }
    }

    # A running exe can't be overwritten but can be renamed: Rayo keeps running from
    # rayo.exe.old (deleted on its next start) and the next launch uses the new one.
    if (Test-Path $exe) {
        Remove-Item "$exe.old" -Force -ErrorAction SilentlyContinue
        Rename-Item $exe 'rayo.exe.old'
    }
    Move-Item $new $exe -Force
    if (Get-Process rayo -ErrorAction SilentlyContinue) {
        Say 'Rayo sigue abierto con la versión anterior: ciérralo y ábrelo para usar la nueva.' `
            'Rayo is still running the old version: close and reopen it to use the new one.' Yellow
    }

    # Start menu shortcut.
    $lnk = Join-Path ([Environment]::GetFolderPath('Programs')) 'Rayo.lnk'
    $s = (New-Object -ComObject WScript.Shell).CreateShortcut($lnk)
    $s.TargetPath = $exe
    $s.WorkingDirectory = $env:USERPROFILE
    $s.IconLocation = "$exe,0"
    $s.Description = 'Rayo — terminal'
    $s.Save()

    # User PATH. Edited in the registry to keep REG_EXPAND_SZ entries like %USERPROFILE%\bin intact.
    $envKey = 'HKCU:\Environment'
    $path = (Get-Item $envKey).GetValue('Path', '', 'DoNotExpandEnvironmentNames')
    if (($path -split ';') -notcontains $dir) {
        $path = (($path.TrimEnd(';'), $dir) | Where-Object { $_ }) -join ';'
        Set-ItemProperty $envKey Path $path -Type ExpandString
        # Setting any user variable broadcasts WM_SETTINGCHANGE so new consoles see the PATH.
        [Environment]::SetEnvironmentVariable('RAYO_INSTALL', '1', 'User')
        [Environment]::SetEnvironmentVariable('RAYO_INSTALL', $null, 'User')
    }

    # Explorer right click: folder, folder background, drive.
    $label = if ($es) { 'Abrir en Rayo' } else { 'Open in Rayo' }
    foreach ($k in 'Directory', 'Directory\Background', 'Drive') {
        $key = "HKCU:\Software\Classes\$k\shell\Rayo"
        New-Item "$key\command" -Force | Out-Null
        Set-ItemProperty $key '(default)' $label
        Set-ItemProperty $key Icon "`"$exe`",0"
        Set-ItemProperty "$key\command" '(default)' "`"$exe`" `"%V`""
    }

    # Uninstaller + Settings > Apps entry.
    $uninstall = Join-Path $dir 'uninstall.ps1'
    Set-Content $uninstall -Encoding UTF8 -Value @'
# Removes Rayo (settings and history in %APPDATA%\rayo are kept).
$ErrorActionPreference = 'Stop'
if (Get-Process rayo -ErrorAction SilentlyContinue) {
    Write-Host 'Close Rayo first / Cierra Rayo primero.' -ForegroundColor Yellow
    exit 1
}
$dir = Join-Path $env:LOCALAPPDATA 'Programs\Rayo'
foreach ($k in 'Directory', 'Directory\Background', 'Drive') {
    Remove-Item "HKCU:\Software\Classes\$k\shell\Rayo" -Recurse -Force -ErrorAction SilentlyContinue
}
Remove-Item 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Rayo' -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item (Join-Path ([Environment]::GetFolderPath('Programs')) 'Rayo.lnk') -Force -ErrorAction SilentlyContinue
$path = (Get-Item 'HKCU:\Environment').GetValue('Path', '', 'DoNotExpandEnvironmentNames')
$kept = ($path -split ';' | Where-Object { $_ -and $_ -ne $dir }) -join ';'
if ($kept -ne $path) {
    Set-ItemProperty 'HKCU:\Environment' Path $kept -Type ExpandString
    [Environment]::SetEnvironmentVariable('RAYO_INSTALL', '1', 'User')
    [Environment]::SetEnvironmentVariable('RAYO_INSTALL', $null, 'User')
}
Remove-Item $dir -Recurse -Force
Write-Host 'Rayo uninstalled / Rayo desinstalado.' -ForegroundColor Green
'@
    $app = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Rayo'
    New-Item $app -Force | Out-Null
    $props = @{
        DisplayName     = 'Rayo'
        DisplayIcon     = "$exe,0"
        Publisher       = 'PabloGonSan8'
        InstallLocation = $dir
        URLInfoAbout    = "https://github.com/$repo"
        UninstallString = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$uninstall`""
    }
    if ($version) { $props.DisplayVersion = $version }
    foreach ($p in $props.GetEnumerator()) { Set-ItemProperty $app $p.Key $p.Value }
    Set-ItemProperty $app NoModify 1 -Type DWord
    Set-ItemProperty $app NoRepair 1 -Type DWord

    Say "Instalado en $dir" "Installed to $dir" Green
    Say "Abre una consola nueva y escribe 'rayo', o clic derecho en una carpeta > $label." `
        "Open a new console and type 'rayo', or right click a folder > $label."
    Say 'Para anclarlo: menú Inicio > Rayo > clic derecho > Anclar a la barra de tareas.' `
        'To pin it: Start menu > Rayo > right click > Pin to taskbar.'
}
