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
    #[serde(default)]
    pub upscore: UpscoreConfiguration,
    /// Keyed by `refid`, and written last: in TOML a sub-table swallows the keys after it.
    #[serde(default)]
    pub profiles: HashMap<String, ProfileConfiguration>,
}

/// Whether a credential is set, which a blank one deliberately is not.
fn filled(credential: Option<&str>) -> bool {
    credential.is_some_and(|value| !value.is_empty())
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

    /// The profile claiming a refid, which is how a save names its player.
    fn profile_for(&self, refid: &str) -> Option<&ProfileConfiguration> {
        self.profiles
            .values()
            .find(|profile| profile.refids.iter().any(|known| known == refid))
    }

    /// The Tachi key for a player's scores, or `None` if they should not be submitted.
    pub fn tachi_api_key_for(&self, refid: &str) -> Option<&str> {
        self.profile_for(refid)
            .and_then(|profile| profile.tachi_api_key.as_deref())
            .or(self.tachi.api_key.as_deref())
            .filter(|key| !key.is_empty())
    }

    /// The Upscore key for a player's plays, or `None` if they should not be sent.
    pub fn upscore_api_key_for(&self, refid: &str) -> Option<&str> {
        self.profile_for(refid)
            .and_then(|profile| profile.upscore_api_key.as_deref())
            .or(self.upscore.api_key())
            .filter(|key| !key.is_empty())
    }

    /// Whether any player's scores can reach Tachi, for what the hook reports at startup.
    pub fn has_tachi_api_key(&self) -> bool {
        filled(self.tachi.api_key.as_deref())
            || self
                .profiles
                .values()
                .any(|profile| filled(profile.tachi_api_key.as_deref()))
    }

    /// Whether any player's plays can reach Upscore.
    pub fn has_upscore_api_key(&self) -> bool {
        filled(self.upscore.api_key())
            || self
                .profiles
                .values()
                .any(|profile| filled(profile.upscore_api_key.as_deref()))
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
pub struct TachiConfiguration {
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default = "default_import_endpoint")]
    pub import: String,
    #[serde(default)]
    pub api_key: Option<String>,
    /// Left unset so Tachi searches every version for the chart.
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default = "default_tachi_timeout")]
    pub timeout: u64,
}

impl Default for TachiConfiguration {
    fn default() -> Self {
        Self {
            base_url: default_base_url(),
            import: default_import_endpoint(),
            api_key: None,
            version: None,
            timeout: default_tachi_timeout(),
        }
    }
}

/// A player's own keys. Omitting one falls back to the section above, `''` sends nothing.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfileConfiguration {
    #[serde(default)]
    pub refids: Vec<String>,
    // Both are spelled out: a profile holds one key per service, so `api_key` alone is ambiguous.
    #[serde(default)]
    pub tachi_api_key: Option<String>,
    #[serde(default)]
    pub upscore_api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpscoreConfiguration {
    #[serde(default = "default_upscore_url")]
    pub url: String,
    #[serde(default)]
    pub api_key: Option<String>,
    /// Longer than Tachi's: Upscore scores the play before it answers, which takes seconds.
    #[serde(default = "default_upscore_timeout")]
    pub timeout: u64,
}

impl UpscoreConfiguration {
    /// The upload key, or `None` when it is unset or blank.
    pub fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref().filter(|key| !key.is_empty())
    }
}

impl Default for UpscoreConfiguration {
    fn default() -> Self {
        Self {
            url: default_upscore_url(),
            api_key: None,
            timeout: default_upscore_timeout(),
        }
    }
}

fn default_upscore_url() -> String {
    "https://hiikvrkmmwmbrxfxihvp.supabase.co/functions/v1/push-plays".to_string()
}

fn default_upscore_timeout() -> u64 {
    30000
}

fn default_true() -> bool {
    true
}

fn default_tachi_timeout() -> u64 {
    3000
}

fn default_base_url() -> String {
    "https://kamai.tachi.ac/".to_string()
}

fn default_import_endpoint() -> String {
    "/ir/direct-manual/import".to_string()
}

#[cfg(test)]
mod tests {
    use super::{Configuration, ProfileConfiguration};
    use std::collections::HashMap;

    /// Tests run at once, so each call needs a file of its own to read and then delete.
    fn profiles() -> Configuration {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

        let path = std::env::temp_dir().join(format!(
            "arrabbiata-profiles-{}.toml",
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::write(
            &path,
            "
            [tachi]
            api_key = 'house-key'

            [upscore]
            api_key = 'house-upscore-key'

            [profiles.'flatmate']
            refids = ['0000001346040870', '0000009999999999']
            tachi_api_key = 'flatmate-tachi-key'
            upscore_api_key = 'flatmate-upscore-key'

            [profiles.'guest']
            refids = ['0000005555555555']
            tachi_api_key = ''
            upscore_api_key = ''

            [profiles.'tachi-only']
            refids = ['0000007777777777']
            upscore_api_key = ''
            ",
        )
        .unwrap();

        let config = confy::load_path(&path).expect("the profile example should parse");
        let _ = std::fs::remove_file(&path);

        config
    }

    /// A profile's key wins over the global one, and an empty key keeps a player out.
    #[test]
    fn a_refid_picks_the_key_it_belongs_to() {
        let config = profiles();

        let key = |refid| config.tachi_api_key_for(refid);

        assert_eq!(key("0000001346040870"), Some("flatmate-tachi-key"));
        assert_eq!(key("0000009999999999"), Some("flatmate-tachi-key"));
        // Any refid no profile claims falls back to the key under [tachi].
        assert_eq!(key("0000001111111111"), Some("house-key"));
        // A blank key in a profile is an opt-out, even with a global key set.
        assert_eq!(key("0000005555555555"), None);
        // A profile that leaves the key out borrows the one under [tachi].
        assert_eq!(key("0000007777777777"), Some("house-key"));
    }

    /// The Upscore key follows the same rules, so one install can serve two accounts.
    #[test]
    fn a_refid_picks_the_upscore_key_it_belongs_to() {
        let config = profiles();
        let key = |refid| config.upscore_api_key_for(refid);

        assert_eq!(key("0000001346040870"), Some("flatmate-upscore-key"));
        assert_eq!(key("0000001111111111"), Some("house-upscore-key"));
        assert_eq!(key("0000005555555555"), None);
        // This player is submitted to Tachi on the house key, but sends nothing to Upscore.
        assert_eq!(key("0000007777777777"), None);
    }

    /// Both are reported at startup, so neither may read as set when only profiles hold one.
    #[test]
    fn credentials_count_wherever_they_are_set() {
        let config = profiles();
        assert!(config.has_tachi_api_key());
        assert!(config.has_upscore_api_key());

        assert!(!Configuration::default().has_tachi_api_key());
        assert!(!Configuration::default().has_upscore_api_key());

        let only_in_a_profile = Configuration {
            profiles: HashMap::from([(
                "them".to_string(),
                ProfileConfiguration {
                    refids: vec!["0000001346040870".to_string()],
                    upscore_api_key: Some("their-upscore-key".to_string()),
                    ..ProfileConfiguration::default()
                },
            )]),
            ..Configuration::default()
        };

        assert!(!only_in_a_profile.has_tachi_api_key());
        assert!(only_in_a_profile.has_upscore_api_key());
    }

    /// The default config ships inside the DLL, so a typo in it only surfaces at startup.
    #[test]
    fn the_shipped_default_parses() {
        let path = std::env::temp_dir().join("arrabbiata-shipped-default.toml");
        std::fs::write(&path, include_bytes!("../arrabbiata.toml")).unwrap();

        let config: Configuration = confy::load_path(&path).expect("shipped default should parse");
        let _ = std::fs::remove_file(&path);

        assert!(config.general.enable);
        assert_eq!(config.tachi.base_url, "https://kamai.tachi.ac/");
        // Out of the box there is no key, so nothing is submitted.
        assert_eq!(config.tachi_api_key_for("0000001346040870"), None);
        // Unset, so Tachi matches charts across versions.
        assert_eq!(config.tachi.version, None);
        // The blank key in the shipped file must read as absent, not as an empty token.
        assert_eq!(config.upscore.api_key(), None);
        // Each service waits on its own clock, and Upscore needs by far the longer one.
        assert_eq!(config.tachi.timeout, 3000);
        assert_eq!(config.upscore.timeout, 30000);
    }
}
