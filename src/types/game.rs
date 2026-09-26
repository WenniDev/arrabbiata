//! The shape of a GRAND PRIX `usersave` request.
//!
//! Only the fields the hook reads are declared; AVS sends a great deal more (play options,
//! ghost data, groove radar, song metadata). Every field is `#[serde(default)]` so a game
//! version that adds or drops one cannot turn a whole payload into a parse error.

use serde::Deserialize;

/// A node AVS serializes either as a single object or as an array, depending on how many
/// children it has.
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
    /// Names the chart across both playstyles: 0-4 singles BEGINNER to CHALLENGE, 5-8
    /// doubles BASIC to CHALLENGE.
    #[serde(default)]
    pub notetype: i32,
    /// The chart's difficulty rating.
    #[serde(default)]
    pub level: i32,
    // `rank` is also present -- the grade, 0 AAA down to 15 E -- and is not modelled: Tachi
    // derives grade from score, and a failed play sends 15 whatever it scored.
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
    // `judge_boo` and `judge_ng` are also present, and always zero. Not modelled.
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
