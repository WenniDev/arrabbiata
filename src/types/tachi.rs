// Names and values follow Tachi's game config, not its docs, which order the lamps differently.
use serde::Serialize;

/// Tachi's lamp enum, in its own order: `LIFE4` sits between `CLEAR` and `FULL COMBO`.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lamp {
    Failed,
    Assist,
    Clear,
    Life4,
    FullCombo,
    GreatFullCombo,
    PerfectFullCombo,
    MarvelousFullCombo,
}

impl Lamp {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Failed => "FAILED",
            Self::Assist => "ASSIST",
            Self::Clear => "CLEAR",
            Self::Life4 => "LIFE4",
            Self::FullCombo => "FULL COMBO",
            Self::GreatFullCombo => "GREAT FULL COMBO",
            Self::PerfectFullCombo => "PERFECT FULL COMBO",
            Self::MarvelousFullCombo => "MARVELOUS FULL COMBO",
        }
    }

    /// Whether this lamp means the player combo'd the whole chart.
    pub fn is_full_combo(self) -> bool {
        matches!(
            self,
            Self::FullCombo
                | Self::GreatFullCombo
                | Self::PerfectFullCombo
                | Self::MarvelousFullCombo
        )
    }
}

impl std::fmt::Display for Lamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for Lamp {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// Tachi's Flare ranks, which `playing_flare` indexes directly: Flare II sends 2.
pub const FLARES: [&str; 11] = [
    "0", "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "EX",
];

/// Tachi's fixed difficulty order for `ddr`.
pub const DIFFICULTIES: [&str; 5] = [
    "BEGINNER",
    "BASIC",
    "DIFFICULT",
    "EXPERT",
    "CHALLENGE",
];

#[derive(Debug, Clone, Serialize)]
pub struct Import {
    pub meta: ImportMeta,
    pub scores: Vec<ImportScore>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportMeta {
    pub game: &'static str,
    pub playtype: &'static str,
    pub service: String,
    /// Left out so Tachi searches every version; its seeds do not tag every chart as `konaste`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportScore {
    #[serde(rename = "matchType")]
    pub match_type: &'static str,
    pub identifier: String,
    pub difficulty: &'static str,
    pub lamp: Lamp,
    pub score: i64,
    #[serde(rename = "timeAchieved")]
    pub time_achieved: i64,
    pub judgements: Judgements,
    #[serde(skip_serializing_if = "Optional::is_empty")]
    pub optional: Optional,
}

#[derive(Debug, Clone, Serialize)]
pub struct Judgements {
    #[serde(rename = "MARVELOUS")]
    pub marvelous: i64,
    #[serde(rename = "PERFECT")]
    pub perfect: i64,
    #[serde(rename = "GREAT")]
    pub great: i64,
    #[serde(rename = "GOOD")]
    pub good: i64,
    #[serde(rename = "OK")]
    pub ok: i64,
    #[serde(rename = "MISS")]
    pub miss: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Optional {
    /// Left out for a play without one: Tachi already defaults to Flare 0.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flare: Option<&'static str>,
    /// Zero is rejected by Tachi and left out; it is `partOfScoreID`, so gaps make duplicates.
    #[serde(rename = "exScore", skip_serializing_if = "Option::is_none")]
    pub ex_score: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fast: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slow: Option<i64>,
    #[serde(rename = "maxCombo", skip_serializing_if = "Option::is_none")]
    pub max_combo: Option<i64>,
}

impl Optional {
    pub fn is_empty(&self) -> bool {
        self.flare.is_none()
            && self.ex_score.is_none()
            && self.fast.is_none()
            && self.slow.is_none()
            && self.max_combo.is_none()
    }
}
