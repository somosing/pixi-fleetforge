# libp2p gossip experiment (optional)

Standalone Rust binary, deliberately separate from the core workspace until its dependencies and protocol are verified on Linux.

From this directory, run `cargo run -- 4601` in one terminal. In another run `cargo run -- 4602 /ip4/127.0.0.1/tcp/4601`. Type a JSON object such as `{"robot_id":"robot-1","active_version":"1.0.0"}` into one terminal to transmit it to the other.

This proves an **experimental signed pub/sub transport**, not trusted fleet membership or authorization. The keys are generated afresh at every start. Do not treat gossip announcements as deployment authorization. Production deployments must use the hub's authorized desired-state channel, not unsolicited peer messages.

Possible future integration: authenticate permitted peer identities, subscribe agents to observations and use peer availability announcements to update candidate artifact sources. The current local agent's artifact cache uses manually configured HTTP peers, not this gossip process.
