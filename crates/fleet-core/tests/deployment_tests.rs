#![cfg(unix)]

use fleet_core::{
    sha256_file, ArtifactKind, DeploymentEngine, DeploymentPhase, DeploymentRequest, RobotState,
};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn write_test_binary(test_root: &Path, version: &str, healthy: bool) -> PathBuf {
    let binary_path = test_root.join(format!("robot-{version}.sh"));
    let exit_code = if healthy { 0 } else { 1 };
    fs::write(
        &binary_path,
        format!("#!/bin/sh\nif [ \"$1\" = \"--health-check\" ]; then exit {exit_code}; fi\n"),
    )
    .unwrap();
    fs::set_permissions(&binary_path, fs::Permissions::from_mode(0o755)).unwrap();
    binary_path
}

fn release_request(binary_path: &Path, version: &str) -> DeploymentRequest {
    DeploymentRequest {
        version: version.to_owned(),
        artifact_path: binary_path.to_owned(),
        sha256: sha256_file(binary_path).unwrap(),
        artifact_kind: ArtifactKind::Executable,
        health_command: None,
        health_timeout_ms: None,
        run_command: None,
    }
}

fn apply_first_release(test_root: &TempDir) -> DeploymentEngine {
    let mut robot_agent = DeploymentEngine::open(test_root.path().join("robot-state"), "robot-1").unwrap();
    let healthy_binary = write_test_binary(test_root.path(), "1.0.0", true);
    let deployed_state = robot_agent.deploy(release_request(&healthy_binary, "1.0.0")).unwrap();
    assert_eq!(deployed_state.phase, DeploymentPhase::Healthy);
    robot_agent
}

#[test]
fn healthy_release_becomes_active_and_survives_restart() {
    let test_root = tempfile::tempdir().unwrap();
    let robot_agent = apply_first_release(&test_root);
    assert_eq!(robot_agent.state().active_version.as_deref(), Some("1.0.0"));

    let restarted_agent = DeploymentEngine::open(test_root.path().join("robot-state"), "robot-1").unwrap();
    assert_eq!(restarted_agent.state().active_version.as_deref(), Some("1.0.0"));
}

#[test]
fn failed_health_check_restores_previous_release() {
    let test_root = tempfile::tempdir().unwrap();
    let mut robot_agent = apply_first_release(&test_root);
    let unhealthy_binary = write_test_binary(test_root.path(), "2.0.0", false);

    let deployed_state = robot_agent.deploy(release_request(&unhealthy_binary, "2.0.0")).unwrap();
    assert_eq!(deployed_state.phase, DeploymentPhase::RolledBack);
    assert_eq!(deployed_state.active_version.as_deref(), Some("1.0.0"));
    assert_eq!(deployed_state.pending_version, None);
}

#[test]
fn corrupted_artifact_cannot_replace_healthy_release() {
    let test_root = tempfile::tempdir().unwrap();
    let mut robot_agent = apply_first_release(&test_root);
    let second_binary = write_test_binary(test_root.path(), "2.0.0", true);
    let mut deployment = release_request(&second_binary, "2.0.0");
    deployment.sha256 = "0".repeat(64);

    let deployed_state = robot_agent.deploy(deployment).unwrap();
    assert_eq!(deployed_state.phase, DeploymentPhase::Rejected);
    assert_eq!(deployed_state.active_version.as_deref(), Some("1.0.0"));
    assert!(!test_root.path().join("robot-state/releases/2.0.0").exists());
}

#[test]
fn startup_recovers_interrupted_activation() {
    let test_root = tempfile::tempdir().unwrap();
    let robot_agent = apply_first_release(&test_root);
    let mut stored_state: RobotState = robot_agent.state();
    stored_state.previous_version = Some("1.0.0".to_owned());
    stored_state.active_version = Some("2.0.0".to_owned());
    stored_state.pending_version = Some("2.0.0".to_owned());
    stored_state.phase = DeploymentPhase::Activating;

    let state_path = test_root.path().join("robot-state/state.json");
    fs::write(state_path, serde_json::to_vec_pretty(&stored_state).unwrap()).unwrap();
    let recovered_agent = DeploymentEngine::open(test_root.path().join("robot-state"), "robot-1").unwrap();

    assert_eq!(recovered_agent.state().phase, DeploymentPhase::Recovered);
    assert_eq!(recovered_agent.state().active_version.as_deref(), Some("1.0.0"));
    assert_eq!(recovered_agent.state().pending_version, None);
}

#[test]
fn startup_preserves_active_release_if_interrupted_during_staging() {
    let test_root = tempfile::tempdir().unwrap();
    let robot_agent = apply_first_release(&test_root);
    let mut stored_state = robot_agent.state();
    stored_state.pending_version = Some("2.0.0".to_owned());
    stored_state.phase = DeploymentPhase::Staging;

    let state_path = test_root.path().join("robot-state/state.json");
    fs::write(state_path, serde_json::to_vec_pretty(&stored_state).unwrap()).unwrap();
    let recovered_agent = DeploymentEngine::open(test_root.path().join("robot-state"), "robot-1").unwrap();

    assert_eq!(recovered_agent.state().phase, DeploymentPhase::Recovered);
    assert_eq!(recovered_agent.state().active_version.as_deref(), Some("1.0.0"));
}

#[test]
fn unsafe_release_identifier_is_rejected() {
    let test_root = tempfile::tempdir().unwrap();
    let mut robot_agent = apply_first_release(&test_root);
    let second_binary = write_test_binary(test_root.path(), "2.0.0", true);

    let rejection = robot_agent.deploy(release_request(&second_binary, "../outside"));
    assert!(rejection.is_err());
    assert_eq!(robot_agent.state().active_version.as_deref(), Some("1.0.0"));
}

#[test]
fn startup_cleans_orphaned_staging_artifacts() {
    let test_root = tempfile::tempdir().unwrap();
    let robot_agent = apply_first_release(&test_root);
    let mut stored_state = robot_agent.state();
    stored_state.pending_version = Some("2.0.0".to_owned());
    stored_state.phase = DeploymentPhase::Staging;

    let releases_root = test_root.path().join("robot-state/releases");
    let orphaned_staging = releases_root.join(".2.0.0.staging");
    let orphaned_release = releases_root.join("2.0.0");
    fs::create_dir(&orphaned_staging).unwrap();
    fs::create_dir(&orphaned_release).unwrap();
    let state_path = test_root.path().join("robot-state/state.json");
    fs::write(state_path, serde_json::to_vec_pretty(&stored_state).unwrap()).unwrap();

    let recovered_agent = DeploymentEngine::open(test_root.path().join("robot-state"), "robot-1").unwrap();
    assert_eq!(recovered_agent.state().active_version.as_deref(), Some("1.0.0"));
    assert!(!orphaned_staging.exists());
    assert!(!orphaned_release.exists());
}

#[test]
fn health_check_timeout_preserves_last_healthy_release() {
    let test_root = tempfile::tempdir().unwrap();
    let mut robot_agent = apply_first_release(&test_root);
    let slow_binary = test_root.path().join("slow.sh");
    fs::write(&slow_binary, "#!/bin/sh\nsleep 5\n").unwrap();
    fs::set_permissions(&slow_binary, fs::Permissions::from_mode(0o755)).unwrap();
    let mut deployment = release_request(&slow_binary, "2.0.0");
    deployment.health_timeout_ms = Some(150);
    let robot_state = robot_agent.deploy(deployment).unwrap();
    assert_eq!(robot_state.phase, DeploymentPhase::RolledBack);
    assert_eq!(robot_state.active_version.as_deref(), Some("1.0.0"));
}

#[test]
fn absolute_health_command_is_rejected() {
    let test_root = tempfile::tempdir().unwrap();
    let mut robot_agent = apply_first_release(&test_root);
    let next_binary = write_test_binary(test_root.path(), "2.0.0", true);
    let mut deployment = release_request(&next_binary, "2.0.0");
    deployment.health_command = Some(vec!["/usr/bin/true".to_owned()]);
    assert!(robot_agent.deploy(deployment).is_err());
}

#[test]
fn runtime_failure_reverts_active_version() {
    let test_root = tempfile::tempdir().unwrap();
    let mut robot_agent = apply_first_release(&test_root);
    let second_binary = write_test_binary(test_root.path(), "2.0.0", true);
    let mut deployment = release_request(&second_binary, "2.0.0");
    deployment.run_command = Some(vec!["robot-app".to_owned(), "--serve".to_owned()]);
    assert_eq!(robot_agent.deploy(deployment).unwrap().phase, DeploymentPhase::Healthy);
    let command = robot_agent.active_run_command().unwrap().unwrap();
    assert_eq!(command.0, "2.0.0");
    assert_eq!(robot_agent.mark_runtime_failure("2.0.0").unwrap().active_version.as_deref(), Some("1.0.0"));
}
