# V2419QW DDC/CI Handoff

This file records the current investigation state for the Dell V2419QW input-source mapping work so it can be synced to GitHub and resumed later.

## Current Status

- Project binary: `monitor-helper`
- DP readback was verified on Windows before the latest write-side probe
- Input switching engineering is now implemented for `V2419QW`
- Latest write-side probe result on Windows:
  - `monitor-helper set --display 2 input displayport` returned success
  - subsequent `get --display 2 input` failed with an I2C read error
  - practical interpretation: the monitor accepted the input-switch write and then moved away from the currently readable Windows source
- Current operational implication:
  - direct one-way switching is now available
  - automatic temporary switch-and-restore is intentionally blocked for ambiguous packed readback on this model
  - if the monitor is left on another source, manual return from the monitor OSD or another connected device may be required before Windows DDC reads resume

## What Was Verified

### 1. Thunderbolt 4 -> HDMI path was effectively unreadable

When the monitor was connected through the TB4 -> HDMI path, reads broadly failed:

- `input`, `brightness`, `contrast`, `power`, `vcp-version` -> `invalid DDC/CI length`
- `volume` -> `DDC/CI checksum mismatch`
- narrow scan `0x10..0x12` -> `readable=0`

Conclusion: that path behaved as write-mostly or broken for DDC/CI reads.

### 2. Direct HDMI restored readable DDC/CI

After switching to direct HDMI, reads recovered:

- `brightness` -> `100/100`
- `contrast` -> `50/100`
- `volume` -> `50/100`
- `power` -> `0/1`
- `vcp-version` -> `513/65535`

Relevant commands used:

```sh
./target/debug/monitor-helper get --display 1 brightness
./target/debug/monitor-helper get --display 1 contrast
./target/debug/monitor-helper get --display 1 volume
./target/debug/monitor-helper get --display 1 power
./target/debug/monitor-helper get --display 1 vcp-version
./target/debug/monitor-helper scan --display 1 --start 0x10 --end 0x12
```

### 3. Input-source read needed decode help

`src/main.rs` was updated so `get` and `scan` now print extra decode fields for VCP values that appear packed into a `u16`:

- `current_hex`
- `current_high_byte`
- `current_low_byte`
- `decoded_current`
- `decoded_current_source`
- `decoded_current_label`

This was added because `input` returned `768`, which is `0x0300`, not a plain one-byte value.

### 4. Manual HDMI mapping observations

Observed after manually switching the monitor OSD input and then reading `0x60`:

- HDMI1 -> `current=768`, `current_hex=0x0300`, `current_high_byte=3`
- HDMI2 -> `current=768`, `current_hex=0x0300`, `current_high_byte=3`

Representative command:

```sh
./target/debug/monitor-helper get --display 1 input
./target/debug/monitor-helper scan --display 1 --start 0x60 --end 0x60
```

Conclusion so far:

- the monitor reports `input` in a packed form where the high byte appears meaningful
- the generic mapping table is not correct for this model
- the current readback does not distinguish HDMI1 vs HDMI2

### 4.1. macOS write-side DP attempt caused a visible mode change pulse

With the monitor on the macOS direct-HDMI path, this command was sent:

```sh
./target/debug/monitor-helper set --display 1 input 15
```

Observed result:

- software reported success with `set=15` and `set_label=displayport-1`
- the user observed that the picture flashed once

Practical interpretation:

- the monitor almost certainly received the DDC/CI input-switch write
- the display attempted to transition toward DP
- if no active DP signal is present, many monitors briefly blank or pulse and then remain on or fall back to the current visible source
- this is strong evidence that `displayport=15` is a valid write-side code on V2419QW even though readback remains ambiguous

Follow-up check from the same macOS host after the flash:

```sh
./target/debug/monitor-helper list
./target/debug/monitor-helper get --display 1 input
```

Observed result:

- the monitor was still enumerated as `display=1`
- `get --display 1 input` still returned `current=768`, `current_hex=0x0300`, `current_high_byte=3`

Updated interpretation:

- the write triggered a visible mode pulse, but the monitor did not persist on another input from the perspective of the current macOS HDMI path
- if a valid DP signal really was present, either the monitor briefly tested DP and returned, or `15` is accepted syntactically but does not map to a lasting DP selection on this exact state/path

### 4.2. Second macOS `15` write caused complete DDC loss from the HDMI-side host

A later retry from the same macOS host produced a different result:

```sh
./target/debug/monitor-helper set --display 1 input 15
./target/debug/monitor-helper get --display 1 input
```

Observed result:

- the `set` command again reported success with `set=15`
- the immediate follow-up `get` failed with `No DDC/CI-capable external monitors were found.`

Updated practical interpretation:

- this is strong evidence that the second `15` write switched the monitor away from the current macOS HDMI-connected source strongly enough that the host could no longer enumerate or read it
- combined with the Windows-side evidence, `displayport=15` should be treated as a valid write-side switch value for V2419QW
- once the monitor leaves the active source for the controlling host, DDC access from that host may disappear immediately

### 4.3. Additional raw candidate probes on macOS

Further direct writes were tested from the same macOS host:

```sh
./target/debug/monitor-helper set --display 1 input 14
./target/debug/monitor-helper set --display 1 input 15
./target/debug/monitor-helper set --display 1 input 16
./target/debug/monitor-helper set --display 1 input 17
./target/debug/monitor-helper set --display 1 input 18
./target/debug/monitor-helper set --display 1 input 19
```

Observed results:

- `14` -> physically confirmed as `HDMI2`
- `15` -> physically confirmed as `HDMI2`; software-side follow-up sometimes lost DDC visibility from the current host immediately after the write
- `16` -> physically confirmed as `HDMI2`
- `17` -> physically confirmed as `HDMI1`; one macOS readback attempt after switching left the monitor enumerable but `get --display 1 input` failed with `invalid DDC/CI length`
- `18` -> physically confirmed as `HDMI1`
- `19` -> physically confirmed as `HDMI2`; one macOS readback attempt immediately afterward returned `No DDC/CI-capable external monitors found.` because the host temporarily lost DDC visibility

Interpretation:

- `14`, `15`, `16`, and `19` all map to `HDMI2` on this model
- `17` and `18` both map to `HDMI1` on this model
- `17` can leave the monitor still enumerable but with read-degraded DDC on this host/path
- `15` and `19` can temporarily make the current host lose DDC visibility even though the physical result still lands on `HDMI2`
- the model does not follow the generic MCCS input alias table; multiple adjacent values collapse onto the same HDMI source
- there is currently no physically confirmed DP write value from the tested set `14..19`

### 5. Windows DisplayPort readback matches HDMI

With the V2419QW connected on Windows and correlated through WMI as a DisplayPort-connected panel, `0x60` still returned the same packed value:

- DP -> `current=768`, `current_hex=0x0300`, `current_high_byte=3`

Relevant commands used:

```powershell
cargo build
.\target\debug\monitor-helper list
.\target\debug\monitor-helper profile --display 2
.\target\debug\monitor-helper get --display 2 input
.\target\debug\monitor-helper scan --display 2 --start 0x60 --end 0x60
Get-CimInstance -Namespace root\wmi -ClassName WmiMonitorConnectionParams |
  Select-Object InstanceName, VideoOutputTechnology
Get-CimInstance -Namespace root\wmi -ClassName WmiMonitorID |
  ForEach-Object {
    [PSCustomObject]@{
      InstanceName = $_.InstanceName
      UserFriendlyName = ([System.Text.Encoding]::ASCII.GetString($_.UserFriendlyName) -replace "`0", "").Trim()
    }
  }
```

Conclusion:

- DP and HDMI both read back as `0x0300`
- this monitor does not expose a useful per-input distinction through VCP `0x60`
- the tool should treat `input` readback on V2419QW as ambiguous instead of mapping `3` to a generic source label such as `dvi-1`
- Windows `scan --display 2 --start 0x60 --end 0x60` now reports `readable=1` with the same raw value once the internal probe path targets the resolved display index

### 6. Windows write-side switching was confirmed

After the V2419QW-specific write aliases were added, a Windows-side probe attempt produced this sequence:

- `monitor-helper set --display 2 input displayport` -> success
- the next write/read attempt failed with an I2C transport error
- `monitor-helper list` then degraded back to generic monitor identities
- `monitor-helper get --display 2 input` failed with a receive error from the I2C bus

Conclusion:

- the monitor accepted at least one input-switch write value
- once switched away from the active Windows-connected source, DDC for that display is no longer reachable from this host
- the safe product behavior is therefore:
  - support direct one-way switching
  - refuse automatic restore when the current input readback is ambiguous

### 7. `hdmi-1=17` was visually confirmed

After returning the display to the Windows-connected DP source, a direct command was sent:

```powershell
.\target\debug\monitor-helper set --display 2 input hdmi-1
```

Observed result:

- the monitor switched to HDMI1
- the command reported `set=17` and `set_label=hdmi-1`
- a follow-up `get --display 2 input` immediately failed with an I2C receive error, consistent with the display having switched away from the current Windows source

Confirmed mapping so far:

- `hdmi-1 = 17` confirmed
- `hdmi-2 = 15` confirmed
- `18` is not HDMI2 under the tested Windows path

### 8. `18` also switched to HDMI1

After returning the display to the Windows-connected DP source, another direct command was sent:

```powershell
.\target\debug\monitor-helper set --display 2 input hdmi-2
```

Observed result:

- the command reported `set=18` and `set_label=hdmi-2`
- visually, the monitor still switched to HDMI1 rather than HDMI2
- a follow-up `get --display 2 input` again failed with an I2C transport error, consistent with the display having switched away from the current Windows source

Updated interpretation:

- `17` reliably maps to HDMI1
- `15` reliably maps to HDMI2
- `18` must not be exposed as confirmed HDMI2 on this model
- for now `18` should be treated as an unconfirmed alternate source code rather than a named duplicate-or-alternate HDMI1-related source code

### 9. `15` was visually confirmed as HDMI2

After returning the display to the Windows-connected source again, a direct command was sent:

```powershell
.\target\debug\monitor-helper set --display 2 input 15
```

Observed result:

- the command reported `set=15` and `set_label=displayport` at the time of the test
- visually, the monitor switched to HDMI2
- a follow-up `get --display 2 input` again failed with an I2C transport error, consistent with the display having switched away from the current Windows source

Updated interpretation:

- `15` is the confirmed HDMI2 switch value for this model
- the earlier `displayport` alias was incorrect and should be removed
- no confirmed DisplayPort switch value is currently known for the tested Windows-connected path

## Code Changes Already Made

### `src/main.rs`

- added decode logic for VCP read values that may be encoded in the high byte
- `get` output now includes raw hex and byte-split fields
- `scan` output now includes the same decode hints via the internal probe path
- refreshed monitor metadata before profile detection in read/write/probe paths so `V2419QW` auto-detection works consistently outside the `profile` command
- changed `scan`'s internal probe selector to reuse the resolved display index instead of a non-unique Windows monitor id
- added structured writeback fields for scripts and operators:
  - `writeback_value`
  - `writeback_source`
  - `writeback_label`
  - `writeback_safe`
  - `writeback_reason`
- packed values like `0x0300` now still expose `decoded_current=3`, but the tool marks the writeback as unsafe on V2419QW because the readback is ambiguous

### `src/profile.rs`

- added a `V2419QW`-specific profile
- left `input` without a source-label table so packed `0x0300` reads remain visible but are not falsely labeled as `dvi-1`
- added separate write-side aliases for `V2419QW input`:
  - `hdmi-1` -> `17`
  - `hdmi-2` -> `15`
  - `source-18` -> `18`
- added unit coverage to keep read labels and write aliases separate

### `scripts/probe_input_values.sh`

- added a helper script to brute-force `input` values across common or full ranges
- useful for write-side probing, but it cannot prove a switch unless the user visually confirms it
- now stops after DDC loss instead of continuing through a cascade of failures

### `scripts/probe_input_values.ps1`

- added a Windows-native probe helper for iterating candidate input values or aliases
- responds to `-?` with help instead of running the probe
- stops once DDC is lost after a switch attempt

### `build.ps1`

- added `switch-input` for direct one-way input switching
- added `probe-inputs` as a Windows-native probe entry point
- `switch-dp` now refuses to run when safe restoration is impossible on the current monitor state
- fixed the top-level argument parser so forwarded subcommand flags like `--target hdmi-1` no longer collide with the script's own first positional parameter

### `scripts/switch_to_hdmi1_and_restore.ps1` and `scripts/switch_to_dp_and_restore.sh`

- both scripts now refuse to run when `writeback_safe=false`
- this prevents unsafe "temporary switch and restore" flows on V2419QW

### `Makefile`

- added `make probe-inputs`

## Pending Work

The core switching plumbing is in place.

### Remaining follow-up

- manually return the monitor to the Windows-connected source if DDC is currently unavailable
- verify the exact physical mapping for the write-side aliases on this model:
  - `hdmi-1=17` is already confirmed
  - `hdmi-2=15` is already confirmed
  - `source-18=18`
- find the actual DisplayPort switch value, which is still unknown
- if desired, extend alias coverage after confirming additional ports such as USB-C

### Useful verification commands

```powershell
.\target\debug\monitor-helper profile --display 2
.\target\debug\monitor-helper get --display 2 input
.\build.ps1 switch-input '--display' '2' '--target' 'hdmi-2'
.\build.ps1 switch-input '--display' '2' '--target' 'source-18'
.\build.ps1 probe-inputs '--display' '2' '--mode' 'common' '--delay' '3'
```

### Goal

Provide reliable direct input switching for this model without pretending that ambiguous packed readback can be restored safely.

The monitor appears unable to expose useful per-input distinction through `0x60`, so the tool should expose a write-capable but read-ambiguous input model for this display.

## Capabilities String Findings

The project now has a `capabilities` command:

```sh
./target/debug/monitor-helper capabilities --display 1
```

On macOS with the V2419QW visible, it returned:

```text
capabilities_raw=(vcp(02 04 05 08 0B 0C 10 12 14(05 06 08 0B) 16 18 1A 60(0F 11) 62 8D(01 02)A8 AC AE B6 C6 C8 C9 D6(04) DF)prot(monitor)type(LCD)cmds(01 02 03 07 0C F3)mccs_ver(2.1)asset_eep(64)mpu_ver(001)model(V2419QW)mswhql(1))
```

Key parsed fields:

- `protocol=monitor`
- `type=lcd`
- `model=V2419QW`
- `mccs_version=2.1`
- `vcp 0x60 values: 0x0F 0x11`

Interpretation:

- the monitor's own advertised MCCS capabilities say `input (0x60)` supports only two values: `0x0F` and `0x11`
- in decimal, those are `15` and `17`
- this strongly suggests the monitor officially exposes only two logical input selections through MCCS, despite other adjacent values sometimes producing HDMI-side behavior in practice
- the most likely interpretation is now:
  - `15` = one logical source
  - `17` = the other logical source
- because physical testing showed many neighboring values collapsing back onto HDMI1 or HDMI2, Dell is likely aliasing or normalizing multiple writes internally while only advertising two real MCCS input states
- this makes `15` the strongest DP candidate again from the capability-string perspective, while `17` remains the strongest HDMI candidate

Confirmed direct-switch values at this point:

- `hdmi-1 -> 17`
- `hdmi-2 -> 15`
- `source-18 -> 18` remains unconfirmed beyond the observation that it also landed on HDMI1 during testing

## Suggested Next Commands For The Next Agent

```sh
cargo build
.\target\debug\monitor-helper list
.\target\debug\monitor-helper profile --display 2
.\target\debug\monitor-helper get --display 2 input
.\build.ps1 switch-input '--display' '2' '--target' 'hdmi-2'
```