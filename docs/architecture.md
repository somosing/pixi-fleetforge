# Architecture

## Scope

FleetForge v0.1 is a **local Linux proof of concept**. Three independently running robot agents expose loopback HTTP endpoints. A Rust controller submits release manifests and gathers state concurrently.

```text
                     fleet-controller
                  /deploy       /state
                      HTTP (loopback)
                   /      |      \
            fleet-agent fleet-agent fleet-agent
              robot-1     robot-2     robot-3
                 |           |           |
              state.json   state.json   state.json
              releases/    releases/    releases/
```

`fleet-core` owns the deployment state machine and SHA-256 verification. The agent handles HTTP requests and executes blocking filesystem/health operations on a Tokio blocking worker. The controller uses concurrent asynchronous HTTP requests.

## Deployment sequence

1. Validate the release identifier and SHA-256 format.
2. Persist `pending_version` with the `staging` phase.
3. Copy the artifact into `releases/.VERSION.staging/`.
4. Hash the staged artifact; reject on mismatch.
5. For `pixi_archive`, run `pixi-unpack` and require `env/bin/python` to exist.
6. Rename the staged directory to `releases/VERSION/`.
7. Persist the new `active_version` as `activating`, retaining the previous version.
8. Run `robot-app --health-check` for an executable, or `env/bin/python --version` for a Pixi archive.
9. Persist `healthy` on success, or restore the previous active version and persist `rolled_back` on failure.

The `state.json.tmp` -> `state.json` rename makes a single robot's version pointer switch atomic on Linux filesystems supporting atomic rename within one directory. The file and containing directory are synchronized. This is **not** a multi-node transaction or an atomic replacement of a running ROS 2 process.

## Crash recovery

On startup, a persisted `pending_version` signals interrupted work. During `staging`, the prior active version remains intact. During `activating`, the agent restores `previous_version`. The new agent persists the recovered state before accepting requests.

Interrupted staging cleans the hidden staging directory and any uncommitted release directory during recovery. A previously completed (including failed health-check) release directory is immutable and must use a new version identifier if rebuilt.

## Pixi relation

The root `pixi.toml` provides a reproducible Rust toolchain specification and development commands. The robot demo has its own Pixi environment manifest. `pixi-pack` archives a concrete Python environment; FleetForge verifies its SHA-256 and calls `pixi-unpack` on the robot agent before checking the unpacked interpreter. See `scripts/pack-robot-demo.sh`.

The current Pixi health check verifies the interpreter starts, **not** a ROS 2 node or whole-robot operational readiness. ROS 2 diagnostics are planned.

## Boundaries

The controller is the sole caller in the local demonstration but is not an authenticated authority. There is no persistent queue, artifact cache, content-addressed peer store, gossip protocol, HTTP authentication, signature verification, concurrency limit, deployment timeout, watchdog, frontend or production-grade process supervision. Network faults are observed as request failures, not healed through peer networking.

Do not expose these agent endpoints outside the local host. The manifest refers to files on the **agent's** filesystem and the agent executes staged binaries after checking only a caller-supplied hash. A hash alone is not authenticity. Production use requires publisher signatures and explicit release authorization.
