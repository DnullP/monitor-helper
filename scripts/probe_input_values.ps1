param(
    [int]$Display = 2,
    [ValidateSet('common', 'full')]
    [string]$Mode = 'common',
    [int]$Delay = 3,
    [string[]]$Values = @(),
    [string]$MonitorHelper = $env:MONITOR_HELPER,
    [Alias('?')]
    [switch]$Help
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$script:RepoRoot = Split-Path -Parent $script:ScriptDir

function Get-BinaryPath {
    if ($MonitorHelper) {
        return $MonitorHelper
    }

    return Join-Path $script:RepoRoot 'target\debug\monitor-helper.exe'
}

function Invoke-External {
    param(
        [Parameter(Mandatory = $true)]
        [string]$FilePath,

        [string[]]$Arguments = @(),

        [switch]$IgnoreExitCode
    )

    & $FilePath @Arguments
    $exitCode = $LASTEXITCODE

    if (-not $IgnoreExitCode -and $exitCode -ne 0) {
        throw "Command failed with exit code ${exitCode}: $FilePath $($Arguments -join ' ')"
    }
}

function Show-Usage {
        @'
Usage: .\scripts\probe_input_values.ps1 [-Display N] [-Mode common|full] [-Delay SECONDS] [-Values VALUE1,VALUE2,...]

Try a sequence of input-select values on a monitor and print each attempted value.
Use this to visually determine which values actually switch the display.

Parameters:
    -Display N         Monitor index passed to monitor-helper. Default: 2
    -Mode common|full  common: likely input values, full: 1 through 27. Default: common
    -Delay SECONDS     Seconds to wait after each attempt. Default: 3
    -Values ...        Explicit values or aliases. Overrides -Mode

Environment:
    MONITOR_HELPER     Path to monitor-helper.exe. Default: target\debug\monitor-helper.exe
'@ | Write-Host
}

if ($Help) {
        Show-Usage
        exit 0
}

if ($Display -lt 1) {
    throw 'Display indices start at 1.'
}

if ($Delay -lt 0) {
    throw 'Delay must be a non-negative integer.'
}

$binaryPath = Get-BinaryPath
if (-not (Test-Path -LiteralPath $binaryPath)) {
    throw "monitor-helper binary not found at: $binaryPath`nRun '.\build.ps1 build' first or set MONITOR_HELPER to the correct path."
}

if ($Values.Count -eq 0) {
    $Values = switch ($Mode) {
        'common' { @('displayport', '15', '16', 'hdmi-1', '17', 'hdmi-2', '18', '27') }
        'full' { 1..27 | ForEach-Object { $_.ToString() } }
    }
}

Write-Host "Probing input-select values on display $Display"
Write-Host "values=$($Values -join ' ')"
Write-Host "delay=${Delay}s"
Write-Host 'Observe the target display and note which values cause a real input switch.'

$attempt = 1
foreach ($value in $Values) {
    $timestamp = Get-Date -Format 'yyyy-MM-dd HH:mm:ss'
    Write-Host "[$timestamp] attempt=$attempt value=$value"

    try {
        Invoke-External -FilePath $binaryPath -Arguments @('set', '--display', $Display, 'input', $value)
    }
    catch {
        Write-Warning $_
        Write-Warning 'Stopping probe because DDC access failed after a switch attempt. If the monitor changed away from this host, this is expected.'
        break
    }

    $attempt += 1
    Start-Sleep -Seconds $Delay
}

Write-Host 'Probe finished.'