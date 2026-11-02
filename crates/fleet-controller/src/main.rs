use fleet_core::{sha256_file, DeploymentRequest, RolloutRequest};
use std::fs::{self, File};
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;
use tokio::task::JoinSet;

#[tokio::main]
async fn main() -> ExitCode {
    match operate_fleet().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure_description) => {
            eprintln!("fleet-controller: {failure_description}");
            ExitCode::FAILURE
        }
    }
}

async fn operate_fleet() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()?;
    let token = std::env::var("FLEETFORGE_TOKEN").ok();
    match arguments.as_slice() {
        [command, agents @ ..] if command == "status" && !agents.is_empty() => {
            dispatch_requests(&client, "GET", agents, None, token.as_deref()).await
        }
        [command, manifest, agents @ ..] if command == "deploy" && !agents.is_empty() => {
            let deployment: DeploymentRequest = serde_json::from_reader(File::open(manifest)?)?;
            dispatch_requests(&client, "POST", agents, Some(serde_json::to_value(deployment)?), token.as_deref()).await
        }
        [command, hub] if command == "fleet" => {
            let response = authenticated(client.get(format!("{hub}/api/fleet")), token.as_deref())
                .send().await?.error_for_status()?;
            println!("{}", serde_json::to_string_pretty(&response.json::<serde_json::Value>().await?)?);
            Ok(())
        }
        [command, artifact, hub] if command == "publish" => {
            let artifact_path = Path::new(artifact);
            let digest = sha256_file(artifact_path)?;
            let content = fs::read(artifact_path)?;
            let response = authenticated(
                client.put(format!("{hub}/api/artifacts/{digest}")).body(content),
                token.as_deref(),
            ).send().await?.error_for_status()?;
            println!("Published artifact sha256:{digest} [{}]", response.status());
            Ok(())
        }
        [command, rollout_file, hub] if command == "rollout" => {
            let rollout: RolloutRequest = serde_json::from_reader(File::open(rollout_file)?)?;
            let response = authenticated(
                client.post(format!("{hub}/api/desired")).json(&rollout),
                token.as_deref(),
            ).send().await?.error_for_status()?;
            println!("Rollout accepted [{}]", response.status());
            Ok(())
        }
        _ => Err("Usage: fleet-controller status <agent-url>... | deploy <manifest.json> <agent-url>... | publish <artifact> <hub-url> | rollout <rollout.json> <hub-url> | fleet <hub-url>".into()),
    }
}

fn authenticated(request: reqwest::RequestBuilder, token: Option<&str>) -> reqwest::RequestBuilder {
    if let Some(token) = token {
        request.bearer_auth(token)
    } else {
        request
    }
}

async fn dispatch_requests(
    http_client: &reqwest::Client,
    method: &str,
    agent_urls: &[String],
    manifest: Option<serde_json::Value>,
    token: Option<&str>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut pending_requests = JoinSet::new();
    for agent_url in agent_urls {
        let agent_url = agent_url.trim_end_matches('/').to_owned();
        let endpoint = if method == "POST" { "deploy" } else { "state" };
        let endpoint_url = format!("{agent_url}/{endpoint}");
        let client = http_client.clone();
        let agent_manifest = manifest.clone();
        let access_token = token.map(str::to_owned);
        let is_deployment = method == "POST";
        pending_requests.spawn(async move {
            let request = if is_deployment {
                client.post(&endpoint_url).json(&agent_manifest)
            } else {
                client.get(&endpoint_url)
            };
            let response = authenticated(request, access_token.as_deref())
                .send()
                .await?;
            let status = response.status();
            let body = response.text().await?;
            Ok::<_, reqwest::Error>((agent_url, status, body))
        });
    }
    let mut failure_count = 0;
    while let Some(join_outcome) = pending_requests.join_next().await {
        match join_outcome {
            Ok(Ok((agent_url, status, body))) => {
                println!("{agent_url} [{status}] {body}");
                if !status.is_success() {
                    failure_count += 1;
                }
            }
            Ok(Err(network_error)) => {
                eprintln!("Network failure: {network_error}");
                failure_count += 1;
            }
            Err(task_error) => {
                eprintln!("Agent request task failed: {task_error}");
                failure_count += 1;
            }
        }
    }
    if failure_count > 0 {
        Err(format!("{failure_count} agent requests failed").into())
    } else {
        Ok(())
    }
}
