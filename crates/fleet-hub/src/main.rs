use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path as UrlPath, State};
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use fleet_core::{sha256_file, AgentHeartbeat, ReleaseTarget, RobotObservation, RolloutRequest};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tower_http::cors::{Any, CorsLayer};

type HubFailure = (StatusCode, String);

#[derive(Clone, Default, Deserialize, Serialize)]
struct HubStore {
    desired: BTreeMap<String, ReleaseTarget>,
    observed: BTreeMap<String, RobotObservation>,
}

struct HubState {
    root: PathBuf,
    bearer_token: String,
    registry: Mutex<HubStore>,
}

type SharedHub = Arc<HubState>;

#[derive(Serialize)]
struct FleetSnapshot {
    desired: BTreeMap<String, ReleaseTarget>,
    observed: BTreeMap<String, RobotObservation>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<String> = std::env::args().collect();
    if arguments.len() != 3 {
        eprintln!("Usage: fleet-hub <state-directory> <listen-address>");
        std::process::exit(2);
    }
    let listen_address: SocketAddr = arguments[2].parse()?;
    if !listen_address.ip().is_loopback() {
        return Err("FleetForge demo hub must bind to loopback".into());
    }
    let bearer_token = std::env::var("FLEETFORGE_TOKEN")?;
    if bearer_token.len() < 16 {
        return Err("Set FLEETFORGE_TOKEN to at least 16 characters".into());
    }
    let root = PathBuf::from(&arguments[1]);
    fs::create_dir_all(root.join("artifacts"))?;
    let registry_path = root.join("registry.json");
    let registry = if registry_path.exists() {
        serde_json::from_slice(&fs::read(registry_path)?)?
    } else {
        HubStore::default()
    };
    let shared_hub = Arc::new(HubState { root, bearer_token, registry: Mutex::new(registry) });
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::PUT])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]);
    let routes = Router::new()
        .route("/healthz", get(|| async { StatusCode::OK }))
        .route("/api/fleet", get(fleet_snapshot))
        .route("/api/desired", post(set_desired))
        .route("/api/desired/{robot_id}", get(get_desired))
        .route("/api/heartbeat", post(record_heartbeat))
        .route("/api/artifacts/{sha256}", get(get_artifact).put(put_artifact))
        .layer(DefaultBodyLimit::max(100 * 1024 * 1024))
        .layer(cors)
        .with_state(shared_hub);
    let listener = tokio::net::TcpListener::bind(listen_address).await?;
    println!("FleetForge hub listening at http://{listen_address}");
    axum::serve(listener, routes).await?;
    Ok(())
}

fn authorized(headers: &HeaderMap, hub: &HubState) -> Result<(), HubFailure> {
    let supplied = headers.get(header::AUTHORIZATION).and_then(|value| value.to_str().ok());
    if supplied != Some(format!("Bearer {}", hub.bearer_token).as_str()) {
        return Err((StatusCode::UNAUTHORIZED, "Invalid fleet access token".to_owned()));
    }
    Ok(())
}

fn validate_digest(sha256: &str) -> Result<(), HubFailure> {
    if sha256.len() == 64 && sha256.bytes().all(|character| character.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err((StatusCode::BAD_REQUEST, "Invalid artifact digest".to_owned()))
    }
}

async fn fleet_snapshot(
    State(hub): State<SharedHub>,
    headers: HeaderMap,
) -> Result<Json<FleetSnapshot>, HubFailure> {
    authorized(&headers, &hub)?;
    let registry = hub.registry.lock().map_err(lock_error)?;
    Ok(Json(FleetSnapshot { desired: registry.desired.clone(), observed: registry.observed.clone() }))
}

async fn set_desired(
    State(hub): State<SharedHub>,
    headers: HeaderMap,
    Json(rollout): Json<RolloutRequest>,
) -> Result<StatusCode, HubFailure> {
    authorized(&headers, &hub)?;
    validate_digest(&rollout.release.sha256)?;
    let version = &rollout.release.version;
    if version.is_empty() || version.len() > 64
        || !version.bytes().next().is_some_and(|letter| letter.is_ascii_alphanumeric())
        || !version.bytes().all(|letter| letter.is_ascii_alphanumeric() || matches!(letter, b'-' | b'_' | b'.')) {
        return Err((StatusCode::BAD_REQUEST, "Invalid release version".to_owned()));
    }
    if rollout.robot_ids.is_empty() || rollout.robot_ids.len() > 50 {
        return Err((StatusCode::BAD_REQUEST, "Rollout must target 1 to 50 robots".to_owned()));
    }
    if rollout.robot_ids.iter().any(|robot_id| !valid_robot_id(robot_id)) {
        return Err((StatusCode::BAD_REQUEST, "Invalid robot identifier".to_owned()));
    }
    let mut registry = hub.registry.lock().map_err(lock_error)?;
    for robot_id in rollout.robot_ids {
        registry.desired.insert(robot_id, rollout.release.clone());
    }
    persist_registry(&hub.root, &registry).map_err(io_error)?;
    Ok(StatusCode::ACCEPTED)
}

async fn get_desired(
    State(hub): State<SharedHub>,
    headers: HeaderMap,
    UrlPath(robot_id): UrlPath<String>,
) -> Result<Json<Option<ReleaseTarget>>, HubFailure> {
    authorized(&headers, &hub)?;
    let registry = hub.registry.lock().map_err(lock_error)?;
    Ok(Json(registry.desired.get(&robot_id).cloned()))
}

async fn record_heartbeat(
    State(hub): State<SharedHub>,
    headers: HeaderMap,
    Json(heartbeat): Json<AgentHeartbeat>,
) -> Result<StatusCode, HubFailure> {
    authorized(&headers, &hub)?;
    let robot_id = heartbeat.state.robot_id.clone();
    if !valid_robot_id(&robot_id) {
        return Err((StatusCode::BAD_REQUEST, "Invalid robot identifier".to_owned()));
    }
    let last_seen_ms = SystemTime::now().duration_since(UNIX_EPOCH)
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
        .as_millis() as u64;
    let mut registry = hub.registry.lock().map_err(lock_error)?;
    registry.observed.insert(robot_id, RobotObservation {
        state: heartbeat.state,
        last_seen_ms,
        endpoint: heartbeat.endpoint,
    });
    persist_registry(&hub.root, &registry).map_err(io_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn put_artifact(
    State(hub): State<SharedHub>,
    headers: HeaderMap,
    UrlPath(sha256): UrlPath<String>,
    payload: Bytes,
) -> Result<StatusCode, HubFailure> {
    authorized(&headers, &hub)?;
    validate_digest(&sha256)?;
    let canonical_digest = sha256.to_ascii_lowercase();
    let artifact_path = hub.root.join("artifacts").join(&canonical_digest);
    let staged_path = hub.root.join("artifacts").join(format!(".{canonical_digest}.upload"));
    fs::write(&staged_path, payload).map_err(io_error)?;
    let actual_digest = sha256_file(&staged_path).map_err(|error| io_error(std::io::Error::other(error)))?;
    if actual_digest != sha256.to_ascii_lowercase() {
        let _ = fs::remove_file(&staged_path);
        return Err((StatusCode::BAD_REQUEST, "Artifact SHA-256 mismatch".to_owned()));
    }
    fs::rename(&staged_path, artifact_path).map_err(io_error)?;
    Ok(StatusCode::CREATED)
}

async fn get_artifact(
    State(hub): State<SharedHub>,
    headers: HeaderMap,
    UrlPath(sha256): UrlPath<String>,
) -> Result<Bytes, HubFailure> {
    authorized(&headers, &hub)?;
    validate_digest(&sha256)?;
    let artifact_path = hub.root.join("artifacts").join(sha256.to_ascii_lowercase());
    let artifact_bytes = fs::read(artifact_path)
        .map_err(|_| (StatusCode::NOT_FOUND, "Artifact not cached".to_owned()))?;
    Ok(Bytes::from(artifact_bytes))
}

fn persist_registry(root: &Path, registry: &HubStore) -> Result<(), std::io::Error> {
    let pending_file = root.join("registry.json.tmp");
    let mut file = File::create(&pending_file)?;
    file.write_all(&serde_json::to_vec_pretty(registry).map_err(std::io::Error::other)?)?;
    file.sync_all()?;
    fs::rename(pending_file, root.join("registry.json"))?;
    File::open(root)?.sync_all()?;
    Ok(())
}

fn valid_robot_id(robot_id: &str) -> bool {
    !robot_id.is_empty() && robot_id.len() <= 64
        && robot_id.bytes().all(|character| character.is_ascii_alphanumeric() || matches!(character, b'-' | b'_'))
}

fn lock_error<T>(_: T) -> HubFailure {
    (StatusCode::INTERNAL_SERVER_ERROR, "Fleet registry lock failure".to_owned())
}

fn io_error(error: std::io::Error) -> HubFailure {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}
