use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DeploymentError {
    #[error("I/O failure: {0}")]
    Io(#[from] std::io::Error),
    #[error("state encoding failure: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid robot or release identifier: {0}")]
    InvalidIdentifier(String),
    #[error("invalid SHA-256 digest: {0}")]
    InvalidDigest(String),
    #[error("release already exists: {0}")]
    ReleaseExists(String),
    #[error("release is already active: {0}")]
    AlreadyActive(String),
    #[error("state belongs to robot {existing_id}, not {requested_id}")]
    WrongRobot {
        existing_id: String,
        requested_id: String,
    },
    #[error("deployment is already in progress")]
    Busy,
    #[error("Pixi environment unpack failed or did not contain env/bin/python")]
    UnpackFailed,
}

impl DeploymentError {
    pub fn is_invalid_request(&self) -> bool {
        matches!(
            self,
            Self::InvalidIdentifier(_)
                | Self::InvalidDigest(_)
                | Self::ReleaseExists(_)
                | Self::AlreadyActive(_)
                | Self::Busy
        )
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentPhase {
    Healthy,
    Staging,
    Activating,
    RolledBack,
    Rejected,
    Recovered,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DeploymentRequest {
    pub version: String,
    pub artifact_path: PathBuf,
    pub sha256: String,
    #[serde(default)]
    pub artifact_kind: ArtifactKind,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    #[default]
    Executable,
    PixiArchive,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RobotState {
    pub robot_id: String,
    pub active_version: Option<String>,
    pub previous_version: Option<String>,
    pub pending_version: Option<String>,
    pub phase: DeploymentPhase,
    pub detail: String,
}

pub struct DeploymentEngine {
    state_root: PathBuf,
    robot_state: RobotState,
}

impl DeploymentEngine {
    pub fn open(state_root: impl AsRef<Path>, robot_id: &str) -> Result<Self, DeploymentError> {
        validate_identifier(robot_id)?;
        let state_root = state_root.as_ref().to_path_buf();
        fs::create_dir_all(state_root.join("releases"))?;
        let state_path = state_root.join("state.json");

        let robot_state = if state_path.exists() {
            let state_file = File::open(state_path)?;
            serde_json::from_reader(state_file)?
        } else {
            RobotState {
                robot_id: robot_id.to_owned(),
                active_version: None,
                previous_version: None,
                pending_version: None,
                phase: DeploymentPhase::Healthy,
                detail: "No release deployed".to_owned(),
            }
        };

        if robot_state.robot_id != robot_id {
            return Err(DeploymentError::WrongRobot {
                existing_id: robot_state.robot_id,
                requested_id: robot_id.to_owned(),
            });
        }

        let mut engine = Self {
            state_root,
            robot_state,
        };

        if let Some(interrupted_version) = engine.robot_state.pending_version.clone() {
            // Staging has not changed the active release; activation may have.
            if engine.robot_state.phase == DeploymentPhase::Activating {
                engine.robot_state.active_version = engine.robot_state.previous_version.clone();
            } else if engine.robot_state.phase == DeploymentPhase::Staging {
                let orphaned_release = engine
                    .state_root
                    .join("releases")
                    .join(&interrupted_version);
                if orphaned_release.exists() {
                    fs::remove_dir_all(orphaned_release)?;
                }
                let staging_directory = engine
                    .state_root
                    .join("releases")
                    .join(format!(".{interrupted_version}.staging"));
                if staging_directory.exists() {
                    fs::remove_dir_all(staging_directory)?;
                }
            }
            engine.robot_state.pending_version = None;
            engine.robot_state.phase = DeploymentPhase::Recovered;
            engine.robot_state.detail = "Interrupted deployment recovered on startup".to_owned();
        }

        engine.persist_state()?;
        Ok(engine)
    }

    pub fn state(&self) -> RobotState {
        self.robot_state.clone()
    }

    pub fn deploy(&mut self, deployment: DeploymentRequest) -> Result<RobotState, DeploymentError> {
        validate_identifier(&deployment.version)?;
        validate_digest(&deployment.sha256)?;

        if self.robot_state.pending_version.is_some() {
            return Err(DeploymentError::Busy);
        }
        if self.robot_state.active_version.as_deref() == Some(&deployment.version) {
            return Err(DeploymentError::AlreadyActive(deployment.version));
        }

        let releases_root = self.state_root.join("releases");
        let release_dir = releases_root.join(&deployment.version);
        let staging_dir = releases_root.join(format!(".{}.staging", deployment.version));

        if release_dir.exists() {
            return Err(DeploymentError::ReleaseExists(deployment.version));
        }
        if staging_dir.exists() {
            fs::remove_dir_all(&staging_dir)?;
        }

        let original_version = self.robot_state.active_version.clone();
        self.robot_state.pending_version = Some(deployment.version.clone());
        self.robot_state.phase = DeploymentPhase::Staging;
        self.robot_state.detail = format!("Staging {}", deployment.version);
        self.persist_state()?;

        let artifact_filename = match deployment.artifact_kind {
            ArtifactKind::Executable => "robot-app",
            ArtifactKind::PixiArchive => "environment.tar",
        };
        let staged_artifact = staging_dir.join(artifact_filename);
        let stage_outcome = (|| -> Result<(), DeploymentError> {
            fs::create_dir_all(&staging_dir)?;
            fs::copy(&deployment.artifact_path, &staged_artifact)?;

            let artifact_digest = sha256_file(&staged_artifact)?;
            if !artifact_digest.eq_ignore_ascii_case(&deployment.sha256) {
                return Err(DeploymentError::InvalidDigest(format!(
                    "expected {}, received {}",
                    deployment.sha256, artifact_digest
                )));
            }

            if matches!(deployment.artifact_kind, ArtifactKind::PixiArchive) {
                let unpack_status = Command::new("pixi-unpack")
                    .arg("environment.tar")
                    .current_dir(&staging_dir)
                    .status()?;
                if !unpack_status.success() || !staging_dir.join("env/bin/python").exists() {
                    return Err(DeploymentError::UnpackFailed);
                }
            }

            fs::rename(&staging_dir, &release_dir)?;
            Ok(())
        })();

        if let Err(stage_error) = stage_outcome {
            if staging_dir.exists() {
                fs::remove_dir_all(&staging_dir)?;
            }
            self.robot_state.pending_version = None;
            self.robot_state.phase = DeploymentPhase::Rejected;
            self.robot_state.detail = format!("Staging rejected: {stage_error}");
            self.persist_state()?;
            return Ok(self.state());
        }

        self.robot_state.previous_version = original_version.clone();
        self.robot_state.active_version = Some(deployment.version.clone());
        self.robot_state.phase = DeploymentPhase::Activating;
        self.robot_state.detail = format!("Validating {}", deployment.version);
        self.persist_state()?;

        // A candidate is committed only after its executable passes the health contract.
        let health_outcome = match deployment.artifact_kind {
            ArtifactKind::Executable => Command::new(release_dir.join("robot-app"))
                .arg("--health-check")
                .status(),
            ArtifactKind::PixiArchive => Command::new(release_dir.join("env/bin/python"))
                .arg("--version")
                .status(),
        };

        if matches!(health_outcome, Ok(exit_status) if exit_status.success()) {
            self.robot_state.phase = DeploymentPhase::Healthy;
            self.robot_state.detail = format!("Release {} is healthy", deployment.version);
        } else {
            self.robot_state.active_version = original_version;
            self.robot_state.phase = DeploymentPhase::RolledBack;
            self.robot_state.detail = format!(
                "Release {} failed health check; previous version restored",
                deployment.version
            );
        }

        self.robot_state.pending_version = None;
        self.persist_state()?;
        Ok(self.state())
    }

    fn persist_state(&self) -> Result<(), DeploymentError> {
        let state_path = self.state_root.join("state.json");
        let staging_path = self.state_root.join("state.json.tmp");
        let serialized_state = serde_json::to_vec_pretty(&self.robot_state)?;

        {
            let mut state_file = File::create(&staging_path)?;
            state_file.write_all(&serialized_state)?;
            state_file.sync_all()?;
        }

        fs::rename(staging_path, state_path)?;
        File::open(&self.state_root)?.sync_all()?;
        Ok(())
    }
}

pub fn sha256_file(binary_path: &Path) -> Result<String, DeploymentError> {
    let mut binary_file = File::open(binary_path)?;
    let mut digest_engine = Sha256::new();
    let mut read_buffer = [0_u8; 8192];

    loop {
        let read_bytes = binary_file.read(&mut read_buffer)?;
        if read_bytes == 0 {
            break;
        }
        digest_engine.update(&read_buffer[..read_bytes]);
    }

    Ok(format!("{:x}", digest_engine.finalize()))
}

fn validate_identifier(identifier: &str) -> Result<(), DeploymentError> {
    let valid_start = identifier
        .chars()
        .next()
        .is_some_and(|letter| letter.is_ascii_alphanumeric());
    let valid_characters = identifier
        .chars()
        .all(|letter| letter.is_ascii_alphanumeric() || matches!(letter, '-' | '_' | '.'));

    if !valid_start || !valid_characters || identifier.len() > 64 {
        return Err(DeploymentError::InvalidIdentifier(identifier.to_owned()));
    }

    Ok(())
}

fn validate_digest(digest: &str) -> Result<(), DeploymentError> {
    if digest.len() != 64 || !digest.bytes().all(|digit| digit.is_ascii_hexdigit()) {
        return Err(DeploymentError::InvalidDigest(digest.to_owned()));
    }
    Ok(())
}
