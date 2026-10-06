use anyhow::Result;
use log::debug;
use serde::Serialize;
use std::fmt::Debug;
use std::sync::LazyLock;

use crate::CONFIGURATION;

static USER_AGENT: LazyLock<String> = LazyLock::new(|| {
    format!(
        "arrabbiata-{}/{}",
        env!("CARGO_PKG_VERSION"),
        option_env!("VERGEN_GIT_DESCRIBE").unwrap_or("unknown")
    )
});

/// `status_as_error` off hands 4xx and 5xx back as responses instead of errors.
fn agent(timeout: u64, status_as_error: bool) -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_millis(timeout)))
        .user_agent(USER_AGENT.as_str())
        .http_status_as_error(status_as_error)
        .build();

    ureq::Agent::new_with_config(config)
}

pub fn request_agent() -> ureq::Agent {
    agent(CONFIGURATION.tachi.timeout, true)
}

/// POSTs to Tachi. A rejected import still answers 200, so the caller must read `success`.
pub fn post<T>(url: &str, api_key: &str, body: &T) -> Result<serde_json::Value>
where
    T: Serialize + Debug,
{
    debug!("POST {url} with body: {body:#?}");

    let request = ureq::http::Request::builder()
        .method("POST")
        .uri(url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .body(serde_json::to_vec(body)?)?;

    let mut response = request_agent()
        .run(request)
        .map_err(|err| anyhow::anyhow!("could not reach Tachi: {err:#}"))?;

    let body: serde_json::Value = response
        .body_mut()
        .read_json()
        .map_err(|err| anyhow::anyhow!("could not read Tachi's response: {err:#}"))?;

    Ok(body)
}

/// GETs JSON, with no key: the poll URL is Tachi's own answer and takes no authorization.
pub fn get(url: &str) -> Result<serde_json::Value> {
    debug!("GET {url}");

    let mut response = request_agent()
        .get(url)
        .call()
        .map_err(|err| anyhow::anyhow!("could not reach Tachi: {err:#}"))?;

    response
        .body_mut()
        .read_json()
        .map_err(|err| anyhow::anyhow!("could not read Tachi's response: {err:#}"))
}

/// POSTs to Upscore, which answers the outcome in the status and a JSON tally, not in `success`.
pub fn post_for_outcome<T>(url: &str, api_key: &str, body: &T) -> Result<(u16, serde_json::Value)>
where
    T: Serialize + Debug,
{
    debug!("POST {url} with body: {body:#?}");

    let request = ureq::http::Request::builder()
        .method("POST")
        .uri(url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .body(serde_json::to_vec(body)?)?;

    let mut response = agent(CONFIGURATION.upscore.timeout, false)
        .run(request)
        .map_err(|err| anyhow::anyhow!("could not reach Upscore: {err:#}"))?;

    let status = response.status().as_u16();
    // A gateway failing in front of Upscore answers HTML, which is no reason to lose the status.
    let body = response.body_mut().read_json().unwrap_or_default();

    Ok((status, body))
}
