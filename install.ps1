# NavForensics — installation / build (Windows, PowerShell)
#
#   Usage :
#     powershell -ExecutionPolicy Bypass -File install.ps1            # installe + build l'installeur
#     powershell -ExecutionPolicy Bypass -File install.ps1 -Action dev   # installe + lance en dev
#     powershell -ExecutionPolicy Bypass -File install.ps1 -Action deps  # installe seulement les dependances
#
param(
    [ValidateSet("build", "dev", "deps")]
    [string]$Action = "build"
)
$ErrorActionPreference = "Stop"

function Info($m)  { Write-Host $m -ForegroundColor Cyan }
function Warn($m)  { Write-Host $m -ForegroundColor Yellow }
function Ok($m)    { Write-Host $m -ForegroundColor Green }
function Have($c)  { return [bool](Get-Command $c -ErrorAction SilentlyContinue) }
function RefreshPath {
    $env:Path = [Environment]::GetEnvironmentVariable("Path", "Machine") + ";" +
                [Environment]::GetEnvironmentVariable("Path", "User") + ";" +
                "$env:USERPROFILE\.cargo\bin"
}

Info "== NavForensics - installation (Windows) =="
Set-Location -Path $PSScriptRoot

# --- Node.js ---
if (-not (Have node)) {
    Warn "Node.js absent."
    if (Have winget) {
        Info "Installation de Node.js (LTS) via winget..."
        winget install -e --id OpenJS.NodeJS.LTS --accept-source-agreements --accept-package-agreements
        RefreshPath
    } else {
        throw "Node.js introuvable et winget indisponible. Installez Node.js 18+ depuis https://nodejs.org puis relancez."
    }
}
Ok ("Node.js : " + (node --version))

# --- Rust ---
if (-not (Have cargo)) {
    Warn "Rust absent."
    if (Have winget) {
        Info "Installation de Rust (rustup) via winget..."
        winget install -e --id Rustlang.Rustup --accept-source-agreements --accept-package-agreements
        RefreshPath
    } else {
        throw "Rust introuvable et winget indisponible. Installez-le depuis https://rustup.rs puis relancez."
    }
}
Ok ("Rust : " + (cargo --version))

# --- Rappels prerequis Tauri ---
Warn "Prerequis Tauri a verifier : Microsoft Edge WebView2 (preinstalle sur Windows 11)"
Warn "et Visual Studio Build Tools avec la charge 'Desktop C++'."

# --- Dependances npm ---
Info "Installation des dependances npm..."
npm install

# --- Icones ---
if (-not (Test-Path "src-tauri/icons/icon.ico")) {
    if (Test-Path "assets/logo.png") {
        Info "Generation des icones a partir de assets/logo.png..."
        npx tauri icon assets/logo.png
    } else {
        Warn "Aucune icone : placez un logo dans assets/logo.png ou des icones dans src-tauri/icons/."
    }
}

switch ($Action) {
    "deps"  { Ok "Dependances installees. Lancez './install.ps1 -Action dev' ou '-Action build'." }
    "dev"   { Info "Lancement en mode developpement..."; npm run tauri dev }
    "build" {
        Info "Construction de l'installeur..."
        npm run tauri build
        Ok "Termine. Installeur : src-tauri\target\release\bundle\"
    }
}
