#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ros_workspace="$repo_root/robot-demo/ros2"
publisher_log="$(mktemp)"
publisher_pid=""

cleanup() {
    if [[ -n "$publisher_pid" ]]; then
        kill "$publisher_pid" 2>/dev/null || true
        wait "$publisher_pid" 2>/dev/null || true
    fi
    rm -f "$publisher_log"
}
trap cleanup EXIT

if ! command -v pixi >/dev/null 2>&1; then
    echo "Pixi is required: https://pixi.prefix.dev/latest/installation/" >&2
    exit 1
fi

cd "$ros_workspace"
export ROS_DOMAIN_ID="${ROS_DOMAIN_ID:-94}"

printf 'Resolving RoboStack Jazzy environment...\n'
pixi install

printf 'Verifying ROS 2 Python imports...\n'
pixi run python -c 'import rclpy; from std_msgs.msg import String; print("ROS 2 Python imports OK")'

printf 'Starting ROS 2 heartbeat publisher (domain %s)...\n' "$ROS_DOMAIN_ID"
pixi run publisher >"$publisher_log" 2>&1 &
publisher_pid="$!"
sleep 2

if ! kill -0 "$publisher_pid" 2>/dev/null; then
    echo "ROS 2 publisher exited before health probe" >&2
    cat "$publisher_log" >&2
    exit 1
fi

if ! timeout 20s pixi run health; then
    echo "ROS 2 health probe failed; publisher output follows:" >&2
    cat "$publisher_log" >&2
    exit 1
fi

printf 'PASS: ROS 2 publisher and subscriber exchanged a healthy heartbeat.\n'