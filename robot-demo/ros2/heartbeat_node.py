import rclpy
from rclpy.node import Node
from std_msgs.msg import String


class FleetHealthPublisher(Node):
    def __init__(self):
        super().__init__("fleetforge_health_publisher")
        self.heartbeat_publisher = self.create_publisher(String, "/fleetforge/health", 10)
        self.timer = self.create_timer(1.0, self.publish_health)

    def publish_health(self):
        heartbeat = String()
        heartbeat.data = "healthy"
        self.heartbeat_publisher.publish(heartbeat)


def main():
    rclpy.init()
    health_node = FleetHealthPublisher()
    try:
        rclpy.spin(health_node)
    finally:
        health_node.destroy_node()
        rclpy.shutdown()


if __name__ == "__main__":
    main()
