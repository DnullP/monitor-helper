#!/bin/sh

set -eu

SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH='' cd -- "$SCRIPT_DIR/.." && pwd)
MONITOR_HELPER=${MONITOR_HELPER:-"$REPO_ROOT/target/debug/monitor-helper"}

display=2
display_id=
display_name=
delay=3
mode=common
values=

usage() {
    cat <<'EOF'
Usage: probe_input_values.sh [--display N | --id ID | --name TEXT] [--mode common|full] [--delay SECONDS] [--values "15 16 17 18"]

Try a sequence of VCP input-select values on a monitor and print each attempted value.
Use this to visually determine which input values actually switch the display.

Options:
  --display N           Monitor index passed to monitor-helper. Default: 2
  --id ID               Exact monitor id from monitor-helper list
  --name TEXT           Match monitor by id/model/controller text
  --mode common|full    common: likely input values, full: standard 1..27. Default: common
  --delay SECONDS       Seconds to wait after each attempt. Default: 3
  --values "..."        Explicit space-separated values. Overrides --mode
  -h, --help            Show this help

Modes:
  common  1 3 15 16 17 18 27
  full    1 through 27

Environment:
  MONITOR_HELPER        Path to the monitor-helper binary. Default: target/debug/monitor-helper
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --display)
            [ "$#" -ge 2 ] || { echo "Missing value for --display" >&2; exit 1; }
            display=$2
            shift 2
            ;;
        --id)
            [ "$#" -ge 2 ] || { echo "Missing value for --id" >&2; exit 1; }
            display_id=$2
            shift 2
            ;;
        --name)
            [ "$#" -ge 2 ] || { echo "Missing value for --name" >&2; exit 1; }
            display_name=$2
            shift 2
            ;;
        --mode)
            [ "$#" -ge 2 ] || { echo "Missing value for --mode" >&2; exit 1; }
            mode=$2
            shift 2
            ;;
        --delay)
            [ "$#" -ge 2 ] || { echo "Missing value for --delay" >&2; exit 1; }
            delay=$2
            shift 2
            ;;
        --values)
            [ "$#" -ge 2 ] || { echo "Missing value for --values" >&2; exit 1; }
            values=$2
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown argument: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

case "$delay" in
    ''|*[!0-9]*)
        echo "Delay must be a non-negative integer." >&2
        exit 1
        ;;
esac

case "$display" in
    ''|*[!0-9]*)
        echo "Display must be a positive integer." >&2
        exit 1
        ;;
esac

if [ ! -x "$MONITOR_HELPER" ]; then
    echo "monitor-helper binary not found at: $MONITOR_HELPER" >&2
    echo "Run 'cargo build' first or set MONITOR_HELPER to the correct path." >&2
    exit 1
fi

run_helper() {
    if [ -n "$display_id" ]; then
        "$MONITOR_HELPER" "$@" --id "$display_id"
    elif [ -n "$display_name" ]; then
        "$MONITOR_HELPER" "$@" --name "$display_name"
    else
        "$MONITOR_HELPER" "$@" --display "$display"
    fi
}

if [ -z "$values" ]; then
    case "$mode" in
        common)
            values="1 3 15 16 17 18 27"
            ;;
        full)
            values="1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27"
            ;;
        *)
            echo "Unsupported mode: $mode. Use common or full." >&2
            exit 1
            ;;
    esac
fi

echo "Probing input-select values on the selected monitor"
echo "values=$values"
echo "delay=${delay}s"
echo "Observe the target display and note which values cause a real input switch."

attempt=1
for value in $values; do
    timestamp=$(date '+%Y-%m-%d %H:%M:%S')
    echo "[$timestamp] attempt=$attempt value=$value"

    if output=$(run_helper set input "$value" 2>&1); then
        printf '%s\n' "$output"
    else
        echo "set_failed=true" >&2
        printf '%s\n' "$output" >&2
        echo "Stopping probe because DDC access failed after a switch attempt. If the monitor changed away from this host, this is expected." >&2
        break
    fi

    attempt=$((attempt + 1))
    sleep "$delay"
done

echo "Probe finished."