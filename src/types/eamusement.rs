// The fields are the game's own score record; the camelCase names come from the upload API.
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub total_items: usize,
    pub items: Vec<MusicResult>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MusicResult {
    pub mcode: u32,
    pub basename: String,
    pub note_type: i32,
    pub title: String,
    pub artist: String,
    pub clear_kind: i32,
    pub score: i64,
    pub ex_score: i64,
    pub max_combo: i64,
    pub fast_count: i64,
    pub slow_count: i64,
    pub judge_marvelous: i64,
    pub judge_perfect: i64,
    pub judge_great: i64,
    pub judge_good: i64,
    pub judge_boo: i64,
    pub judge_miss: i64,
    pub judge_ok: i64,
    pub judge_ng: i64,
    pub timestamp: i64,
    pub flare: i32,
}
