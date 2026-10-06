// Every field is #[serde(default)] so a game update adding or dropping one cannot fail the parse.
use serde::Deserialize;

/// A node AVS serializes as a single object or an array, depending on child count.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum OneOrMany<T> {
    One(Box<T>),
    Many(Vec<T>),
}

impl<T> Default for OneOrMany<T> {
    fn default() -> Self {
        Self::Many(Vec::new())
    }
}

impl<T> OneOrMany<T> {
    pub fn iter(&self) -> Box<dyn Iterator<Item = &T> + '_> {
        match self {
            Self::One(value) => Box::new(std::iter::once(value.as_ref())),
            Self::Many(values) => Box::new(values.iter()),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Envelope {
    pub eacnet: Eacnet,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Eacnet {
    pub request: Request,
    #[serde(default)]
    pub info: Info,
}

/// Identifies the game and build. The hook gates on `game_id`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Info {
    #[serde(default)]
    pub game_id: String,
    #[serde(default)]
    pub soft_version: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    // `service` is also present ("local"), and is not modelled.
    #[serde(default)]
    pub module: String,
    #[serde(default)]
    pub method: String,
    pub data: RequestData,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RequestData {
    pub data: UserSave,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UserSave {
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub refid: String,
    #[serde(default)]
    pub ddrcode: i64,
    #[serde(default)]
    pub name: String,
    /// True on the extra save fired at game over, which repeats the final stage verbatim.
    #[serde(default)]
    pub isgameover: bool,
    #[serde(default)]
    pub note: OneOrMany<Note>,
}

/// One stage's result.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Note {
    #[serde(default)]
    pub stagenum: i32,
    /// Tachi's `inGameID` for `ddr`.
    #[serde(default)]
    pub mcode: u32,
    /// 0-4 singles BEGINNER to CHALLENGE, 5-8 doubles BASIC to CHALLENGE.
    #[serde(default)]
    pub notetype: i32,
    /// The chart's difficulty rating.
    #[serde(default)]
    pub level: i32,
    // `rank` is also present, the grade 0 AAA to 15 E; not modelled, a fail sends 15 regardless.
    #[serde(default)]
    pub clearkind: i32,
    #[serde(default)]
    pub score: i64,
    #[serde(default)]
    pub exscore: i64,
    #[serde(default)]
    pub maxcombo: i64,
    #[serde(default)]
    pub fastcount: i64,
    #[serde(default)]
    pub slowcount: i64,
    #[serde(default)]
    pub judge_marvelous: i64,
    #[serde(default)]
    pub judge_perfect: i64,
    #[serde(default)]
    pub judge_great: i64,
    #[serde(default)]
    pub judge_good: i64,
    #[serde(default)]
    pub judge_miss: i64,
    #[serde(default)]
    pub judge_ok: i64,
    /// Always zero in practice, but the Upscore payload carries it.
    #[serde(default)]
    pub judge_boo: i64,
    /// Failed freeze arrows. Also always zero, and also in the Upscore payload.
    #[serde(default)]
    pub judge_ng: i64,
    /// 0 is SINGLE, 1 is DOUBLE.
    #[serde(default)]
    pub playstyle: i32,
    #[serde(default)]
    pub playing_flare: i32,
    /// Milliseconds since the epoch. Both saves of one stage share it, so it deduplicates.
    #[serde(default)]
    pub endtime: i64,
    #[serde(default)]
    pub basename: String,
    /// The song title, base64-encoded UTF-8.
    #[serde(default)]
    pub title_b64: String,
    /// The artist, base64-encoded UTF-8.
    #[serde(default)]
    pub artist_b64: String,
}

impl Note {
    /// An unused note slot: zeroed rather than absent.
    pub fn is_empty(&self) -> bool {
        self.stagenum <= 0 || self.mcode == 0
    }

    /// Judgements that make up a combo. `judge_ok` is not one of them.
    pub fn combo_notes(&self) -> i64 {
        self.judge_marvelous + self.judge_perfect + self.judge_great + self.judge_good
    }
}
