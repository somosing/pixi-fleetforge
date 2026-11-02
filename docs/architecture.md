# FleetForge architecture and data flow

## Components

```text
SvelteKit operator dashboard (127.0.0.1:5173)
    |  Bearer token, browser fetch
    v
fleet-hub HTTP API (127.0.0.1:4500)
    |-- desired state / robot observations (persistent JSON)
    |-- SHA-256 addressed artifact registry
    |              ^
    v              | optional peer cache fallback
fleet-agent -------+------> fleet-agent
    |                           |
    | deployment engine         | deployment engine
    | release staging           | release staging
    | health gate               | health gate
    | local supervisor          | local supervisor
    v                           v
last healthy release           last healthy release
```

All HTTP services bind to loopback. Hub-mode requests use `FLEETFORGE_TOKEN`. This is **not secure multi-host deployment**. The standalone libp2p gossip experiment demonstrates signed pub/sub messages, but is not an authority for deployments and is not connected to production agents.

## Desired-state reconciliation

1. The operator publishes an artifact to the hub by SHA-256. The hub re-hashes uploaded bytes.
2. A rollout request records each robot's desired version and artifact digest in the hub's persistent registry.
3. Agents send heartbeats with their current phase, version, and endpoint every 4 seconds while the hub is reachable.
4. Each agent polls its desired state. If already healthy or previously attempted for that immutable version, it does not retry.
5. The agent checks its local cache, configured HTTP peers, then the hub for matching bytes. Every candidate is verified against the digest.
6. The deployment engine stages the release and validates its health command within the requested timeout.
7. On success, the active version becomes healthy and the optional runtime command is supervised. On health failure, the previous active version is selected.
8. If the hub becomes unavailable, agents retain the last active release. Heartbeat and desired-state fetching resume on reconnect.

This is polling-based reconciliation, **not** a CRDT, consensus algorithm, or multi-node transaction. The controller remains the sole desired-state authority.

## Local deployment phases

- `staging`: save pending version; copy and validate artifact; unpack Pixi environments.
- `activating`: persist candidate pointer and run its release health contract.
- `healthy`: complete health validation.
- `rejected`: reject incompatible, missing, or digest-mismatched artifact without changing the working release.
- `rolled_back`: failed health validation or supervised application exit; previous version selected.
- `recovered`: interrupted deployment reconciled when the agent restarts.

State is synchronized to disk through `state.json.tmp` → `state.json` rename and directory sync. This is a local metadata transition, **not** atomic process replacement. A process supervisor runs a configured release command after activation, and selects the previous active version if that command exits. There is no supervisor process that survives agent termination.

## Content-addressed artifacts

The hub stores artifact bytes by SHA-256. Agents keep verified cached copies at `state/<robot>/cache/<digest>`. An agent can serve its cache to a configured peer with the same bearer token; this demonstrates peer-assisted distribution without relying on libp2p. The `experiments/fleet-gossip/` tool is an independent research experiment for future discovery/state announcements.

## Security boundary

The prototype has one shared bearer token, loopback-only listeners, no TLS, no operator RBAC, no signed publisher manifests, no rate limits or quotas, no network-wide identity/authorization, and no sandbox for executing downloaded programs. Digest integrity is **not** proof of publisher authenticity. Do not expose its HTTP endpoints to public networks, containers with untrusted tenants, or physical robots.
