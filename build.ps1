param(
    [Parameter(Position = 0)]
    [string]$Action = 'help',

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
        .\build.ps1 set '--display' '2' 'input' 'hdmi-2'
    .\build.ps1 profile '--display' '1'
    .\build.ps1 scan '--display' '1' '--start' '0x00' '--end' '0xFF'
        .\build.ps1 switch-input '--display' '2' '--target' 'displayport'
        .\build.ps1 probe-inputs '--display' '2' '--mode' 'common' '--delay' '3'
    .\build.ps1 switch-dp '--display' '1' '--target' 'dp1' '--duration' '10'
  .\build.ps1 check                     Run cargo check
  .\build.ps1 fmt                       Format code
  .\build.ps1 lint                      Run clippy with warnings denied
  .\build.ps1 test                      Run tests
  .\build.ps1 clean                     Remove build artifacts

Environment:
  CARGO             Cargo executable to use. Default: cargo
  MONITOR_HELPER    Path to monitor-helper.exe. Default: target\debug\monitor-helper.exe
'@ | Write-Host
}

function Get-CurrentInputWritebackInfo {
    param(
        [Parameter(Mandatory = $true)]
        [string]$BinaryPath,

        [Parameter(Mandatory = $true)]
        [string]$Display
    )

    $currentOutput = & $BinaryPath 'get' '--display' $Display 'input'
    $exitCode = $LASTEXITCODE
    if ($exitCode -ne 0) {
        throw "Failed to read current input from display $Display."
    }

    $writebackValueLine = $currentOutput | Where-Object { $_ -match '^writeback_value=' } | Select-Object -First 1
    $writebackSafeLine = $currentOutput | Where-Object { $_ -match '^writeback_safe=' } | Select-Object -First 1
    $writebackReasonLine = $currentOutput | Where-Object { $_ -match '^writeback_reason=' } | Select-Object -First 1

    $writebackValue = $null
    if ($writebackValueLine) {
        $writebackValue = $writebackValueLine.Substring('writeback_value='.Length)
    }

    $writebackSafe = $false
    if ($writebackSafeLine) {
        $writebackSafe = $writebackSafeLine.Substring('writeback_safe='.Length).ToLowerInvariant() -eq 'true'
    }

    $writebackReason = $null
    if ($writebackReasonLine) {
        $writebackReason = $writebackReasonLine.Substring('writeback_reason='.Length)
    }

    [PSCustomObject]@{
        Value  = $writebackValue
        Safe   = $writebackSafe
        Reason = $writebackReason
        Output = $currentOutput
    }
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
This command refuses to run when the current input readback is ambiguous and cannot be restored safely.

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

    $writeback = Get-CurrentInputWritebackInfo -BinaryPath $binaryPath -Display $options.Display
    if (-not $writeback.Value -or $writeback.Value -notmatch '^[0-9]+$') {
        throw "Failed to determine a restorable input value.`n$($writeback.Output -join [Environment]::NewLine)"
    }
    if (-not $writeback.Safe) {
        $reasonText = if ($writeback.Reason) { $writeback.Reason } else { 'unknown' }
        throw "Refusing to switch display $($options.Display) with auto-restore because the current input readback is ambiguous ($reasonText). Use '.\build.ps1 switch-input' for a direct one-way switch instead."
    }

    $currentCode = $writeback.Value

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

function Get-SwitchInputOptions {
    param([string[]]$Arguments)

    $options = [ordered]@{
        Display = 1
        Target  = $null
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
            '-h' {
                Show-SwitchInputHelp
                exit 0
            }
            '--help' {
                Show-SwitchInputHelp
                exit 0
            }
            default {
                throw "Unknown argument: $argument"
            }
        }
    }

    if (-not $options.Target) {
        throw 'Missing required --target value.'
    }

    return $options
}

function Show-SwitchInputHelp {
    @'
Usage: .\build.ps1 switch-input '--display' 'N' '--target' 'VALUE'

Switches the monitor input directly without attempting to restore the previous source.

Options:
  --display N           Monitor index passed to monitor-helper. Default: 1
  --target VALUE        Input value or alias such as displayport, hdmi-1, hdmi-2, 15, 17, 18
  -h, --help            Show this help
'@ | Write-Host
}

function Invoke-SwitchInput {
    param([string[]]$Arguments)

    $options = Get-SwitchInputOptions -Arguments $Arguments
    $binaryPath = Get-BinaryPath
    if (-not (Test-Path -LiteralPath $binaryPath)) {
        throw "monitor-helper binary not found at: $binaryPath`nRun '.\build.ps1 build' first or set MONITOR_HELPER to the correct path."
    }

    Invoke-External -FilePath $binaryPath -Arguments @('set', '--display', $options.Display, 'input', $options.Target)
}

$normalizedArgs = Normalize-RemainingArgs -Arguments $CommandArgs

Push-Location $script:RepoRoot
try {
    switch ($Action.ToLowerInvariant()) {
        'help' { Show-Help }
        'build' { Invoke-External -FilePath $script:Cargo -Arguments @('build') }
        'release' { Invoke-External -FilePath $script:Cargo -Arguments @('build', '--release') }
        'run' { Invoke-External -FilePath $script:Cargo -Arguments (@('run', '--') + $normalizedArgs) }
        'list' { Invoke-External -FilePath $script:Cargo -Arguments @('run', '--', 'list') }
        'get' { Invoke-External -FilePath $script:Cargo -Arguments (@('run', '--', 'get') + $normalizedArgs) }
        'set' { Invoke-External -FilePath $script:Cargo -Arguments (@('run', '--', 'set') + $normalizedArgs) }
        'profile' { Invoke-External -FilePath $script:Cargo -Arguments (@('run', '--', 'profile') + $normalizedArgs) }
        'scan' { Invoke-External -FilePath $script:Cargo -Arguments (@('run', '--', 'scan') + $normalizedArgs) }
        'switch-input' { Invoke-SwitchInput -Arguments $normalizedArgs }
        'probe-inputs' { Invoke-External -FilePath 'powershell' -Arguments @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', (Join-Path $script:RepoRoot 'scripts\probe_input_values.ps1')) + $normalizedArgs }
        'switch-dp' { Invoke-SwitchDp -Arguments $normalizedArgs }
        'check' { Invoke-External -FilePath $script:Cargo -Arguments @('check') }
        'fmt' { Invoke-External -FilePath $script:Cargo -Arguments @('fmt') }
        'lint' { Invoke-External -FilePath $script:Cargo -Arguments @('clippy', '--all-targets', '--', '-D', 'warnings') }
        'test' { Invoke-External -FilePath $script:Cargo -Arguments @('test') }
        'clean' { Invoke-External -FilePath $script:Cargo -Arguments @('clean') }
        default {
            throw "Unknown target: $Action`nRun '.\build.ps1 help' to see the supported targets."
        }
    }
}
finally {
    Pop-Location
}
