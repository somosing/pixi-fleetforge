use axum::body::Bytes;
use axum::extract::{Path as UrlPath, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use fleet_core::{
    sha256_file, AgentHeartbeat, DeploymentEngine, DeploymentRequest, ReleaseTarget, RobotState,
};
use serde::Serialize;
use std::fs;
use std::net::SocketAddr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

type ApiFailure = (StatusCode, Json<ApiError>);

#[derive(Serialize)]
struct ApiError {
    message: String,
}

struct AgentRuntime {
    engine: Arc<Mutex<DeploymentEngine>>,
    state_root: PathBuf,
    robot_id: String,
    listen_address: SocketAddr,
    token: Option<String>,
}

type SharedAgent = Arc<AgentRuntime>;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<String> = std::env::args().collect();
    if !(arguments.len() == 4 || arguments.len() == 5) {
        eprintln!("Usage: fleet-agent <robot-id> <state-directory> <listen-address> [hub-url]");
        std::process::exit(2);
    }
    let robot_id = arguments[1].clone();
    let state_root = PathBuf::from(&arguments[2]);
    let listen_address: SocketAddr = arguments[3].parse()?;
    if !listen_address.ip().is_loopback() {
        return Err("FleetForge demo agents must bind to loopback".into());
    }
    let token = std::env::var("FLEETFORGE_TOKEN").ok();
    if arguments.len() == 5 && token.as_deref().is_none_or(|secret| secret.len() < 16) {
        return Err("Set FLEETFORGE_TOKEN to at least 16 characters for hub mode".into());
    }

    let deployment_engine = DeploymentEngine::open(&state_root, &robot_id)?;
    fs::create_dir_all(state_root.join("cache"))?;
    let shared_agent = Arc::new(AgentRuntime {
        engine: Arc::new(Mutex::new(deployment_engine)),
        state_root,
        robot_id: robot_id.clone(),
        listen_address,
        token,
    });

    if let Some(hub_url) = arguments.get(4) {
        let reconcile_agent = Arc::clone(&shared_agent);
        let reconcile_hub_url = hub_url.trim_end_matches('/').to_owned();
        tokio::spawn(async move { reconcile_loop(reconcile_agent, reconcile_hub_url).await });
    }

    let supervisor_agent = Arc::clone(&shared_agent);
    tokio::spawn(async move { supervise_loop(supervisor_agent).await });

    let routes = Router::new()
        .route("/healthz", get(|| async { StatusCode::OK }))
        .route("/state", get(get_state))
        .route("/deploy", post(deploy_release))
        .route("/api/artifacts/{sha256}", get(get_cached_artifact))
        .with_state(shared_agent);
    let listener = tokio::net::TcpListener::bind(listen_address).await?;
    println!("{robot_id}: listening on http://{listen_address}");
    axum::serve(listener, routes).await?;
    Ok(())
}

async fn get_state(State(agent): State<SharedAgent>) -> Result<Json<RobotState>, ApiFailure> {
    let engine = agent
        .engine
        .lock()
        .map_err(|_| internal_error("Agent state lock poisoned"))?;
    Ok(Json(engine.state()))
}

async fn deploy_release(
    State(agent): State<SharedAgent>,
    headers: HeaderMap,
    Json(deployment): Json<DeploymentRequest>,
) -> Result<Json<RobotState>, ApiFailure> {
    authorized(&headers, &agent)?;
    let deployment_engine = Arc::clone(&agent.engine);
    tokio::task::spawn_blocking(move || {
        let mut engine = deployment_engine
            .lock()
            .map_err(|_| internal_error("Agent state lock poisoned"))?;
        engine
            .deploy(deployment)
            .map(Json)
            .map_err(|deployment_error| {
                let status_code = if deployment_error.is_invalid_request() {
                    StatusCode::BAD_REQUEST
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                };
                (
                    status_code,
                    Json(ApiError {
                        message: deployment_error.to_string(),
                    }),
                )
            })
    })
    .await
    .map_err(|worker_error| internal_error(&format!("Deployment task failed: {worker_error}")))?
}

async fn get_cached_artifact(
    State(agent): State<SharedAgent>,
    headers: HeaderMap,
    UrlPath(sha256): UrlPath<String>,
) -> Result<Bytes, ApiFailure> {
    authorized(&headers, &agent)?;
    if !valid_digest(&sha256) {
        return Err(bad_request("Invalid artifact digest"));
    }
    let artifact_path = agent
        .state_root
        .join("cache")
        .join(sha256.to_ascii_lowercase());
    let content = fs::read(artifact_path).map_err(|_| {
        (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                message: "Artifact not cached".to_owned(),
            }),
        )
    })?;
    Ok(Bytes::from(content))
}

async fn reconcile_loop(agent: SharedAgent, hub_url: String) {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
    {
        Ok(client) => client,
        Err(client_error) => {
            eprintln!(
                "{}: HTTP initialization failed: {client_error}",
                agent.robot_id
            );
            return;
        }
    };
    loop {
        if let Err(reconcile_error) = reconcile_once(&client, &agent, &hub_url).await {
            eprintln!(
                "{}: reconciliation deferred: {reconcile_error}",
                agent.robot_id
            );
        }
        tokio::time::sleep(Duration::from_secs(4)).await;
    }
}

async fn reconcile_once(
    client: &reqwest::Client,
    agent: &SharedAgent,
    hub_url: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let token = agent.token.as_deref().ok_or("Missing fleet token")?;
    let latest_state = agent
        .engine
        .lock()
        .map_err(|_| "Agent state lock poisoned")?
        .state();
    let heartbeat = AgentHeartbeat {
        state: latest_state.clone(),
        endpoint: Some(format!("http://{}", agent.listen_address)),
    };
    client
        .post(format!("{hub_url}/api/heartbeat"))
        .bearer_auth(token)
        .json(&heartbeat)
        .send()
        .await?
        .error_for_status()?;

    let desired: Option<ReleaseTarget> = client
        .get(format!("{hub_url}/api/desired/{}", agent.robot_id))
        .bearer_auth(token)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let Some(target) = desired else {
        return Ok(());
    };
    if latest_state.active_version.as_deref() == Some(&target.version)
        || latest_state.last_attempted_version.as_deref() == Some(&target.version)
    {
        return Ok(());
    }
    if !valid_digest(&target.sha256) {
        return Err("Desired artifact digest is invalid".into());
    }
    let artifact_path = agent
        .state_root
        .join("cache")
        .join(target.sha256.to_ascii_lowercase());
    if !valid_cached_artifact(&artifact_path, &target.sha256) {
        let configured_peers = std::env::var("FLEETFORGE_PEERS").unwrap_or_default();
        let mut sources: Vec<String> = configured_peers
            .split(',')
            .map(str::trim)
            .filter(|endpoint| !endpoint.is_empty())
            .map(str::to_owned)
            .collect();
        sources.push(hub_url.to_owned());
        let mut downloaded = false;
        for source in sources {
            let artifact_url = if source == hub_url {
                format!("{source}/api/artifacts/{}", target.sha256)
            } else {
                format!(
                    "{}/api/artifacts/{}",
                    source.trim_end_matches('/'),
                    target.sha256
                )
            };
            let response = client.get(artifact_url).bearer_auth(token).send().await;
            let Ok(response) = response else {
                continue;
            };
            let Ok(response) = response.error_for_status() else {
                continue;
            };
            let Ok(content) = response.bytes().await else {
                continue;
            };
            if content.len() > 100 * 1024 * 1024 {
                continue;
            }
            let pending_path = artifact_path.with_extension("download");
            fs::write(&pending_path, &content)?;
            if valid_cached_artifact(&pending_path, &target.sha256) {
                fs::rename(pending_path, &artifact_path)?;
                downloaded = true;
                break;
            }
            let _ = fs::remove_file(&pending_path);
        }
        if !downloaded {
            return Err("Artifact unavailable from the hub and configured peers".into());
        }
    }
    if matches!(target.artifact_kind, fleet_core::ArtifactKind::Executable) {
        fs::set_permissions(&artifact_path, fs::Permissions::from_mode(0o755))?;
    }
    let deployment_engine = Arc::clone(&agent.engine);
    let deployed_state = tokio::task::spawn_blocking(
        move || -> Result<RobotState, Box<dyn std::error::Error + Send + Sync>> {
            let mut engine = deployment_engine
                .lock()
                .map_err(|_| "Agent state lock poisoned")?;
            Ok(engine.deploy(target.local_request(artifact_path))?)
        },
    )
    .await??;
    client
        .post(format!("{hub_url}/api/heartbeat"))
        .bearer_auth(token)
        .json(&AgentHeartbeat {
            state: deployed_state,
            endpoint: Some(format!("http://{}", agent.listen_address)),
        })
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

fn valid_cached_artifact(artifact_path: &Path, expected_digest: &str) -> bool {
    artifact_path.is_file()
        && sha256_file(artifact_path)
            .is_ok_and(|digest| digest.eq_ignore_ascii_case(expected_digest))
}

fn valid_digest(digest: &str) -> bool {
    digest.len() == 64
        && digest
            .bytes()
            .all(|character| character.is_ascii_hexdigit())
}

fn authorized(headers: &HeaderMap, agent: &AgentRuntime) -> Result<(), ApiFailure> {
    if let Some(token) = agent.token.as_ref() {
        let supplied = headers
            .get(header::AUTHORIZATION)
            .and_then(|header| header.to_str().ok());
        if supplied != Some(format!("Bearer {token}").as_str()) {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(ApiError {
                    message: "Invalid fleet token".to_owned(),
                }),
            ));
        }
    }
    Ok(())
}

fn internal_error(message: &str) -> ApiFailure {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            message: message.to_owned(),
        }),
    )
}

fn bad_request(message: &str) -> ApiFailure {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiError {
            message: message.to_owned(),
        }),
    )
}

async fn supervise_loop(agent: SharedAgent) {
    let mut current_process: Option<(String, tokio::process::Child)> = None;
    loop {
        let desired_process = match agent.engine.lock() {
            Ok(engine) => engine.active_run_command(),
            Err(_) => {
                eprintln!("{}: supervisor lock poisoned", agent.robot_id);
                break;
            }
        };
        match desired_process {
            Ok(Some((version, release_root, command))) => {
                if current_process
                    .as_ref()
                    .is_some_and(|(running_version, _)| running_version != &version)
                {
                    if let Some((_, mut process)) = current_process.take() {
                        let _ = process.kill().await;
                        let _ = process.wait().await;
                    }
                }
                if current_process.is_none() {
                    match tokio::process::Command::new(release_root.join(&command[0]))
                        .args(&command[1..])
                        .current_dir(&release_root)
                        .kill_on_drop(true)
                        .spawn()
                    {
                        Ok(process) => current_process = Some((version.clone(), process)),
                        Err(spawn_error) => {
                            eprintln!(
                                "{}: release {version} launch failed: {spawn_error}",
                                agent.robot_id
                            );
                            record_runtime_failure(&agent, &version);
                        }
                    }
                }
            }
            Ok(None) => {
                if let Some((_, mut process)) = current_process.take() {
                    let _ = process.kill().await;
                    let _ = process.wait().await;
                }
            }
            Err(read_error) => eprintln!(
                "{}: supervisor manifest error: {read_error}",
                agent.robot_id
            ),
        }
        if let Some((version, process)) = current_process.as_mut() {
            match process.try_wait() {
                Ok(Some(exit_status)) => {
                    eprintln!(
                        "{}: runtime {version} exited: {exit_status}",
                        agent.robot_id
                    );
                    let failed_version = version.clone();
                    record_runtime_failure(&agent, &failed_version);
                    current_process = None;
                }
                Err(wait_error) => {
                    eprintln!(
                        "{}: runtime process check failed: {wait_error}",
                        agent.robot_id
                    );
                    let failed_version = version.clone();
                    record_runtime_failure(&agent, &failed_version);
                    current_process = None;
                }
                Ok(None) => {}
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

fn record_runtime_failure(agent: &AgentRuntime, version: &str) {
    match agent.engine.lock() {
        Ok(mut engine) => {
            if let Err(recovery_error) = engine.mark_runtime_failure(version) {
                eprintln!(
                    "{}: runtime failure recovery error: {recovery_error}",
                    agent.robot_id
                );
            }
        }
        Err(_) => eprintln!("{}: unable to record runtime failure", agent.robot_id),
    }
}
