#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
run_root="$(mktemp -d)"
service_pids=()
export FLEETFORGE_TOKEN="${FLEETFORGE_TOKEN:-fleetforge-local-peer-cache-demo}"
hub_url="http://127.0.0.1:4700"

cleanup() {
    for service_pid in "${service_pids[@]}"; do kill "$service_pid" 2>/dev/null || true; done
    rm -rf "$run_root"
}
trap cleanup EXIT

cd "$repo_root"
cargo build --workspace --quiet
cat > "$run_root/robot.sh" <<'APP'
#!/bin/sh
if [ "$1" = "--health-check" ]; then exit 0; fi
exit 2
APP
chmod +x "$run_root/robot.sh"
artifact_digest="$(sha256sum "$run_root/robot.sh" | cut -d ' ' -f 1)"
"$repo_root/target/debug/fleet-hub" "$run_root/hub" 127.0.0.1:4700 >"$run_root/hub.log" 2>&1 &
service_pids+=("$!")
"$repo_root/target/debug/fleet-agent" robot-1 "$run_root/robot-1" 127.0.0.1:4711 "$hub_url" >"$run_root/robot-1.log" 2>&1 &
service_pids+=("$!")
FLEETFORGE_PEERS=http://127.0.0.1:4711 "$repo_root/target/debug/fleet-agent" robot-2 "$run_root/robot-2" 127.0.0.1:4712 "$hub_url" >"$run_root/robot-2.log" 2>&1 &
service_pids+=("$!")

for service_url in "$hub_url/healthz" http://127.0.0.1:4711/healthz http://127.0.0.1:4712/healthz; do
    for attempt in $(seq 1 50); do
        if curl --silent --fail "$service_url" >/dev/null; then break; fi
        sleep 0.2
    done
    curl --silent --fail "$service_url" >/dev/null
 done

"$repo_root/target/debug/fleet-controller" publish "$run_root/robot.sh" "$hub_url"
cat > "$run_root/rollout.json" <<JSON
{
  "robot_ids": ["robot-1"],
  "release": {
    "version": "1.0.0",
    "sha256": "$artifact_digest",
    "artifact_kind": "executable"
  }
}
JSON
"$repo_root/target/debug/fleet-controller" rollout "$run_root/rollout.json" "$hub_url"

wait_for_robot() {
    local robot_id="$1"
    for attempt in $(seq 1 40); do
        local report
        report="$(curl --silent --header "Authorization: Bearer $FLEETFORGE_TOKEN" "$hub_url/api/fleet")"
        if printf '%s' "$report" | python3 -c '
import json, sys
robot_id = sys.argv[1]
fleet = json.load(sys.stdin)
state = fleet.get("observed", {}).get(robot_id, {}).get("state", {})
sys.exit(0 if state.get("active_version") == "1.0.0" and state.get("phase") == "healthy" else 1)
' "$robot_id"; then return 0; fi
        sleep 1
    done
    cat "$run_root"/*.log >&2
    echo "Robot did not reach healthy state: $robot_id" >&2
    exit 1
}

wait_for_robot robot-1
if [ ! -f "$run_root/robot-1/cache/$artifact_digest" ]; then
    echo "Robot-1 does not have the expected artifact cache" >&2
    exit 1
fi
rm "$run_root/hub/artifacts/$artifact_digest"
cat > "$run_root/rollout.json" <<JSON
{
  "robot_ids": ["robot-2"],
  "release": {
    "version": "1.0.0",
    "sha256": "$artifact_digest",
    "artifact_kind": "executable"
  }
}
JSON
"$repo_root/target/debug/fleet-controller" rollout "$run_root/rollout.json" "$hub_url"
wait_for_robot robot-2
printf '\nPASS: robot-2 fetched the authenticated, hash-verified artifact from robot-1 after it was removed from the hub.\n'
