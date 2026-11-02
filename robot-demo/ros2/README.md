# Optional RoboStack ROS 2 Jazzy health demonstration

This environment is separate from the already verified lightweight Python robot demo. It is intentionally not included in the default Rust CI or in Pixi environment archives until Linux resolution and runtime checks are verified.

```bash
cd robot-demo/ros2
pixi install
pixi run publisher
```

From another terminal in the same directory:

```bash
pixi run health
```

The publisher emits `std_msgs/String` messages on `/fleetforge/health`; the probe exits successfully only after receiving `healthy` within five seconds.

Once the environment is packed and unpacked in a release, a suitable explicit release health contract is `env/bin/python ros_health_probe.py` **only if** the publisher is started as part of the application lifecycle and the probe script is packaged into the release. FleetForge's current archive example only packages Python. The ROS 2 packages, health scripts and launch order still need integration verification on your Ubuntu machine.

Reference: https://robostack.github.io/GettingStarted.html
