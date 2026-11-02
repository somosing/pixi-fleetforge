mod deployment;
mod protocol;

pub use deployment::{
    sha256_file, ArtifactKind, DeploymentEngine, DeploymentError, DeploymentPhase,
    DeploymentRequest, RobotState,
};

pub use protocol::{AgentHeartbeat, ReleaseTarget, RobotObservation, RolloutRequest};
