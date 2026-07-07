# Shared helpers for crabridge-cli setup / docker-check wrapper scripts on Windows.

function Get-CrabridgeRepoRoot {
    param([string]$ScriptDir)
    return (Split-Path -Parent $ScriptDir)
}

function Get-CrabbridgeDefaultConfigDir {
    param([string]$ConfigDir = "")
    if ($ConfigDir) { return $ConfigDir }
    return Join-Path $env:APPDATA "crabbridge"
}

function Get-CrabbridgeDefaultConfigFile {
    param([string]$ConfigDir = "")
    return Join-Path (Get-CrabbridgeDefaultConfigDir $ConfigDir) "config.toml"
}

function Get-CrabbridgeDefaultBindAddr {
    param([string]$BindAddr = "")
    if ($BindAddr) { return $BindAddr }
    if ($env:BIND_ADDR) { return $env:BIND_ADDR }
    return "127.0.0.1:11435"
}

function Resolve-CrabridgeCli {
    param([string]$RepoRoot)

    if ($env:CRABRIDGE_CLI -and (Test-Path $env:CRABRIDGE_CLI)) {
        return @{ Mode = "binary"; Path = $env:CRABRIDGE_CLI }
    }

    $candidates = @(
        (Get-Command "crabridge-cli.exe" -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source),
        (Join-Path $env:LOCALAPPDATA "crabbridge\bin\crabridge-cli.exe"),
        (Join-Path $RepoRoot "target\release\crabridge-cli.exe")
    ) | Where-Object { $_ -and (Test-Path $_) }

    if ($candidates.Count -gt 0) {
        return @{ Mode = "binary"; Path = $candidates[0] }
    }

    if ((Test-Path (Join-Path $RepoRoot "Cargo.toml")) -and (Get-Command cargo -ErrorAction SilentlyContinue)) {
        return @{ Mode = "cargo"; Path = $RepoRoot }
    }

    return $null
}

function Invoke-CrabridgeCli {
    param(
        [hashtable]$Cli,
        [string[]]$Args
    )

    if ($Cli.Mode -eq "cargo") {
        Push-Location $Cli.Path
        try {
            & cargo run --quiet --bin crabridge-cli -- @Args
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
        } finally {
            Pop-Location
        }
        return
    }

    & $Cli.Path @Args
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}

function Show-CrabbridgeSetupUsage {
    param([string]$Platform)
    $defaultConfig = Get-CrabbridgeDefaultConfigFile
    $defaultConfigDir = Get-CrabbridgeDefaultConfigDir
    $defaultBind = Get-CrabbridgeDefaultBindAddr
    Write-Host @"
Usage: setup-${Platform}.ps1 [OPTIONS] [-- EXTRA_ARGS...]

Write or refresh Codex + CrabBridge configuration via crabridge-cli setup.

Options:
  -Check              Check configuration only (same as docker-check-${Platform}.ps1)
  -Config FILE        Bridge config path (default: $defaultConfig)
  -ConfigDir DIR      Config directory (default: $defaultConfigDir)
  -BindAddr ADDR      Bridge listen address (default: $defaultBind)
  -Provider SLUG      Single provider preset (deepseek | kimi)
  -Providers LIST     Comma-separated providers (e.g. kimi,deepseek)
  -AllProviders       Configure deepseek + kimi (default when no provider flags)
  -CodexOnly          Skip writing bridge TOML
  -ForceConfig        Overwrite existing bridge config
  -Help               Show this help

Environment:
  CRABRIDGE_CLI        Path to crabridge-cli.exe
  BIND_ADDR            Override bridge bind address
  DEEPSEEK_API_KEY, KIMI_API_KEY, UPSTREAM_API_KEY

Examples:
  powershell -ExecutionPolicy Bypass -File scripts/setup-${Platform}.ps1
  powershell -ExecutionPolicy Bypass -File scripts/setup-${Platform}.ps1 -Check
  powershell -ExecutionPolicy Bypass -File scripts/setup-${Platform}.ps1 -Provider kimi -ForceConfig
"@
}

function Show-CrabbridgeDockerCheckUsage {
    param([string]$Platform)
    $defaultConfig = Get-CrabbridgeDefaultConfigFile
    $defaultConfigDir = Get-CrabbridgeDefaultConfigDir
    $defaultBind = Get-CrabbridgeDefaultBindAddr
    Write-Host @"
Usage: docker-check-${Platform}.ps1 [OPTIONS] [-- EXTRA_ARGS...]

Validate Codex + bridge configuration before or during Docker deployment.
Runs: crabridge-cli setup --docker

Options:
  -Config FILE        Bridge config path (default: $defaultConfig)
  -ConfigDir DIR      Config directory (default: $defaultConfigDir)
  -BindAddr ADDR      Expected bridge listen address (default: $defaultBind)
  -Provider SLUG      Provider slug when config file has no [providers.*] sections
  -Providers LIST     Comma-separated provider slugs to validate
  -AllProviders       Validate deepseek + kimi
  -Help               Show this help

Environment:
  CRABRIDGE_CLI, BIND_ADDR

Examples:
  powershell -ExecutionPolicy Bypass -File scripts/docker-check-${Platform}.ps1
  powershell -ExecutionPolicy Bypass -File scripts/docker-check-${Platform}.ps1 -AllProviders
  powershell -ExecutionPolicy Bypass -File scripts/docker-check-${Platform}.ps1 -Config .\crabbridge.docker.toml
"@
}

function Build-CrabbridgeCliArgs {
    param(
        [string]$ConfigFile,
        [string]$BindAddr,
        [switch]$CheckOnly,
        [switch]$AllProviders,
        [string]$Provider = "",
        [string]$Providers = "",
        [switch]$CodexOnly,
        [switch]$ForceConfig,
        [string[]]$ExtraArgs = @()
    )

    $args = @(
        "-c", $ConfigFile,
        "setup",
        "--bind-addr", $BindAddr
    )

    if ($CheckOnly) { $args += "--docker" }
    if ($AllProviders) {
        $args += "--all-providers"
    } elseif ($Providers) {
        $args += @("--providers", $Providers)
    } elseif ($Provider) {
        $args += @("--provider", $Provider)
    } elseif (-not $CheckOnly) {
        $args += "--all-providers"
    }
    if ($CodexOnly) { $args += "--codex-only" }
    if ($ForceConfig) { $args += "--force-config" }
    if ($ExtraArgs.Count -gt 0) { $args += $ExtraArgs }
    return ,$args
}

function Invoke-CrabbridgeSetupFlow {
    param(
        [string]$Platform,
        [string]$ScriptDir,
        [switch]$CheckOnly,
        [string]$Config = "",
        [string]$ConfigDir = "",
        [string]$BindAddr = "",
        [string]$Provider = "",
        [string]$Providers = "",
        [switch]$AllProviders,
        [switch]$CodexOnly,
        [switch]$ForceConfig,
        [switch]$Help,
        [string[]]$ExtraArgs = @()
    )

    if ($Help) {
        Show-CrabbridgeSetupUsage $Platform
        return
    }

    $repoRoot = Get-CrabbridgeRepoRoot $ScriptDir
    $configFile = if ($Config) { $Config } else { Get-CrabbridgeDefaultConfigFile $ConfigDir }
    $bind = Get-CrabbridgeDefaultBindAddr $BindAddr
    $cli = Resolve-CrabridgeCli $repoRoot
    if (-not $cli) {
        throw "crabridge-cli not found. Install it or set CRABRIDGE_CLI."
    }

    $configParent = Split-Path -Parent $configFile
    if ($configParent) {
        New-Item -ItemType Directory -Force -Path $configParent | Out-Null
    }

    $cliArgs = Build-CrabbridgeCliArgs `
        -ConfigFile $configFile `
        -BindAddr $bind `
        -CheckOnly:$CheckOnly `
        -AllProviders:$AllProviders `
        -Provider $Provider `
        -Providers $Providers `
        -CodexOnly:$CodexOnly `
        -ForceConfig:$ForceConfig `
        -ExtraArgs $ExtraArgs

    if ($CheckOnly) {
        Write-Host "==> Checking CrabBridge configuration ($Platform)"
    } else {
        Write-Host "==> Applying CrabBridge setup ($Platform)"
    }
    Write-Host "    config: $configFile"
    Write-Host "    bind:   $bind"

    Invoke-CrabridgeCli -Cli $cli -Args $cliArgs
}

function Invoke-CrabbridgeDockerCheckFlow {
    param(
        [string]$Platform,
        [string]$ScriptDir,
        [string]$Config = "",
        [string]$ConfigDir = "",
        [string]$BindAddr = "",
        [string]$Provider = "",
        [string]$Providers = "",
        [switch]$AllProviders,
        [switch]$Help,
        [string[]]$ExtraArgs = @()
    )

    if ($Help) {
        Show-CrabbridgeDockerCheckUsage $Platform
        return
    }

    Invoke-CrabbridgeSetupFlow `
        -Platform $Platform `
        -ScriptDir $ScriptDir `
        -CheckOnly `
        -Config $Config `
        -ConfigDir $ConfigDir `
        -BindAddr $BindAddr `
        -Provider $Provider `
        -Providers $Providers `
        -AllProviders:$AllProviders `
        -ExtraArgs $ExtraArgs
}
