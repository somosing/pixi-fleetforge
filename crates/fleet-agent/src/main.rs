use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use fleet_core::{DeploymentEngine, DeploymentRequest, RobotState};
use serde::Serialize;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Serialize)]
struct ApiError {
    message: String,
}

type SharedEngine = Arc<Mutex<DeploymentEngine>>;
type ApiFailure = (StatusCode, Json<ApiError>);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli_args: Vec<String> = std::env::args().collect();
    if cli_args.len() != 4 {
        eprintln!("Usage: fleet-agent <robot-id> <state-directory> <listen-address>");
        std::process::exit(2);
    }

    let robot_id = &cli_args[1];
    let state_root = PathBuf::from(&cli_args[2]);
    let listen_address: SocketAddr = cli_args[3].parse()?;
    if !listen_address.ip().is_loopback() {
        return Err("This prototype only accepts loopback listeners".into());
    }

    let deployment_engine = DeploymentEngine::open(state_root, robot_id)?;
    let shared_engine = Arc::new(Mutex::new(deployment_engine));
    let api_router = Router::new()
        .route("/healthz", get(health_check))
        .route("/state", get(get_state))
        .route("/deploy", post(deploy_release))
        .with_state(shared_engine);

    let tcp_listener = tokio::net::TcpListener::bind(listen_address).await?;
    println!("{robot_id}: listening on http://{listen_address}");
    axum::serve(tcp_listener, api_router).await?;
    Ok(())
}

async fn health_check() -> StatusCode {
    StatusCode::OK
}

async fn get_state(State(shared_engine): State<SharedEngine>) -> Result<Json<RobotState>, ApiFailure> {
    let deployment_engine = shared_engine.lock().map_err(|_| internal_error("Agent state lock poisoned"))?;
    Ok(Json(deployment_engine.state()))
}

async fn deploy_release(
    State(shared_engine): State<SharedEngine>,
    Json(deployment): Json<DeploymentRequest>,
) -> Result<Json<RobotState>, ApiFailure> {
    tokio::task::spawn_blocking(move || {
        let mut deployment_engine = shared_engine.lock()
            .map_err(|_| internal_error("Agent state lock poisoned"))?;

        deployment_engine.deploy(deployment).map(Json).map_err(|deployment_error| {
            let status_code = if deployment_error.is_invalid_request() {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status_code, Json(ApiError { message: deployment_error.to_string() }))
        })
    })
    .await
    .map_err(|worker_error| internal_error(&format!("Deployment task failed: {worker_error}")))?
}

fn internal_error(description: &str) -> ApiFailure {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError { message: description.to_owned() }))
}
