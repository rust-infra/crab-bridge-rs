# Configure Codex + CrabBridge on Windows via crabridge-cli setup.
# Usage: powershell -ExecutionPolicy Bypass -File scripts/setup-windows.ps1
param(
    [switch]$Check,
    [string]$Config = "",
    [string]$ConfigDir = "",
    [string]$BindAddr = "",
    [string]$Provider = "",
    [string]$Providers = "",
    [switch]$AllProviders,
    [switch]$CodexOnly,
    [switch]$ForceConfig,
    [switch]$Help,
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$ExtraArgs
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
. (Join-Path $ScriptDir "lib/crabridge-cli-common.ps1")

Invoke-CrabbridgeSetupFlow `
    -Platform "windows" `
    -ScriptDir $ScriptDir `
    -CheckOnly:$Check `
    -Config $Config `
    -ConfigDir $ConfigDir `
    -BindAddr $BindAddr `
    -Provider $Provider `
    -Providers $Providers `
    -AllProviders:$AllProviders `
    -CodexOnly:$CodexOnly `
    -ForceConfig:$ForceConfig `
    -Help:$Help `
    -ExtraArgs $ExtraArgs
