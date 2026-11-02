#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
run_root="$(mktemp -d)"
agent_pids=()

cleanup() {
    for agent_pid in "${agent_pids[@]}"; do
        kill "$agent_pid" 2>/dev/null || true
    done
    rm -rf "$run_root"
}
trap cleanup EXIT

cd "$repo_root"
cargo build --workspace --quiet

cat > "$run_root/healthy.sh" <<'APP'
#!/bin/sh
[ "$1" = "--health-check" ] && exit 0
exit 2
APP
cat > "$run_root/unhealthy.sh" <<'APP'
#!/bin/sh
[ "$1" = "--health-check" ] && exit 1
exit 2
APP
chmod +x "$run_root/healthy.sh" "$run_root/unhealthy.sh"

healthy_digest="$(sha256sum "$run_root/healthy.sh" | cut -d ' ' -f 1)"
unhealthy_digest="$(sha256sum "$run_root/unhealthy.sh" | cut -d ' ' -f 1)"

cat > "$run_root/release-1.json" <<JSON
{"version":"1.0.0","artifact_path":"$run_root/healthy.sh","sha256":"$healthy_digest","artifact_kind":"executable"}
JSON
cat > "$run_root/release-2-failing.json" <<JSON
{"version":"2.0.0","artifact_path":"$run_root/unhealthy.sh","sha256":"$unhealthy_digest","artifact_kind":"executable"}
JSON

agent_urls=()
for robot_number in 1 2 3; do
    robot_port="$((4300 + robot_number))"
    robot_url="http://127.0.0.1:$robot_port"
    "$repo_root/target/debug/fleet-agent" "robot-$robot_number" "$run_root/state-$robot_number" "127.0.0.1:$robot_port" > "$run_root/robot-$robot_number.log" 2>&1 &
    agent_pids+=("$!")
    agent_urls+=("$robot_url")
done

for robot_url in "${agent_urls[@]}"; do
    ready=false
    for attempt in $(seq 1 40); do
        if curl --silent --fail "$robot_url/healthz" >/dev/null; then
            ready=true
            break
        fi
        sleep 0.1
    done
    if [ "$ready" != true ]; then
        echo "Agent unavailable: $robot_url" >&2
        exit 1
    fi
done

printf '\n=== Deploying healthy release 1.0.0 to all agents ===\n'
"$repo_root/target/debug/fleet-controller" deploy "$run_root/release-1.json" "${agent_urls[@]}"

printf '\n=== Attempting unhealthy 2.0.0 only on robot-3 ===\n'
"$repo_root/target/debug/fleet-controller" deploy "$run_root/release-2-failing.json" "${agent_urls[2]}"

printf '\n=== Final fleet state ===\n'
"$repo_root/target/debug/fleet-controller" status "${agent_urls[@]}"
