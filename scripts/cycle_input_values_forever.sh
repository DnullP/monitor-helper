#!/bin/sh

set -eu

SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH='' cd -- "$SCRIPT_DIR/.." && pwd)
MONITOR_HELPER=${MONITOR_HELPER:-"$REPO_ROOT/target/debug/monitor-helper"}

display=1
display_id=
display_name=
start=1
delay=10
end=255

usage() {
    cat <<'EOF'
Usage: cycle_input_values_forever.sh [--display N | --id ID | --name TEXT] [--start VALUE] [--end VALUE] [--delay SECONDS]

Continuously cycles monitor input-select values in ascending order.
Starts at the configured value, waits after each attempt, and wraps back to start after reaching end.
Runs until interrupted.

Options:
  --display N         Monitor index passed to monitor-helper. Default: 1
  --id ID             Exact monitor id from monitor-helper list
  --name TEXT         Match monitor by id/model/controller text
  --start VALUE       First input value to try. Default: 1
  --end VALUE         Last input value before wrapping. Default: 255
  --delay SECONDS     Delay between attempts. Default: 10
  -h, --help          Show this help

Environment:
  MONITOR_HELPER      Path to the monitor-helper binary. Default: target/debug/monitor-helper
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
        --start)
            [ "$#" -ge 2 ] || { echo "Missing value for --start" >&2; exit 1; }
            start=$2
            shift 2
            ;;
        --end)
            [ "$#" -ge 2 ] || { echo "Missing value for --end" >&2; exit 1; }
            end=$2
            shift 2
            ;;
        --delay)
            [ "$#" -ge 2 ] || { echo "Missing value for --delay" >&2; exit 1; }
            delay=$2
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

for value in "$display" "$start" "$end" "$delay"; do
    case "$value" in
        ''|*[!0-9]*)
            echo "Display, start, end, and delay must be non-negative integers." >&2
            exit 1
            ;;
    esac
done

if [ "$start" -gt "$end" ]; then
    echo "Start must be less than or equal to end." >&2
    exit 1
fi

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

echo "Cycling input-select values from $start to $end every ${delay}s"
echo "Press Ctrl+C to stop."

attempt=1
current=$start

while :; do
    timestamp=$(date '+%Y-%m-%d %H:%M:%S')
    if output=$(run_helper set input "$current" 2>&1); then
        echo "[$timestamp] attempt=$attempt value=$current status=ok"
        printf '%s\n' "$output"
    else
        echo "[$timestamp] attempt=$attempt value=$current status=error" >&2
        printf '%s\n' "$output" >&2
    fi

    attempt=$((attempt + 1))

    if [ "$current" -ge "$end" ]; then
        current=$start
    else
        current=$((current + 1))
    fi

    sleep "$delay"
done