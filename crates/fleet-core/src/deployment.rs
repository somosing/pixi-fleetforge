use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};
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
    #[error("invalid release health command: {0}")]
    InvalidHealthCommand(String),
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
                | Self::InvalidHealthCommand(_)
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
    #[serde(default)]
    pub health_command: Option<Vec<String>>,
    #[serde(default)]
    pub health_timeout_ms: Option<u64>,
    #[serde(default)]
    pub run_command: Option<Vec<String>>,
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
    #[serde(default)]
    pub last_attempted_version: Option<String>,
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
                last_attempted_version: None,
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
                let orphaned_release = engine.state_root.join("releases").join(&interrupted_version);
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

    pub fn deploy(
        &mut self,
        deployment: DeploymentRequest,
    ) -> Result<RobotState, DeploymentError> {
        validate_identifier(&deployment.version)?;
        validate_digest(&deployment.sha256)?;
        let health_spec = build_health_spec(&deployment)?;
        if let Some(run_command) = &deployment.run_command {
            validate_release_command(run_command)?;
        }

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
        self.robot_state.last_attempted_version = Some(deployment.version.clone());
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

            if let Some(run_command) = &deployment.run_command {
                fs::write(staging_dir.join("run-command.json"), serde_json::to_vec(run_command)?)?;
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

        // The candidate is only committed after its release-specific health contract succeeds.
        let health_outcome = run_health_check(&release_dir, &health_spec);

        if health_outcome.is_ok() {
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

    pub fn active_run_command(&self) -> Result<Option<(String, PathBuf, Vec<String>)>, DeploymentError> {
        let Some(active_version) = &self.robot_state.active_version else { return Ok(None); };
        let release_root = self.state_root.join("releases").join(active_version);
        let run_manifest = release_root.join("run-command.json");
        if !run_manifest.exists() { return Ok(None); }
        let run_command: Vec<String> = serde_json::from_slice(&fs::read(run_manifest)?)?;
        validate_release_command(&run_command)?;
        Ok(Some((active_version.clone(), release_root, run_command)))
    }

    pub fn mark_runtime_failure(&mut self, failed_version: &str) -> Result<RobotState, DeploymentError> {
        if self.robot_state.active_version.as_deref() == Some(failed_version) {
            self.robot_state.active_version = self.robot_state.previous_version.clone();
            self.robot_state.phase = DeploymentPhase::RolledBack;
            self.robot_state.detail = format!("Release {failed_version} exited unexpectedly; previous version selected");
            self.robot_state.pending_version = None;
            self.persist_state()?;
        }
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
    let valid_start = identifier.chars().next().is_some_and(|letter| letter.is_ascii_alphanumeric());
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

struct HealthSpec {
    executable: PathBuf,
    arguments: Vec<String>,
    timeout: Duration,
}

fn build_health_spec(deployment: &DeploymentRequest) -> Result<HealthSpec, DeploymentError> {
    let default_command = match deployment.artifact_kind {
        ArtifactKind::Executable => vec!["robot-app".to_owned(), "--health-check".to_owned()],
        ArtifactKind::PixiArchive => vec!["env/bin/python".to_owned(), "--version".to_owned()],
    };
    let health_command = deployment.health_command.clone().unwrap_or(default_command);
    validate_release_command(&health_command)?;
    let executable = &health_command[0];
    let relative_path = Path::new(executable);
    let timeout_ms = deployment.health_timeout_ms.unwrap_or(10_000);
    if !(100..=120_000).contains(&timeout_ms) {
        return Err(DeploymentError::InvalidHealthCommand(format!("timeout {timeout_ms} ms outside 100..120000")));
    }
    Ok(HealthSpec {
        executable: relative_path.to_path_buf(),
        arguments: health_command.into_iter().skip(1).collect(),
        timeout: Duration::from_millis(timeout_ms),
    })
}

fn run_health_check(release_dir: &Path, health: &HealthSpec) -> Result<(), DeploymentError> {
    let mut health_process = Command::new(release_dir.join(&health.executable))
        .args(&health.arguments)
        .current_dir(release_dir)
        .spawn()?;
    let started_at = Instant::now();
    loop {
        if let Some(exit_status) = health_process.try_wait()? {
            if exit_status.success() {
                return Ok(());
            }
            return Err(DeploymentError::InvalidHealthCommand(format!("health command exited with {exit_status}")));
        }
        if started_at.elapsed() >= health.timeout {
            health_process.kill()?;
            health_process.wait()?;
            return Err(DeploymentError::InvalidHealthCommand("health command timed out".to_owned()));
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn validate_release_command(command: &[String]) -> Result<(), DeploymentError> {
    let Some(executable) = command.first() else {
        return Err(DeploymentError::InvalidHealthCommand("empty release command".to_owned()));
    };
    let executable_path = Path::new(executable);
    if !executable_path.components().all(|component| matches!(component, std::path::Component::Normal(_))) {
        return Err(DeploymentError::InvalidHealthCommand(executable.to_owned()));
    }
    Ok(())
}
