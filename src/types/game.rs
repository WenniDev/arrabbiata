//! The shape of a GRAND PRIX `usersave` request, as captured from real play.
//!
//! Only the fields this fork reads are declared; AVS sends a great deal more (play options,
//! ghost data, groove radar, song metadata) that nothing here needs. Every field is
//! `#[serde(default)]` so that a future game version adding or dropping one cannot turn a
//! whole payload into a parse error.

use serde::Deserialize;

/// A node that AVS may serialize either as a single object or as an array, depending on how
/// many children it has. `note` has five slots in every capture so far, but nothing
/// guarantees that stays true.
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

/// Identifies the game and build, carried in the request itself. This is what the hook gates
/// on: Konasute has no `avs2-ea3.dll` to read a boot node from, but every request says who
/// it came from.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Info {
    #[serde(default)]
    pub game_id: String,
    #[serde(default)]
    pub soft_version: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    // `service` is also present, and is "local" on every request captured so far. Nothing
    // needs it, so it is not modelled.
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
    /// Identifies the player. Konasute has no `cardmng`, so profiles key off this rather
    /// than off an E000 card number.
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

/// One stage's result. The array has five slots but only the first is ever filled, so the
/// rest arrive zeroed and are skipped as empty.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Note {
    #[serde(default)]
    pub stagenum: i32,
    /// Tachi's `inGameID` for `ddr`, verified against its seeds.
    #[serde(default)]
    pub mcode: u32,
    /// Indexes Tachi's difficulty order: 0 BEGINNER through 4 CHALLENGE.
    #[serde(default)]
    pub notetype: i32,
    /// The chart's difficulty rating. Restates what Tachi's seeds hold, so it is a free
    /// check that a parse landed on the right chart.
    #[serde(default)]
    pub level: i32,
    // `rank` is also present: the grade, indexing Tachi's grade list descending from 0 AAA
    // to 15 E. Not modelled, because Tachi derives grade from score and a failed play sends
    // 15 regardless of what it scored, which makes it a poor cross-check. See the README.
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
    /// Stayed zero across every capture, including plays with 11 and 17 misses. Counted as
    /// combo-breaking anyway, since a non-zero value would mean this assumption was wrong.
    #[serde(default)]
    pub judge_boo: i64,
    #[serde(default)]
    pub judge_miss: i64,
    #[serde(default)]
    pub judge_ok: i64,
    /// Failed freeze arrows. Also always zero so far, and also treated as combo-breaking.
    #[serde(default)]
    pub judge_ng: i64,
    /// 0 is SINGLE, 1 is DOUBLE.
    #[serde(default)]
    pub playstyle: i32,
    #[serde(default)]
    pub playing_flare: i32,
    /// Milliseconds since the epoch. Two saves of the same stage share it, which is what
    /// makes it usable for deduplication.
    #[serde(default)]
    pub endtime: i64,
    #[serde(default)]
    pub basename: String,
}

impl Note {
    /// An unused slot in the five-element array: zeroed rather than absent.
    pub fn is_empty(&self) -> bool {
        self.stagenum <= 0 || self.mcode == 0
    }

    /// Judgements that break a combo. `judge_boo` and `judge_ng` have never been seen
    /// non-zero, so including them costs nothing and fails safe if they ever are.
    pub fn combo_breaks(&self) -> i64 {
        self.judge_miss + self.judge_boo + self.judge_ng
    }

    /// Judgements that make up a combo. `judge_ok` is not among them: a captured full combo
    /// with 21 O.K. judgements reported a `maxcombo` matching the others exactly.
    pub fn combo_notes(&self) -> i64 {
        self.judge_marvelous + self.judge_perfect + self.judge_great + self.judge_good
    }
}
