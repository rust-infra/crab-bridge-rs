# Install CrabBridge on Windows.
# Usage: powershell -ExecutionPolicy Bypass -File scripts/install-windows.ps1
param(
    [string]$Prefix = "$env:LOCALAPPDATA\crabbridge",
    [string]$ConfigDir = "$env:APPDATA\crabbridge",
    [string]$BuildDir = "",
    [switch]$SkipBuild,
    [switch]$Help
)

$ErrorActionPreference = "Stop"
$BinaryName = "crabridge.exe"
$CliBinaryName = "crabridge-cli.exe"

function Show-Usage {
    Write-Host @"
Usage: install-windows.ps1 [OPTIONS]

Build and install CrabBridge on Windows.

Options:
  -Prefix DIR        Install directory (default: %LOCALAPPDATA%\crabbridge)
  -ConfigDir DIR     Config directory (default: %APPDATA%\crabbridge)
  -BuildDir DIR      Source directory with Cargo.toml (default: repo root)
  -SkipBuild         Skip cargo build (install existing release binary)
  -Help              Show this help

Environment:
  DEEPSEEK_API_KEY   If set, written into the generated config.toml

Examples:
  powershell -ExecutionPolicy Bypass -File scripts/install-windows.ps1
  `$env:DEEPSEEK_API_KEY = "sk-..." ; .\scripts\install-windows.ps1
"@
}

if ($Help) {
    Show-Usage
    exit 0
}

function Write-Step([string]$Message) {
    Write-Host "==> $Message"
}

function Ensure-Cargo {
    if (Get-Command cargo -ErrorAction SilentlyContinue) {
        return
    }
    throw "Rust toolchain not found. Install from https://rustup.rs/ then reopen your terminal."
}

function Build-Binary([string]$SourceDir) {
    if (-not (Test-Path (Join-Path $SourceDir "Cargo.toml"))) {
        throw "No Cargo.toml in BUILD_DIR=$SourceDir"
    }
    Write-Step "Building release binary in $SourceDir"
    Push-Location $SourceDir
    try {
        cargo build --release --bin $BinaryName
        cargo build --release --bin "crabridge-cli" --no-default-features
    } finally {
        Pop-Location
    }
}

function Install-Binary([string]$SourceDir, [string]$BinDir) {
    $src = Join-Path $SourceDir "target\release\$BinaryName"
    $cliSrc = Join-Path $SourceDir "target\release\$CliBinaryName"
    if (-not (Test-Path $src)) {
        throw "Binary not found at $src. Run build first."
    }
    if (-not (Test-Path $cliSrc)) {
        throw "Binary not found at $cliSrc. Run build first."
    }
    New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
    Copy-Item -Force $src (Join-Path $BinDir $BinaryName)
    Copy-Item -Force $cliSrc (Join-Path $BinDir $CliBinaryName)
    Write-Step "Installed $(Join-Path $BinDir $BinaryName)"
    Write-Step "Installed $(Join-Path $BinDir $CliBinaryName)"
}

function Install-Config([string]$RepoRoot, [string]$TargetConfigDir) {
    New-Item -ItemType Directory -Force -Path $TargetConfigDir | Out-Null
    New-Item -ItemType Directory -Force -Path (Join-Path $TargetConfigDir "data") | Out-Null

    $configFile = Join-Path $TargetConfigDir "config.toml"
    if (Test-Path $configFile) {
        Write-Warning "Config already exists: $configFile (unchanged)"
        return
    }

    $example = Join-Path $RepoRoot "crabbridge.example.toml"
    if (Test-Path $example) {
        Copy-Item $example $configFile
    } else {
        @"
default_provider = "deepseek"

[providers.deepseek]
api_key = "sk-your-api-key-here"
base_url = "https://api.deepseek.com/v1"
model = "deepseek-v4-pro"

[providers.kimi]
api_key = "sk-your-kimi-code-key"
base_url = "https://api.kimi.com/coding/v1"
model = "kimi-for-coding"

[server]
bind_addr = "127.0.0.1:11435"
log_level = "info"

[session]
db = "data/crabbridge.db"
memory_only = false
"@ | Set-Content -Path $configFile -Encoding UTF8
    }

    $content = Get-Content $configFile -Raw
    if ($env:DEEPSEEK_API_KEY) {
        $content = $content -replace '(?m)^api_key = .*', "api_key = `"$($env:DEEPSEEK_API_KEY)`""
    }
    Set-Content -Path $configFile -Value $content.TrimEnd() -Encoding UTF8

    Write-Step "Created config: $configFile"
}

function Ensure-UserPath([string]$BinDir) {
    $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
    if ($userPath -split ";" | Where-Object { $_ -eq $BinDir }) {
        return $false
    }
    $newPath = if ($userPath) { "$userPath;$BinDir" } else { $BinDir }
    [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
    $env:Path = "$env:Path;$BinDir"
    Write-Step "Added $BinDir to user PATH (restart terminal if needed)"
    return $true
}

function Show-NextSteps([string]$BinDir, [string]$TargetConfigDir, [string]$ExePath) {
    Write-Host @"

CrabBridge installed successfully.

  Binary:  $ExePath
           $(Join-Path (Split-Path $ExePath) $CliBinaryName)
  Config:  $(Join-Path $TargetConfigDir "config.toml")

Next steps:
  1. Edit $(Join-Path $TargetConfigDir "config.toml") and set upstream.api_key
  2. Start the bridge:
       $ExePath serve
  3. Configure Codex:
       $(Join-Path (Split-Path $ExePath) $CliBinaryName) setup
  4. Test:
       $ExePath prompt "Hello"
"@
}

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Split-Path -Parent $ScriptDir
if (-not $BuildDir) {
    $BuildDir = $RepoRoot
}

$BinDir = Join-Path $Prefix "bin"
$ExePath = Join-Path $BinDir $BinaryName

Write-Step "Installing CrabBridge on Windows"
Ensure-Cargo

if (-not $SkipBuild) {
    Build-Binary $BuildDir
}

Install-Binary $BuildDir $BinDir
Install-Config $RepoRoot $ConfigDir
Ensure-UserPath $BinDir | Out-Null
Show-NextSteps $BinDir $ConfigDir $ExePath
