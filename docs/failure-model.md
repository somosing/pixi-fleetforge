# Failure-model and acceptance tests

| Failure | Implemented response | Evidence path |
|---|---|---|
| Incorrect SHA-256 | Reject staged artifact without replacing active release | `corrupted_artifact_cannot_replace_healthy_release` |
| Unsafe release identifier | Reject version with traversal syntax | `unsafe_release_identifier_is_rejected` |
| Health-check exits nonzero | Select prior working release | `failed_health_check_restores_previous_release` |
| Health-check hangs | Kill direct health-check child after configured timeout and roll back | `health_check_timeout_preserves_last_healthy_release` |
| Agent restarts in staging | Clean incomplete staging; preserve active release | `startup_preserves_active_release_if_interrupted_during_staging` |
| Agent restarts in activation | Restore previous version pointer | `startup_recovers_interrupted_activation` |
| Agent restarts after success | Read the previously persisted active version | `healthy_release_becomes_active_and_survives_restart` |
| Runtime command exits | Supervisor selects previous release pointer | `runtime_failure_reverts_active_version` (core-level); runtime demo needs review |
| Hub unreachable | Keep existing active process, retry heartbeat/reconciliation | Agent code; end-to-end partition demo not automated |
| Hub artifact disappears | Attempt SHA-verified download from configured HTTP peers | `scripts/demo-peer-cache.sh` (requires running on Ubuntu) |
| Corrupt peer bytes | Reject local staged bytes and try next source | Agent hash validation; standalone negative peer test not automated |
| Operator requests faulty rollout | Targeted robot returns to previous version | `scripts/demo-v02.sh` (requires running on Ubuntu) |

## Limitations

A successfully executed `--health-check` is a momentary readiness signal, not proof of long-running health. The optional application supervisor notices process exit, not application deadlock or unsafe motion. Health timeouts kill the directly spawned process, not necessarily its entire descendant process group. This is a prototype for local Linux processes, not a SIL-rated robot safety controller.

Robots in demos are independent processes and test executables, not physical robots. The RoboStack ROS 2 health example is separately packaged and requires validation on an installed ROS 2 environment. The libp2p gossip component is an independent experiment, not integrated with artifact control.
