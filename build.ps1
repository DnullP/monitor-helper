param(
    [Parameter(Position = 0)]
    [string]$Target = 'help',

    [Parameter(Position = 1, ValueFromRemainingArguments = $true)]
    [string[]]$CommandArgs = @()
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:RepoRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$script:Cargo = if ($env:CARGO) { $env:CARGO } else { 'cargo' }
$script:AppName = 'monitor-helper'

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

function Get-BinaryPath {
    if ($env:MONITOR_HELPER) {
        return $env:MONITOR_HELPER
    }

    return Join-Path $script:RepoRoot "target\debug\$($script:AppName).exe"
}

function Show-Help {
    @'
Targets:
  .\build.ps1 build                     Build debug binary
  .\build.ps1 release                   Build release binary
    .\build.ps1 run 'list'                Run with custom arguments
  .\build.ps1 list                      List detected monitors
    .\build.ps1 get '--display' '1' 'brightness'
    .\build.ps1 set '--display' '1' 'input' 'hdmi'
    .\build.ps1 profile '--display' '1'
    .\build.ps1 scan '--display' '1' '--start' '0x00' '--end' '0xFF'
    .\build.ps1 switch-dp '--display' '1' '--target' 'dp1' '--duration' '10'
  .\build.ps1 check                     Run cargo check
  .\build.ps1 fmt                       Format code
  .\build.ps1 lint                      Run clippy with warnings denied
  .\build.ps1 test                      Run tests
  .\build.ps1 clean                     Remove build artifacts

Environment:
  CARGO             Cargo executable to use. Default: cargo
  MONITOR_HELPER    Path to monitor-helper.exe. Default: target\debug\monitor-helper.exe
    MONITOR_PROFILE   Optional profile override such as dell-p2711v when Windows only exposes Generic PnP Monitor
'@ | Write-Host
}

function Normalize-RemainingArgs {
    param([string[]]$Arguments)

    if ($Arguments.Count -gt 0 -and $Arguments[0] -eq '--') {
        if ($Arguments.Count -eq 1) {
            return @()
        }

        return $Arguments[1..($Arguments.Count - 1)]
    }

    return $Arguments
}

function Get-SwitchDpOptions {
    param([string[]]$Arguments)

    $options = [ordered]@{
        Display  = 1
        Target   = 'dp2'
        Duration = 10
    }

    $index = 0
    while ($index -lt $Arguments.Count) {
        $argument = $Arguments[$index]

        switch ($argument) {
            '--display' {
                if ($index + 1 -ge $Arguments.Count) {
                    throw 'Missing value for --display'
                }

                $options.Display = $Arguments[$index + 1]
                $index += 2
            }
            '--target' {
                if ($index + 1 -ge $Arguments.Count) {
                    throw 'Missing value for --target'
                }

                $options.Target = $Arguments[$index + 1]
                $index += 2
            }
            '--duration' {
                if ($index + 1 -ge $Arguments.Count) {
                    throw 'Missing value for --duration'
                }

                $options.Duration = $Arguments[$index + 1]
                $index += 2
            }
            '-h' {
                Show-SwitchDpHelp
                exit 0
            }
            '--help' {
                Show-SwitchDpHelp
                exit 0
            }
            default {
                throw "Unknown argument: $argument"
            }
        }
    }

    return $options
}

function Show-SwitchDpHelp {
    @'
Usage: .\build.ps1 switch-dp '--display' 'N' '--target' 'dp1|dp2' '--duration' 'SECONDS'

Temporarily switches the monitor input to DisplayPort and restores the original input after a delay.

Options:
  --display N           Monitor index passed to monitor-helper. Default: 1
  --target dp1|dp2      Target DisplayPort connector. Default: dp2
  --duration SECONDS    Seconds to wait before restoring the original input. Default: 10
  -h, --help            Show this help
'@ | Write-Host
}

function Invoke-SwitchDp {
    param([string[]]$Arguments)

    $options = Get-SwitchDpOptions -Arguments $Arguments

    if ($options.Duration -notmatch '^[0-9]+$') {
        throw 'Duration must be a non-negative integer.'
    }

    $targetInfo = switch ($options.Target.ToLowerInvariant()) {
        'dp1' { @{ Code = '15'; Label = 'dp1' } }
        'displayport-1' { @{ Code = '15'; Label = 'dp1' } }
        'dp2' { @{ Code = '16'; Label = 'dp2' } }
        'displayport-2' { @{ Code = '16'; Label = 'dp2' } }
        default { throw "Unsupported DP target: $($options.Target). Use dp1 or dp2." }
    }

    $binaryPath = Get-BinaryPath
    if (-not (Test-Path -LiteralPath $binaryPath)) {
        throw "monitor-helper binary not found at: $binaryPath`nRun '.\build.ps1 build' first or set MONITOR_HELPER to the correct path."
    }

    $currentOutput = & $binaryPath 'get' '--display' $options.Display 'input'
    $exitCode = $LASTEXITCODE
    if ($exitCode -ne 0) {
        throw "Failed to read current input from display $($options.Display)."
    }

    $currentLine = $currentOutput | Where-Object { $_ -match '^current=' } | Select-Object -First 1
    if (-not $currentLine) {
        throw "Failed to parse the current input value.`n$($currentOutput -join [Environment]::NewLine)"
    }

    $currentCode = $currentLine.Substring('current='.Length)
    if ($currentCode -notmatch '^[0-9]+$') {
        throw "Failed to parse the current input value.`n$($currentOutput -join [Environment]::NewLine)"
    }

    $restored = $false
    try {
        Write-Host "Current input VCP value: $currentCode"
        Write-Host "Switching display $($options.Display) to $($targetInfo.Label) (VCP value $($targetInfo.Code)) for $($options.Duration)s"
        Invoke-External -FilePath $binaryPath -Arguments @('set', '--display', $options.Display, 'input', $targetInfo.Code)
        Start-Sleep -Seconds ([int]$options.Duration)
    }
    finally {
        if (-not $restored) {
            $restored = $true
            Write-Host "Restoring input to VCP value $currentCode"
            Invoke-External -FilePath $binaryPath -Arguments @('set', '--display', $options.Display, 'input', $currentCode)
            Write-Host 'Input restored.'
        }
    }
}

$normalizedArgs = Normalize-RemainingArgs -Arguments $CommandArgs

Push-Location $script:RepoRoot
try {
    switch ($Target.ToLowerInvariant()) {
        'help' { Show-Help }
        'build' { Invoke-External -FilePath $script:Cargo -Arguments @('build') }
        'release' { Invoke-External -FilePath $script:Cargo -Arguments @('build', '--release') }
        'run' { Invoke-External -FilePath $script:Cargo -Arguments (@('run', '--') + $normalizedArgs) }
        'list' { Invoke-External -FilePath $script:Cargo -Arguments @('run', '--', 'list') }
        'get' { Invoke-External -FilePath $script:Cargo -Arguments (@('run', '--', 'get') + $normalizedArgs) }
        'set' { Invoke-External -FilePath $script:Cargo -Arguments (@('run', '--', 'set') + $normalizedArgs) }
        'profile' { Invoke-External -FilePath $script:Cargo -Arguments (@('run', '--', 'profile') + $normalizedArgs) }
        'scan' { Invoke-External -FilePath $script:Cargo -Arguments (@('run', '--', 'scan') + $normalizedArgs) }
        'switch-dp' { Invoke-SwitchDp -Arguments $normalizedArgs }
        'check' { Invoke-External -FilePath $script:Cargo -Arguments @('check') }
        'fmt' { Invoke-External -FilePath $script:Cargo -Arguments @('fmt') }
        'lint' { Invoke-External -FilePath $script:Cargo -Arguments @('clippy', '--all-targets', '--', '-D', 'warnings') }
        'test' { Invoke-External -FilePath $script:Cargo -Arguments @('test') }
        'clean' { Invoke-External -FilePath $script:Cargo -Arguments @('clean') }
        default {
            throw "Unknown target: $Target`nRun '.\build.ps1 help' to see the supported targets."
        }
    }
}
finally {
    Pop-Location
}
