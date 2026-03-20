#!/bin/sh

set -eu

SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH='' cd -- "$SCRIPT_DIR/.." && pwd)
MONITOR_HELPER=${MONITOR_HELPER:-"$REPO_ROOT/target/debug/monitor-helper"}

display=1
interval=5

usage() {
    cat <<'EOF'
Usage: retry_switch_to_hdmi2.sh [--display N] [--interval SECONDS]

Repeatedly attempts to switch the monitor input to HDMI-2 every few seconds.
This runs until interrupted.

Options:
  --display N            Monitor index passed to monitor-helper. Default: 1
  --interval SECONDS     Delay between attempts. Default: 5
  -h, --help             Show this help

Environment:
  MONITOR_HELPER         Path to the monitor-helper binary. Default: target/debug/monitor-helper
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --display)
            [ "$#" -ge 2 ] || { echo "Missing value for --display" >&2; exit 1; }
            display=$2
            shift 2
            ;;
        --interval)
            [ "$#" -ge 2 ] || { echo "Missing value for --interval" >&2; exit 1; }
            interval=$2
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

case "$interval" in
    ''|*[!0-9]*)
        echo "Interval must be a non-negative integer." >&2
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

echo "Attempting to switch display $display to HDMI-2 every ${interval}s"
echo "Press Ctrl+C to stop."

attempt=1

while :; do
    timestamp=$(date '+%Y-%m-%d %H:%M:%S')
    if output=$($MONITOR_HELPER set --display "$display" input 18 2>&1); then
        echo "[$timestamp] attempt=$attempt status=ok target=hdmi2"
    else
        echo "[$timestamp] attempt=$attempt status=error target=hdmi2" >&2
        printf '%s\n' "$output" >&2
    fi

    attempt=$((attempt + 1))
    sleep "$interval"
done