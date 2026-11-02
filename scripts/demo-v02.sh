#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
run_root="$(mktemp -d)"
service_pids=()
export FLEETFORGE_TOKEN="${FLEETFORGE_TOKEN:-fleetforge-demo-token-not-for-production}"
hub_url="http://127.0.0.1:4500"

cleanup() {
    for service_pid in "${service_pids[@]}"; do
        pkill -TERM -P "$service_pid" 2>/dev/null || true
        kill "$service_pid" 2>/dev/null || true
    done
    rm -rf "$run_root"
}
trap cleanup EXIT

cd "$repo_root"
cargo build --workspace --quiet

cat > "$run_root/healthy.sh" <<'APP'
#!/bin/sh
case "$1" in
    --health-check) exit 0 ;;
    --serve) while true; do sleep 1; done ;;
    *) exit 2 ;;
esac
APP
cat > "$run_root/unhealthy.sh" <<'APP'
#!/bin/sh
case "$1" in
    --health-check) exit 1 ;;
    *) exit 2 ;;
esac
APP
chmod +x "$run_root/healthy.sh" "$run_root/unhealthy.sh"

healthy_digest="$(sha256sum "$run_root/healthy.sh" | cut -d ' ' -f 1)"
unhealthy_digest="$(sha256sum "$run_root/unhealthy.sh" | cut -d ' ' -f 1)"

"$repo_root/target/debug/fleet-hub" "$run_root/hub" 127.0.0.1:4500 >"$run_root/hub.log" 2>&1 &
service_pids+=("$!")

for robot_number in 1 2 3; do
    robot_port="$((4510 + robot_number))"
    FLEETFORGE_PEERS="http://127.0.0.1:4511,http://127.0.0.1:4512,http://127.0.0.1:4513" \
        "$repo_root/target/debug/fleet-agent" "robot-$robot_number" \
        "$run_root/robot-$robot_number" "127.0.0.1:$robot_port" "$hub_url" \
        >"$run_root/robot-$robot_number.log" 2>&1 &
    service_pids+=("$!")
done

for service_url in "$hub_url/healthz" http://127.0.0.1:4511/healthz http://127.0.0.1:4512/healthz http://127.0.0.1:4513/healthz; do
    ready=false
    for attempt in $(seq 1 60); do
        if curl --silent --fail "$service_url" >/dev/null; then
            ready=true
            break
        fi
        sleep 0.2
    done
    if [ "$ready" != true ]; then
        echo "Service startup failed: $service_url" >&2
        cat "$run_root"/*.log >&2
        exit 1
    fi
done

"$repo_root/target/debug/fleet-controller" publish "$run_root/healthy.sh" "$hub_url"
"$repo_root/target/debug/fleet-controller" publish "$run_root/unhealthy.sh" "$hub_url"

cat > "$run_root/v1-rollout.json" <<JSON
{
  "robot_ids": ["robot-1", "robot-2", "robot-3"],
  "release": {
    "version": "1.0.0",
    "sha256": "$healthy_digest",
    "artifact_kind": "executable",
    "health_command": ["robot-app", "--health-check"],
    "run_command": ["robot-app", "--serve"],
    "health_timeout_ms": 5000
  }
}
JSON
cat > "$run_root/v2-rollout.json" <<JSON
{
  "robot_ids": ["robot-3"],
  "release": {
    "version": "2.0.0",
    "sha256": "$unhealthy_digest",
    "artifact_kind": "executable",
    "health_command": ["robot-app", "--health-check"],
    "health_timeout_ms": 5000
  }
}
JSON

"$repo_root/target/debug/fleet-controller" rollout "$run_root/v1-rollout.json" "$hub_url"

wait_for_state() {
    local robot_id="$1"
    local active_version="$2"
    local phase="$3"
    for attempt in $(seq 1 40); do
        local report
        report="$(curl --silent --header "Authorization: Bearer $FLEETFORGE_TOKEN" "$hub_url/api/fleet")"
        if printf '%s' "$report" | python3 -c '
import json, sys
report = json.load(sys.stdin)
robot_id, version, phase = sys.argv[1:]
state = report.get("observed", {}).get(robot_id, {}).get("state", {})
sys.exit(0 if state.get("active_version") == version and state.get("phase") == phase else 1)
' "$robot_id" "$active_version" "$phase"; then
            return 0
        fi
        sleep 1
    done
    echo "Timed out: $robot_id did not reach $active_version ($phase)" >&2
    cat "$run_root"/*.log >&2
    exit 1
}

printf '\nWaiting for version 1.0.0 on three agents...\n'
for robot_number in 1 2 3; do
    wait_for_state "robot-$robot_number" "1.0.0" healthy
done

"$repo_root/target/debug/fleet-controller" rollout "$run_root/v2-rollout.json" "$hub_url"
wait_for_state robot-3 1.0.0 rolled_back

printf '\n=== Fleet after failed robot-3 rollout ===\n'
"$repo_root/target/debug/fleet-controller" fleet "$hub_url"
printf '\nPASS: healthy rollout, one failed health check and rollback.\n'
