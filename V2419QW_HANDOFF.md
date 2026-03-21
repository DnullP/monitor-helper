# V2419QW DDC/CI Handoff

This file records the current investigation state for the Dell V2419QW input-source mapping work so it can be synced to GitHub and resumed later.

## Current Status

- Project binary: `monitor-helper`
- Current monitor state during the latest successful read tests: direct HDMI connection on macOS
- Current enumerated monitor after direct HDMI reconnect:
  - `display=1`
  - `description=macos:V2419QW V2419QW`
  - `controller=Generic DDC/CI`

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

## Code Changes Already Made

### `src/main.rs`

- added decode logic for VCP read values that may be encoded in the high byte
- `get` output now includes raw hex and byte-split fields
- `scan` output now includes the same decode hints via the internal probe path

### `scripts/probe_input_values.sh`

- added a helper script to brute-force `input` values across common or full ranges
- useful for write-side probing, but it cannot prove a switch unless the user visually confirms it

### `Makefile`

- added `make probe-inputs`

## Pending Work

The next missing data point is the DP readback value for V2419QW.

### Required external step

Switch the monitor to DP from another device or another environment where DP is available.

### Then run

```sh
./target/debug/monitor-helper get --display 1 input
./target/debug/monitor-helper scan --display 1 --start 0x60 --end 0x60
```

### Goal

Capture whether DP returns a value different from HDMI.

If DP returns a distinct high-byte value, the next implementation should be:

1. add a `V2419QW`-specific profile in `src/profile.rs`
2. replace the generic input label mapping for this model
3. treat this monitor as distinguishing at least `HDMI` vs `DP`

If DP also reads back as the same code, then the monitor likely cannot expose useful per-input distinction through `0x60`, and the tool should only present a limited or write-only input model for this display.

## Suggested Next Commands For The Next Agent

```sh
cargo build
./target/debug/monitor-helper list
./target/debug/monitor-helper profile --display 1
./target/debug/monitor-helper get --display 1 input
./target/debug/monitor-helper scan --display 1 --start 0x60 --end 0x60
```