# Validation provenance

## Evidence from the user's actual Ubuntu run (v0.1)

- 7/7 Rust integration tests passed.
- `cargo fmt --check`, `clippy --workspace --all-targets -- -D warnings` passed.
- Three-agent demo deployed healthy release v1 and rolled back a failing v2 on robot 3.
- Pixi Python environment packed to a 44.93 MiB archive; 22 packages unpacked and packaged Python 3.13.18 launched.
- Source and lockfiles published to GitHub `somosing/pixi-fleetforge` at commit `caefd9c` (robot-demo lock).

## Static checks performed on this v0.2 source package

- Rust Cargo manifests and Pixi manifests parsed as TOML.
- Shell scripts parsed with `bash -n`.
- RoboStack Python scripts passed `python -m py_compile`.
- JSON files parsed by standard JSON tooling.

## Not verified in this environment

- Rust compilation and Clippy for newly added source (Rust compiler and Cargo unavailable here).
- `scripts/demo-v02.sh` and `scripts/demo-peer-cache.sh` actual end-to-end execution.
- SvelteKit dependency installation, type checking and build.
- Independent libp2p experiment compile/runtime.
- RoboStack Jazzy package resolution and ROS 2 heartbeat delivery.
- Remote secure deployment, physical robot operational safety or network-wide fault tolerance.

Do not describe unverified items as passing tests when presenting this project to an employer.
