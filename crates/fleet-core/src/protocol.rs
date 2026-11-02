use crate::{ArtifactKind, DeploymentRequest, RobotState};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReleaseTarget {
    pub version: String,
    pub sha256: String,
    pub artifact_kind: ArtifactKind,
    #[serde(default)]
    pub health_command: Option<Vec<String>>,
    #[serde(default)]
    pub health_timeout_ms: Option<u64>,
    #[serde(default)]
    pub run_command: Option<Vec<String>>,
}

impl ReleaseTarget {
    pub fn local_request(&self, cached_artifact: PathBuf) -> DeploymentRequest {
        DeploymentRequest {
            version: self.version.clone(),
            artifact_path: cached_artifact,
            sha256: self.sha256.clone(),
            artifact_kind: self.artifact_kind,
            health_command: self.health_command.clone(),
            health_timeout_ms: self.health_timeout_ms,
            run_command: self.run_command.clone(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RolloutRequest {
    pub robot_ids: Vec<String>,
    pub release: ReleaseTarget,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RobotObservation {
    pub state: RobotState,
    pub last_seen_ms: u64,
    pub endpoint: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AgentHeartbeat {
    pub state: RobotState,
    pub endpoint: Option<String>,
}
