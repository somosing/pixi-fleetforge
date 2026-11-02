# FleetForge validation matrix

This record separates confirmed local prototype tests from work that remains experimental.

## Tested on Ubuntu (2 November 2026)

| Feature | Verification | Limits |
| --- | --- | --- |
| Rust workspace | 10 integration tests passed; Clippy and rustfmt passed | No physical hardware |
| Multi-agent reconciliation | Three local agents deployed v1; failing v2 on robot-3 rolled back | Local-only controller |
| Artifact distribution | Robot-2 retrieved a cached artifact from robot-1 after hub removal | HTTP fallback, not libp2p transfer |
| Pixi archive | 22-package environment (44.93 MiB) unpacked and activated; Python 3.13.18 started | Interpreter check is not application health |
| libp2p | Two peer processes exchanged signed Gossipsub messages both ways | Standalone demo with ephemeral identities |
| Dashboard | SvelteKit checks/build and live robot observation passed | Browser against a local hub |
| CI | Four pull-request checks passed for v0.2 | Repository workflow, not full end-to-end deployment |

Source baseline: [v0.2.0](https://github.com/somosing/pixi-fleetforge/tree/v0.2.0).

## Not yet verified

- ROS 2 Jazzy pub/sub within the RoboStack Pixi environment: run `bash scripts/verify-ros2-heartbeat.sh`.
- Packaging and supervising an entire ROS 2 application as a FleetForge deployment.
- Integrated libp2p artifact transport and authorized multi-machine membership.
- Physical robots, operational safety, secure remote access or production deployment.

## Deployment design lessons

A lockfile ensures reproducible dependency resolution but does not automatically include robot source code in a deployment artifact. Archive integrity and application readiness are separate checks. SHA-256 protects artifact bytes but not publisher authorization. Retaining a working release when a controller is unavailable is safer than making a robot's operation contingent on continuous connectivity.

For Pixi/RoboStack work, the [pixi-build-ros backend](https://pixi.prefix.dev/latest/build/backends/pixi-build-ros/) provides preview automatic dependency mapping; the lack of direct `rosdep` support in a conventional Pixi environment is a distinct limitation.
