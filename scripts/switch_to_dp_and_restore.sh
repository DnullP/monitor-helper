#!/bin/sh

set -eu

SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH='' cd -- "$SCRIPT_DIR/.." && pwd)
MONITOR_HELPER=${MONITOR_HELPER:-"$REPO_ROOT/target/debug/monitor-helper"}

display=1
target=dp2
duration=10

usage() {
    cat <<'EOF'
Usage: switch_to_dp_and_restore.sh [--display N] [--target dp1|dp2] [--duration SECONDS]

Temporarily switches the monitor input to DisplayPort and restores the original input after a delay.

Options:
  --display N           Monitor index passed to monitor-helper. Default: 1
  --target dp1|dp2      Target DisplayPort connector. Default: dp1
  --duration SECONDS    Seconds to wait before restoring the original input. Default: 10
  -h, --help            Show this help

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
        --target)
            [ "$#" -ge 2 ] || { echo "Missing value for --target" >&2; exit 1; }
            target=$2
            shift 2
            ;;
        --duration)
            [ "$#" -ge 2 ] || { echo "Missing value for --duration" >&2; exit 1; }
            duration=$2
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

case "$target" in
    dp1|displayport-1)
        target_code=15
        target_label=dp1
        ;;
    dp2|displayport-2)
        target_code=16
        target_label=dp2
        ;;
    *)
        echo "Unsupported DP target: $target. Use dp1 or dp2." >&2
        exit 1
        ;;
esac

case "$duration" in
    ''|*[!0-9]*)
        echo "Duration must be a non-negative integer." >&2
        exit 1
        ;;
esac

if [ ! -x "$MONITOR_HELPER" ]; then
    echo "monitor-helper binary not found at: $MONITOR_HELPER" >&2
    echo "Run 'cargo build' first or set MONITOR_HELPER to the correct path." >&2
    exit 1
fi

current_output=$($MONITOR_HELPER get --display "$display" input)
current_code=$(printf '%s\n' "$current_output" | awk -F= '/^current=/{print $2}')

case "$current_code" in
    ''|*[!0-9]*)
        echo "Failed to parse the current input value." >&2
        printf '%s\n' "$current_output" >&2
        exit 1
        ;;
esac

restored=0

restore_input() {
    if [ "$restored" -eq 1 ]; then
        return
    fi

    restored=1
    echo "Restoring input to VCP value $current_code" >&2
    if ! $MONITOR_HELPER set --display "$display" input "$current_code" >/dev/null; then
        echo "Failed to restore the original input. Try running: $MONITOR_HELPER set --display $display input $current_code" >&2
        exit 1
    fi
}

trap 'restore_input' EXIT HUP INT TERM

echo "Current input VCP value: $current_code"
echo "Switching display $display to $target_label (VCP value $target_code) for ${duration}s"
$MONITOR_HELPER set --display "$display" input "$target_code" >/dev/null
sleep "$duration"
restore_input
trap - EXIT HUP INT TERM
echo "Input restored."