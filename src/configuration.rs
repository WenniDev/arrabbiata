use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

const CONFIG_FILE: &str = "arrabbiata.toml";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Configuration {
    #[serde(default)]
    pub general: GeneralConfiguration,
    #[serde(default)]
    pub tachi: TachiConfiguration,
    /// Extra API keys selected by `refid`, which is how a save names its player.
    #[serde(default)]
    pub profiles: HashMap<String, ProfileConfiguration>,
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
        self.profiles
            .values()
            .find(|profile| profile.refids.iter().any(|known| known == refid))
            .map(|profile| profile.api_key.as_str())
            .or(self.tachi.api_key.as_deref())
            .filter(|key| !key.is_empty())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfiguration {
    #[serde(default = "default_true")]
    pub enable: bool,
    #[serde(default = "default_timeout")]
    pub timeout: u64,
}

impl Default for GeneralConfiguration {
    fn default() -> Self {
        Self {
            enable: true,
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

fn default_true() -> bool {
    true
}

fn default_timeout() -> u64 {
    3000
}

fn default_base_url() -> String {
    // Kamaitachi's canonical host. kamaitachi.xyz answers with a 308 to this one, and a
    // cross-host redirect drops the Authorization header, so it can never authenticate.
    "https://kamai.tachi.ac/".to_string()
}

fn default_import_endpoint() -> String {
    "/ir/direct-manual/import".to_string()
}

#[cfg(test)]
mod tests {
    use super::Configuration;

    /// The default config ships inside the DLL through `include_bytes!`, so a typo in it
    /// surfaces only when a player starts the game.
    #[test]
    fn the_shipped_default_parses() {
        let path = std::env::temp_dir().join("arrabbiata-shipped-default.toml");
        std::fs::write(&path, include_bytes!("../arrabbiata.toml")).unwrap();

        let config: Configuration = confy::load_path(&path).expect("shipped default should parse");
        let _ = std::fs::remove_file(&path);

        assert!(config.general.enable);
        assert_eq!(config.tachi.base_url, "https://kamai.tachi.ac/");
        // Out of the box there is no key, so nothing is submitted.
        assert_eq!(config.api_key_for("0000001346040870"), None);
        // Unset, so Tachi matches charts across versions.
        assert_eq!(config.tachi.version, None);
    }
}
