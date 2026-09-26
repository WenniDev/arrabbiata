use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

const CONFIG_FILE: &str = "arrabbiata.toml";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Configuration {
    #[serde(default)]
    pub general: GeneralConfiguration,
    #[serde(default)]
    pub tachi: TachiConfiguration,
    /// Extra API keys selected by `refid`. Konasute has no `cardmng`, so a player is
    /// identified by the `refid` their own save carries.
    #[serde(default)]
    pub profiles: HashMap<String, ProfileConfiguration>,
    #[serde(default)]
    pub dump: DumpConfiguration,
}

impl Configuration {
    pub fn load() -> Result<Self> {
        if !Path::new(CONFIG_FILE).exists() {
            File::create(CONFIG_FILE)
                .and_then(|mut file| file.write_all(include_bytes!("../arrabbiata.toml")))
                .map_err(|err| anyhow::anyhow!("Could not create default config file: {err}"))?;
        }

        confy::load_path(CONFIG_FILE).map_err(|err| anyhow::anyhow!("Could not load config: {err}"))
    }

    /// The API key to submit a given player's scores under, or `None` if that player should
    /// not be submitted at all.
    pub fn api_key_for(&self, refid: &str) -> Option<&str> {
        let from_profile = self
            .profiles
            .values()
            .find(|profile| profile.refids.iter().any(|known| known == refid))
            .map(|profile| profile.api_key.as_str());

        from_profile
            .or(self.tachi.api_key.as_deref())
            .filter(|key| !key.is_empty())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfiguration {
    #[serde(default = "default_true")]
    pub enable: bool,
    /// Set to `false` to watch traffic without sending anything anywhere.
    #[serde(default = "default_true")]
    pub submit: bool,
    #[serde(default = "default_timeout")]
    pub timeout: u64,
}

impl Default for GeneralConfiguration {
    fn default() -> Self {
        Self {
            enable: true,
            submit: true,
            timeout: default_timeout(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TachiConfiguration {
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default = "default_import_endpoint")]
    pub import: String,
    #[serde(default)]
    pub api_key: Option<String>,
    /// Tachi has no `grandprix` version for `ddr`. Left unset so Tachi resolves charts
    /// across versions rather than filing scores under one that was guessed at.
    #[serde(default)]
    pub version: Option<String>,
}

impl Default for TachiConfiguration {
    fn default() -> Self {
        Self {
            base_url: default_base_url(),
            import: default_import_endpoint(),
            api_key: None,
            version: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfileConfiguration {
    #[serde(default)]
    pub refids: Vec<String>,
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DumpConfiguration {
    #[serde(default = "default_directory")]
    pub directory: PathBuf,
    /// Dump every property the hooks see. This is how the protocol was worked out; leave it
    /// off for normal play.
    #[serde(default)]
    pub all: bool,
    /// Always dump a payload that could not be turned into a score. Each refusal is then
    /// something that can be diagnosed rather than a score silently lost.
    #[serde(default = "default_true")]
    pub on_refusal: bool,
    #[serde(default = "default_true")]
    pub on_destroy: bool,
    #[serde(default = "default_true")]
    pub on_write: bool,
    #[serde(default)]
    pub write_kbin: bool,
    #[serde(default)]
    pub roots: Vec<String>,
    #[serde(default)]
    pub filter: Vec<String>,
    #[serde(default = "default_max_size")]
    pub max_size: usize,
}

impl Default for DumpConfiguration {
    fn default() -> Self {
        Self {
            directory: default_directory(),
            all: false,
            on_refusal: true,
            on_destroy: true,
            on_write: true,
            write_kbin: false,
            roots: Vec::new(),
            filter: Vec::new(),
            max_size: default_max_size(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_timeout() -> u64 {
    3000
}

fn default_base_url() -> String {
    // Kamaitachi's canonical host. The older kamaitachi.xyz still answers, but with a 308
    // to this one, and a cross-host redirect drops the Authorization header -- so inheriting
    // upstream's URL meant a default install could never authenticate.
    "https://kamai.tachi.ac/".to_string()
}

fn default_import_endpoint() -> String {
    "/ir/direct-manual/import".to_string()
}

fn default_directory() -> PathBuf {
    PathBuf::from("arrabbiata-dumps")
}

fn default_max_size() -> usize {
    16 * 1024 * 1024
}
