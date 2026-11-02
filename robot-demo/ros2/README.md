# RoboStack ROS 2 Jazzy heartbeat experiment

This is a **standalone verification of a Pixi-managed ROS 2 runtime**. It is not yet a FleetForge-controlled process and is not packaged into a FleetForge release. The already-tested `robot-demo/pixi.toml` packages a lightweight Python environment only.

## Reproducible test on Ubuntu x86-64

From the repository root:

```bash
bash scripts/verify-ros2-heartbeat.sh
```

The script resolves the ROS 2 Jazzy environment through Pixi, imports `rclpy` and `std_msgs`, runs `heartbeat_node.py`, and waits for `ros_health_probe.py` to observe a `healthy` message on `/fleetforge/health` (default `ROS_DOMAIN_ID=94`). It fails if dependencies cannot resolve, the publisher exits prematurely, or no matching message is received. It prints the publisher log on failure.

You can also run the publisher and health probe manually in separate terminals:

```bash
cd robot-demo/ros2
pixi install
pixi run publisher
```

```bash
cd robot-demo/ros2
pixi run health
```

The test covers **ROS 2 pub/sub connectivity inside a Pixi environment**. It does not validate a packaged application, FleetForge's process supervisor, authenticated fleet membership, ROS network partitions, or physical robots. The base environment may be large; check the size before attempting hub uploads (current demo artifact API has a 100 MiB limit).

## Engineering questions

- Does the locked environment resolve in a clean clone?
- Are `rclpy` and `std_msgs` usable through `pixi run` without sourcing a system-wide ROS installation?
- Can the publisher and subscriber communicate under an isolated `ROS_DOMAIN_ID`?
- What does a missing middleware dependency look like in diagnostic logs?
- How much disk space would a packed ROS 2 runtime require, and does that exceed the current artifact API limit?

The [`pixi-build-ros` backend](https://pixi.prefix.dev/latest/build/backends/pixi-build-ros/) offers automatic ROS dependency mapping but is still in preview. Do not claim Pixi lacks ROS dependency mapping. The older ROS 2 tutorial's statement that `rosdep` itself is unsupported is a separate point.
