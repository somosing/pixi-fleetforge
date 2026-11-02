# FleetForge

**Failure-aware robot software deployment — Rust + Pixi, local prototype v0.1.0**

FleetForge explores a small but real piece of robot fleet management: distributing a release to independent robot agents while protecting the last working version when packaging or health validation fails.

This is a learning and engineering prototype, **not** a deployed production fleet manager. Distributed peer discovery, gossip, ROS 2 supervision and the web UI are not implemented in v0.1.

## Implemented

- Rust `fleet-core` deployment state machine with persisted state and restart recovery.
- SHA-256 validation **before** activation; version identifiers are validated against path traversal.
- Staged release directories and atomic replacement of the local `state.json` file on Linux.
- Health-check rejection and restoration of the previous active release identifier.
- Pixi archive deployment via `pixi-pack` and `pixi-unpack` with a basic unpacked-Python health check.
- HTTP robot agent with `GET /state`, `GET /healthz`, `POST /deploy` (local loopback only).
- Concurrent controller commands for multiple robot agent endpoints.
- Rust tests, a three-agent demonstration, GitHub Actions and a failure model.

## Run with Pixi (Ubuntu/Linux x86-64)

Install [Pixi](https://pixi.prefix.dev/latest/installation/) first.

```bash
pixi install
pixi run check
pixi run test
pixi run demo
```

The environment specification is provided as `pixi.toml`. The initial prototype cannot ship a verified `pixi.lock` until dependency resolution is run on an internet-connected development machine. Generate and commit the lockfile before describing the build as fully locked.

Alternatively, with Rust and Cargo already installed:

```bash
cargo test --workspace
bash scripts/demo.sh
```

The demonstration starts **three actual agent processes** on ports 4301–4303. It deploys a healthy shell executable to all three, attempts an unhealthy release on robot 3, and shows the resulting rollback. These robots are independent local processes, not simulated physical robot hardware.

## Pack and deploy a Pixi environment

Install the Pixi packaging tools:

```bash
pixi global install pixi-pack pixi-unpack
bash scripts/pack-robot-demo.sh
```

This builds `environment.tar` and a local manifest at `robot-demo/pixi-release.json`. The archive is ignored by Git. Launch an agent in another terminal:

```bash
pixi run cargo run -p fleet-agent -- robot-pixi ./state/robot-pixi 127.0.0.1:4401
```

Then deploy:

```bash
pixi run cargo run -p fleet-controller -- deploy robot-demo/pixi-release.json http://127.0.0.1:4401
```

The agent needs `pixi-unpack` on `PATH`. The archive is verified before unpacking. For the Pixi mode, the current health contract only tests `env/bin/python --version`.

## Manual API example

The controller accepts local URLs, for example:

```bash
cargo run -p fleet-controller -- status http://127.0.0.1:4301 http://127.0.0.1:4302
cargo run -p fleet-controller -- deploy release.json http://127.0.0.1:4301
```

An executable manifest is a JSON object with `version`, absolute `artifact_path`, its `sha256` and `artifact_kind: "executable"`. The referenced file must exist on the robot agent's machine and respond successfully to `--health-check`.

## Design notes

- [Architecture and deployment state machine](docs/architecture.md)
- [Failure model, test coverage and gaps](docs/failure-model.md)
- [Pixi reproducibility and improvement experiments](docs/pixi-experiments.md)

## Roadmap

1. **v0.1** — local artifact integrity, rollback, restart recovery and Pixi archive support.
2. **v0.2** — configurable agent health contracts, timeouts, safe process supervision, a real artifact registry and authenticated requests.
3. **v0.3** — bounded rollouts, libp2p peer discovery, digest-addressed artifact cache, offline reconciliation.
4. **v0.4** — RoboStack ROS 2 diagnostic node and SvelteKit operator dashboard.

## Security and limitations

This code is intentionally restricted to loopback connections. An agent currently accepts a manifest containing a local artifact path and executes it. SHA-256 verifies bytes against a **caller-supplied** hash, not the publisher's identity. Do not expose the API to a network or use it on physical robots. There is no authentication, signature verification, deployment timeout, running-process switch, or operational safety policy.

## License

MIT — see [LICENSE](LICENSE).
