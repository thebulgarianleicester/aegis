use crate::{http::Response, AppState};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct SimulatorRequest {
    workload: String,
}

pub fn route(state: &AppState, request: &crate::http::Request) -> Response {
    if request.method == "OPTIONS" {
        return Response {
            status: 200,
            content_type: "text/plain".to_owned(),
            body: Vec::new(),
        };
    }
    let result = match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/api/v1/status") => state
            .telemetry()
            .map(|telemetry| Response::json(&telemetry.status)),
        ("GET", "/api/v1/telemetry") => state.telemetry().map(|value| Response::json(&value)),
        ("GET", "/api/v1/processes") => state
            .telemetry()
            .map(|telemetry| Response::json(&telemetry.processes)),
        ("GET", "/api/v1/journal") => state
            .telemetry()
            .map(|telemetry| Response::json(&telemetry.ledger)),
        ("GET", "/api/v1/runs") => state
            .telemetry()
            .map(|telemetry| Response::json(&telemetry.runs)),
        ("GET", "/api/v1/canaries") => state
            .telemetry()
            .map(|telemetry| Response::json(&telemetry.canaries)),
        ("GET", "/api/v1/alerts") => state
            .telemetry()
            .map(|telemetry| Response::json(&telemetry.alerts)),
        ("GET", "/api/v1/stream") => Ok(Response::error(
            426,
            "use a WebSocket upgrade for the live telemetry stream",
        )),
        ("POST", "/api/v1/canaries/deploy") => {
            state.deploy_canaries().map(|value| Response::json(&value))
        }
        ("POST", "/api/v1/simulator/run") => match parse_body(request) {
            Ok(body) => state
                .start_simulation(&body.workload)
                .map(|value| Response::json(&value)),
            Err(response) => return response,
        },
        ("POST", path) => {
            let parts: Vec<&str> = path.trim_matches('/').split('/').collect();
            match parts.as_slice() {
                ["api", "v1", "processes", ragid, "contain"] => {
                    state.contain(ragid).map(|value| Response::json(&value))
                }
                ["api", "v1", "processes", ragid, "rollback"] => {
                    state.rollback(ragid).map(|value| Response::json(&value))
                }
                _ => Ok(Response::error(404, "unknown API endpoint")),
            }
        }
        ("GET", _) => Ok(Response::error(404, "unknown API endpoint")),
        _ => Ok(Response::error(405, "method not allowed")),
    };
    result.unwrap_or_else(|error| {
        let message = format!("{error:#}");
        let status = if message.contains("already active") || message.contains("unknown") {
            409
        } else {
            400
        };
        Response::error(status, message)
    })
}

fn parse_body(request: &crate::http::Request) -> Result<SimulatorRequest, Response> {
    serde_json::from_slice(&request.body).map_err(|error| {
        Response::error(
            400,
            format!("expected JSON {{\"workload\":\"intermittent_encryption\"}}: {error}"),
        )
    })
}
