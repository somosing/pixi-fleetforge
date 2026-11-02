# FleetForge — Robot Software Fleet Deployment

**Rust + Tokio + Pixi + SHA-256 artifact registry + polling reconciliation + peer-assisted distribution + SvelteKit**

FleetForge is a **local Linux engineering prototype** built to explore reproducible software deployments across multiple robot agents, with failure-aware release activation and recovery. It is not a production robotics system and has not been tested on physical hardware.

## Verified prototype status (2 November 2026)

- **[v0.2.0](https://github.com/somosing/pixi-fleetforge/tree/v0.2.0):** 10 Rust tests passed; formatting, Clippy, fleet demonstrations, and SvelteKit checks/build all passed GitHub Actions CI.
- **Fleet control:** a three-agent local rollout succeeded; a deliberately unhealthy v2 release on robot 3 rolled back to v1. Hash-verified peer-assisted HTTP artifact retrieval was exercised successfully.
- **Pixi artifact:** a 44.93 MiB lightweight Python environment was packed, SHA-256 checked, unpacked, and health-checked on Ubuntu (Python 3.13.18). This does **not** demonstrate a full robot application deployment.
- **Networking and UI:** two separate libp2p Gossipsub peers exchanged messages bidirectionally; the SvelteKit dashboard displayed live state from a local hub.
- **ROS 2 Jazzy:** a separate Pixi-managed RoboStack publisher and subscriber successfully exchanged a `/fleetforge/health` heartbeat on Ubuntu using `bash scripts/verify-ros2-heartbeat.sh`. Its dependency lockfile is committed. This is **not yet a FleetForge-managed ROS deployment**.

See [validation results](docs/validation-results.md) for the scope and limits of each test.

## Architecture

```text
SvelteKit dashboard --(HTTP + token)--> fleet-hub (desired state, artifact registry)
                                         |     ^          ^
                         agent polling   |     |          | uploads
                                         v     |          |
                                       robot agents   fleet-controller CLI
                                         |    |
                                  verified Pixi artifacts
                                  health-check gate, rollback
                                  optional runtime supervisor
```

The hub is authoritative for desired versions; the agents are responsible for local safe operation. A disconnected agent retains the last deployed software and resumes polling when the hub returns.

## Requirements

- Ubuntu Linux x86-64 with Pixi, a C/C++ build toolchain and internet for first dependency installation.
- `python3`, `curl` for example scripts.
- Node.js 22+ for the SvelteKit dashboard.
- `pixi-pack`/`pixi-unpack` for environment packaging (the lightweight robot environment was already tested on Ubuntu).

## Build and verify

```bash
pixi install
pixi run cargo fmt --all
pixi run fmt
pixi run clippy
pixi run test
pixi run demo
pixi run demo-full
pixi run demo-peer
```

The Rust and Pixi lockfiles are committed. GitHub Actions validates formatting, Clippy, Rust tests, fleet demos, and the dashboard build. The separate ROS 2 smoke test is run manually on Ubuntu; it is not part of the default CI.

## Local fleet control demo

`pixi run demo-full` starts one hub and three independent agent processes, uploads two hash-checked release artifacts, assigns version `1.0.0` to all three robots, then deploys deliberately unhealthy `2.0.0` to robot 3. It expects robot 3 to return to `1.0.0` and prints the hub's fleet observations. Temporary state and processes are cleaned on exit.

`pixi run demo-peer` starts two agents. Robot 1 downloads a release from the hub and caches it. The script removes that artifact from the hub, then asks robot 2 to deploy the same digest through its configured peer `robot-1`. Success demonstrates HTTP peer-assisted, hash-verified artifact delivery, **not** libp2p-integrated delivery.

## Manual hub, agent and controller

Set a non-production local token in each shell:

```bash
export FLEETFORGE_TOKEN='replace-with-at-least-16-characters'
```

In separate terminals from the repository root:

```bash
pixi run cargo run -p fleet-hub -- ./state/hub 127.0.0.1:4500
pixi run cargo run -p fleet-agent -- robot-1 ./state/robot-1 127.0.0.1:4511 http://127.0.0.1:4500
pixi run cargo run -p fleet-controller -- fleet http://127.0.0.1:4500
```

To publish an executable or Pixi archive:

```bash
pixi run cargo run -p fleet-controller -- publish /absolute/path/to/artifact http://127.0.0.1:4500
```

Create a local JSON rollout manifest, replacing the digest with the 64-digit SHA-256 printed by `publish`:

```json
{
  "robot_ids": ["robot-1"],
  "release": {
    "version": "1.0.0",
    "sha256": "REPLACE_WITH_64_HEX_DIGITS",
    "artifact_kind": "executable",
    "health_command": ["robot-app", "--health-check"],
    "health_timeout_ms": 10000,
    "run_command": ["robot-app", "--serve"]
  }
}
```

Save the JSON to `rollout.json` and run:

```bash
pixi run cargo run -p fleet-controller -- rollout rollout.json http://127.0.0.1:4500
pixi run cargo run -p fleet-controller -- fleet http://127.0.0.1:4500
```

`run_command` is optional and is appropriate only for long-running robot applications. Without it, the release is validated but no application process is supervised.

## Pixi environment deployment

```bash
pixi global install pixi-pack pixi-unpack
pixi install --manifest-path robot-demo/pixi.toml
bash scripts/pack-robot-demo.sh
```

The pack script creates a 64-digit digest and `robot-demo/pixi-release.json`. The local agent must have `pixi-unpack` on `PATH`. The default Pixi archive health contract is `env/bin/python --version`, which verifies a packaged interpreter, **not** full ROS operation. For manually specified contracts, binaries must be relative to the release root (e.g. `env/bin/python`) and the command must exit successfully within the timeout.

## Dashboard

```bash
cd dashboard
npm install
npm run check
npm run dev
```

Open `http://127.0.0.1:5173`, connect to your locally running hub, and enter the same `FLEETFORGE_TOKEN`. It displays observed robot states, desired releases and freshness, and can assign desired versions for artifacts already uploaded through the CLI. The token is held in page memory only.

## Optional research components

- `robot-demo/ros2`: independent RoboStack Jazzy Pixi environment. Its heartbeat publisher/subscriber test passed on Ubuntu and its lockfile is committed. **Not yet integrated into a packaged FleetForge release lifecycle.**
- `experiments/fleet-gossip`: standalone Rust libp2p signed-gossipsub peer lab. **Not integrated into FleetForge's authority or artifact path.** Its peer identities are ephemeral and do not establish fleet authorization.

## Documentation

- [Architecture, control and deployment flow](docs/architecture.md)
- [Failure cases and verification matrix](docs/failure-model.md)
- [Verified prototype results and limitations](docs/validation-results.md)
- [Pixi experiments and improvement worksheet](docs/pixi-experiments.md)
- [ROS 2 optional experiment](robot-demo/ros2/README.md)
- [libp2p gossip experiment](experiments/fleet-gossip/README.md)

## Security and caveats

**Loopback-only. Never deploy to physical robots or expose ports to untrusted networks.** The shared bearer token is a local demonstration control, not identity/authorization suitable for production. Artifacts are hash-verified, not publisher-signed. HTTP is unencrypted. There is no sandbox, hardened secrets storage, fleet-wide rollback policy, remote secure transport or robot operational safety controller. Process health checks and supervision are deliberately basic. Offline operation means preserving previously running software; it does not guarantee that a disconnected robot can obtain a desired release without a peer or cached artifact.

## License

MIT, see [LICENSE](LICENSE).
