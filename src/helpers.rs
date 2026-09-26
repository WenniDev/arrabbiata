//! Talking to Tachi.

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

pub fn request_agent() -> ureq::Agent {
    let timeout = CONFIGURATION.general.timeout.min(10_000);

    let config = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_millis(timeout)))
        .user_agent(USER_AGENT.as_str())
        .build();

    ureq::Agent::new_with_config(config)
}

/// POSTs a body to Tachi and returns the decoded response.
///
/// Tachi answers a rejected import with HTTP 200 and `success: false`, so the caller has to
/// read the body rather than trust the status.
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
