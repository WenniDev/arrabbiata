use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

const CONFIG_FILE: &str = "arrabbiata.toml";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Configuration {
    #[serde(default)]
    pub general: GeneralConfiguration,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfiguration {
    #[serde(default = "default_true")]
    pub enable: bool,
}

impl Default for GeneralConfiguration {
    fn default() -> Self {
        Self { enable: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DumpConfiguration {
    #[serde(default = "default_directory")]
    pub directory: PathBuf,
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

fn default_directory() -> PathBuf {
    PathBuf::from("arrabbiata-dumps")
}

fn default_max_size() -> usize {
    16 * 1024 * 1024
}
