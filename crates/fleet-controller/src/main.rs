use fleet_core::DeploymentRequest;
use std::fs::File;
use std::process::ExitCode;
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
    let cli_args: Vec<String> = std::env::args().skip(1).collect();
    let (http_method, robot_addresses) = match cli_args.as_slice() {
        [command, fleet_addresses @ ..] if command == "status" && !fleet_addresses.is_empty() => {
            ("GET", fleet_addresses.to_vec())
        }
        [command, manifest_path, fleet_addresses @ ..]
            if command == "deploy" && !fleet_addresses.is_empty() =>
        {
            let manifest_file = File::open(manifest_path)?;
            let deployment: DeploymentRequest = serde_json::from_reader(manifest_file)?;
            let request_json = serde_json::to_value(deployment)?;
            return dispatch_requests("POST", fleet_addresses, Some(request_json)).await;
        }
        _ => {
            return Err("Usage: fleet-controller status <agent-url>... | deploy <manifest.json> <agent-url>...".into());
        }
    };

    dispatch_requests(http_method, &robot_addresses, None).await
}

async fn dispatch_requests(
    http_method: &str,
    fleet_addresses: &[String],
    manifest_json: Option<serde_json::Value>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let mut pending_requests = JoinSet::new();

    for agent_address in fleet_addresses {
        let agent_address = agent_address.trim_end_matches('/').to_owned();
        let agent_http_client = http_client.clone();
        let agent_manifest = manifest_json.clone();
        let target_endpoint = if http_method == "POST" { "deploy" } else { "state" };
        let endpoint_url = format!("{agent_address}/{target_endpoint}");
        let is_deployment = http_method == "POST";

        pending_requests.spawn(async move {
            let http_request = if is_deployment {
                agent_http_client.post(&endpoint_url).json(&agent_manifest)
            } else {
                agent_http_client.get(&endpoint_url)
            };
            let http_response = http_request.send().await?;
            let status_code = http_response.status();
            let response_body = http_response.text().await?;
            Ok::<_, reqwest::Error>((agent_address, status_code, response_body))
        });
    }

    let mut failures = 0;
    while let Some(join_outcome) = pending_requests.join_next().await {
        match join_outcome {
            Ok(Ok((agent_address, status_code, response_body))) => {
                println!("{agent_address} [{status_code}] {response_body}");
                if !status_code.is_success() {
                    failures += 1;
                }
            }
            Ok(Err(network_error)) => {
                eprintln!("Network failure: {network_error}");
                failures += 1;
            }
            Err(task_error) => {
                eprintln!("Agent request task failed: {task_error}");
                failures += 1;
            }
        }
    }

    if failures > 0 {
        Err(format!("{failures} agent requests failed").into())
    } else {
        Ok(())
    }
}
