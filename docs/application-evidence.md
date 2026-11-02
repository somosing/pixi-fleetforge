# FleetForge: evidence for the prefix.dev reconsideration letter

Use this as an engineering notebook, **not as a generated motivation letter**. Wolf Vollprecht requested a letter written by the applicant. Record your own opinions, run the remaining tests yourself, and write your own email.

## Verified by the candidate on Ubuntu — 2 November 2026

| Test | Evidence | Scope |
| --- | --- | --- |
| Rust tests | 10/10 passed | Deployment core including timeouts, restart recovery and rollback |
| Rust CI | Both push and pull-request Rust jobs passed | Workspace formatting, Clippy, tests, fleet demos |
| Dashboard CI | Both push and pull-request jobs passed | SvelteKit check and build |
| Pixi deployment | 22 packages packed into a 44.93 MiB archive; unpack and activation returned `healthy` | Lightweight Python runtime; Python 3.13.18, not ROS 2 |
| Fleet rollout | Three independent local agents; unhealthy v2 rejected on robot-3 and v1 retained | Local Linux test, not physical robots |
| Peer artifact retrieval | Robot-2 fetched a hash-verified artifact from robot-1 after removal from hub | HTTP peer fallback; not integrated libp2p artifact transport |
| libp2p | Two local peers exchanged signed Gossipsub observations both ways | Separate experiment; ephemeral identities |
| Dashboard | Live hub connection and robot-1 shown as online | Local browser + loopback services |

Stable code reference: [`v0.2.0`](https://github.com/somosing/pixi-fleetforge/tree/v0.2.0). Repository: https://github.com/somosing/pixi-fleetforge

## Pixi: what was actually learned

1. The root development workspace and the robot runtime are **independent Pixi environments with separate lockfiles**. A pack attempt failed until `robot-demo/pixi.lock` was generated.
2. `pixi-pack` packaged the lightweight environment and `pixi-unpack` reproduced it on the same Linux platform. The archive's SHA-256 was checked before staging.
3. The startup contract `env/bin/python --version` is a **weak health check**; it proves a Python interpreter starts, not that a robot application works.
4. Pixi packages dependencies. Application source and its launch/health contract need an explicit deployment story; packaging a runtime alone does not ship a running ROS 2 node.
5. Pixi's `pixi-build-ros` preview backend provides ROS build integration and automatic dependency mapping. The fact that `rosdep` cannot be used directly in a standard Pixi environment does not mean Pixi has no ROS integration.

## Pixi improvement hypothesis to evaluate (not a confirmed defect)

A deployability preflight command or report for robot environments could flag: missing/outdated lockfile; target platform/architecture; archive size and expected disk usage; whether applications and assets are included beyond the managed dependency environment; and missing runtime activation/health commands. Verify whether existing Pixi/Pixi Pack functionality already addresses each need before proposing this upstream.

Candidate test: package both the lightweight Python and ROS 2 environments, compare packed sizes, unpack in clean directories, and record which commands work without internet or system ROS. State specific missing diagnostics only after reproducing them.

## Fleet architecture: decisions to be able to explain

- **Desired vs actual:** hub records requested release; agents reconcile changes and send observations. Offline agents keep their last known-good release.
- **Rollback:** stage and hash-verify the candidate, run a bounded health check, and revert active release state if validation fails.
- **Integrity vs authority:** SHA-256 verifies downloaded bytes against the requested digest; it does **not** prove who is authorized to publish or request the software.
- **Peer-assisted delivery:** cache by digest; verify again after downloading from another agent. Peers must not override deployment authority.
- **Failure handling:** distinguish a failed candidate, a disappeared controller, a timed-out health check, and a runtime process crash.
- **Not implemented:** production authorization/identity, secure multi-host transport, hardware safety policies, integrated libp2p control plane, full ROS 2 lifecycle deployment.

## Four parts of the handwritten letter

1. **Actual Pixi use:** briefly describe the separate lockfile mistake, 22-package archive, successful unpack and what that revealed. Add one well-supported improvement idea after running the next experiment.
2. **Fleet design:** explain agents, desired-state reconciliation, integrity checking, health checks, staged rollout, rollback, offline behavior and why you chose that order.
3. **Reference:** ask a real supervisor or collaborator for permission before giving their name and contact information. Do not invent anyone or assume they consented.
4. **Berlin:** explicitly say you are ready to relocate and work on-site, if that remains true.

Keep the letter short, specific and unmistakably in your own language. Do not claim code you have not run or downstream integrations that do not exist.

## Outstanding experiment

```bash
bash scripts/verify-ros2-heartbeat.sh
```

Record: Pixi version, lockfile resolution result, ROS distro, stdout from health probe, approximate install time, and failure modes. This experiment is not yet marked completed.
