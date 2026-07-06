# Validate CrabBridge + Codex configuration on Windows (crabridge-cli setup --docker).
# Usage: powershell -ExecutionPolicy Bypass -File scripts/docker-check-windows.ps1
param(
    [string]$Config = "",
    [string]$ConfigDir = "",
    [string]$BindAddr = "",
    [string]$Provider = "",
    [string]$Providers = "",
    [switch]$AllProviders,
    [switch]$Help,
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$ExtraArgs
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
. (Join-Path $ScriptDir "lib/crabridge-cli-common.ps1")

Invoke-CrabbridgeDockerCheckFlow `
    -Platform "windows" `
    -ScriptDir $ScriptDir `
    -Config $Config `
    -ConfigDir $ConfigDir `
    -BindAddr $BindAddr `
    -Provider $Provider `
    -Providers $Providers `
    -AllProviders:$AllProviders `
    -Help:$Help `
    -ExtraArgs $ExtraArgs
