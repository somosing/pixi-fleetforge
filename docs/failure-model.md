# Failure model and verification

| Failure | Detection | Current response | Verification |
| --- | --- | --- | --- |
| Binary differs from manifest digest | SHA-256 mismatch | Reject before activation | `corrupted_artifact_cannot_replace_healthy_release` |
| Candidate health returns nonzero | Health process exit status | Restore old active version | `failed_health_check_restores_previous_release` |
| Agent restarts during staging | Persisted pending version | Keep old active version | `startup_preserves_active_release_if_interrupted_during_staging` and `startup_cleans_orphaned_staging_artifacts` |
| Agent restarts during activation | Persisted pending version | Restore previous active version | `startup_recovers_interrupted_activation` |
| Agent restarts after successful deployment | Persisted state | Preserve active version | `healthy_release_becomes_active_and_survives_restart` |
| Release ID attempts directory traversal | Allowlist of characters | Reject request | `unsafe_release_identifier_is_rejected` |
| Network connection interrupted | HTTP client error | Caller reports failure; no retry yet | Manual exercise required |
| Pixi archive cannot be unpacked | `pixi-unpack` nonzero/missing interpreter | Reject before activation | Manual exercise required |

## Recovery limits

A health check returning successfully does not imply an application is supervised or continues running afterward. The prototype persists a desired active release identifier and can run the candidate's health executable; it does not switch a live ROS 2 graph. For production deployment, make release validation application-specific, supervise processes, limit executable permissions, and define a hardware-safe rollback policy.

## Future acceptance tests

- Five agents and bounded parallel rollouts with canary policy.
- Offline robots retaining their release while connected peers exchange an artifact by digest.
- Recovery after agent restart during unpack, activation and post-update process startup.
- Operator-authenticated change requests and signed artifact verification.
- ROS 2 node health, diagnostic status and motion-safety preconditions.
- Peer-to-peer discovery and content-addressed caching with integrity checks.
