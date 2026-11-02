import time

import rclpy
from rclpy.node import Node
from std_msgs.msg import String


class FleetHealthProbe(Node):
    def __init__(self):
        super().__init__("fleetforge_health_probe")
        self.heartbeat_received = False
        self.subscription = self.create_subscription(
            String, "/fleetforge/health", self.on_heartbeat, 10
        )

    def on_heartbeat(self, message):
        if message.data == "healthy":
            self.heartbeat_received = True


def main():
    rclpy.init()
    health_probe = FleetHealthProbe()
    started_at = time.monotonic()
    try:
        while time.monotonic() - started_at < 5.0:
            rclpy.spin_once(health_probe, timeout_sec=0.2)
            if health_probe.heartbeat_received:
                print("ROS 2 heartbeat healthy")
                return
        raise SystemExit("ROS 2 heartbeat timed out")
    finally:
        health_probe.destroy_node()
        rclpy.shutdown()


if __name__ == "__main__":
    main()
