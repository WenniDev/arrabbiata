use std::sync::atomic::{AtomicU64, Ordering};

use base64::Engine;
use log::{error, info, warn};

use crate::types::eamusement::{MusicResult, Page};
use crate::types::game::Note;
use crate::{CONFIGURATION, helpers};

static SENT: AtomicU64 = AtomicU64::new(0);

pub fn sent() -> u64 {
    SENT.load(Ordering::Relaxed)
}

/// Decodes a base64 field, falling back to the raw text so an unreadable title loses nothing.
fn decode(value: &str) -> String {
    base64::engine::general_purpose::STANDARD
        .decode(value)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .unwrap_or_else(|| value.to_string())
}

fn result_from(note: &Note) -> MusicResult {
    MusicResult {
        mcode: note.mcode,
        basename: note.basename.clone(),
        note_type: note.notetype,
        title: decode(&note.title_b64),
        artist: decode(&note.artist_b64),
        clear_kind: note.clearkind,
        score: note.score,
        ex_score: note.exscore,
        max_combo: note.maxcombo,
        fast_count: note.fastcount,
        slow_count: note.slowcount,
        judge_marvelous: note.judge_marvelous,
        judge_perfect: note.judge_perfect,
        judge_great: note.judge_great,
        judge_good: note.judge_good,
        judge_boo: note.judge_boo,
        judge_miss: note.judge_miss,
        judge_ok: note.judge_ok,
        judge_ng: note.judge_ng,
        timestamp: note.endtime,
        flare: note.playing_flare,
    }
}

/// Sends one play on its own thread, so the game is not waiting on the request.
pub fn send(note: &Note, summary: &str, code: &str) {
    let page = Page {
        total_items: 1,
        items: vec![result_from(note)],
    };
    let url = CONFIGURATION.upscore.url.clone();
    let code = code.to_string();
    let summary = summary.to_string();

    std::thread::spawn(move || match helpers::post_for_outcome(&url, &code, &page) {
        Ok((200, body)) => report(&summary, &body),
        Ok((401, _)) => error!("Upscore refused the code: check upscore.code in arrabbiata.toml"),
        Ok((429, _)) => warn!("Upscore is over its hourly limit, so it did not take {summary}"),
        Ok((502, _)) => warn!("Upscore could not score {summary}, which is worth retrying"),
        Ok((status, body)) => {
            error!("Upscore answered {status} for {summary}: {}", reason(&body))
        }
        Err(err) => error!("Could not reach Upscore for {summary}: {err:#}"),
    });
}

/// What Upscore did with a play, which a 200 alone does not say.
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    Held,
    NoChart,
    Nothing,
}

/// Reads the tally Upscore answers with. `held` counts the plays it took.
fn outcome(body: &serde_json::Value) -> Outcome {
    let tally = |field: &str| body.get(field).and_then(|value| value.as_u64()).unwrap_or(0);

    if tally("held") > 0 {
        Outcome::Held
    } else if tally("unmatched") > 0 {
        Outcome::NoChart
    } else {
        Outcome::Nothing
    }
}

fn report(summary: &str, body: &serde_json::Value) {
    match outcome(body) {
        Outcome::Held => {
            SENT.fetch_add(1, Ordering::Relaxed);
            info!("Upscore took {summary}");
        }
        Outcome::NoChart => warn!("Upscore could not import {summary}: it has no such chart"),
        Outcome::Nothing => warn!("Upscore kept nothing for {summary}: {body}"),
    }
}

/// The reason Upscore names for a refusal this does not read as a case of its own.
fn reason(body: &serde_json::Value) -> &str {
    body.get("error")
        .and_then(|value| value.as_str())
        .unwrap_or("no reason given")
}

#[cfg(test)]
mod tests {
    use super::{Outcome, Page, decode, outcome, result_from};
    use crate::types::game::Note;

    /// Both bodies are Upscore's own answers: a play it took, then one whose chart it lacks.
    #[test]
    fn a_two_hundred_is_read_from_the_tally_it_carries() {
        let held = serde_json::json!({
            "received": 1, "held": 1, "unmatched": 0, "skipped": [],
            "upload_id": "d3c90a52-8814-4a88-ad79-de47d9e10294"
        });
        assert_eq!(outcome(&held), Outcome::Held);

        let unmatched = serde_json::json!({
            "received": 1, "held": 0, "unmatched": 1, "skipped": [], "upload_id": null
        });
        assert_eq!(outcome(&unmatched), Outcome::NoChart);
    }

    /// An answer in a shape this does not know must not read as a score sent.
    #[test]
    fn an_unreadable_tally_counts_nothing() {
        assert_eq!(outcome(&serde_json::Value::Null), Outcome::Nothing);
        assert_eq!(outcome(&serde_json::json!({})), Outcome::Nothing);
    }

    #[test]
    fn titles_are_decoded_from_base64() {
        assert_eq!(decode("QUZST05PVkE="), "AFRONOVA");
        assert_eq!(decode("UkUtVkVOR0U="), "RE-VENGE");
    }

    #[test]
    fn unreadable_titles_fall_back_to_the_raw_text() {
        assert_eq!(decode("not base64 at all"), "not base64 at all");
        assert_eq!(decode(""), "");
    }

    /// AFRONOVA on BEGINNER 5, as the game sent it.
    #[test]
    fn a_play_becomes_the_shape_upscore_accepts() {
        let note = Note {
            mcode: 124,
            notetype: 0,
            clearkind: 8,
            score: 981_120,
            exscore: 283,
            maxcombo: 108,
            fastcount: 7,
            slowcount: 29,
            judge_marvelous: 72,
            judge_perfect: 31,
            judge_great: 5,
            endtime: 1_790_388_044_481,
            basename: "afro".to_string(),
            title_b64: "QUZST05PVkE=".to_string(),
            artist_b64: "UkUtVkVOR0U=".to_string(),
            ..Note::default()
        };

        let json = serde_json::to_value(Page {
            total_items: 1,
            items: vec![result_from(&note)],
        })
        .unwrap();

        assert_eq!(
            json,
            serde_json::json!({
                "totalItems": 1,
                "items": [{
                    "mcode": 124,
                    "basename": "afro",
                    "noteType": 0,
                    "title": "AFRONOVA",
                    "artist": "RE-VENGE",
                    "clearKind": 8,
                    "score": 981120,
                    "exScore": 283,
                    "maxCombo": 108,
                    "fastCount": 7,
                    "slowCount": 29,
                    "judgeMarvelous": 72,
                    "judgePerfect": 31,
                    "judgeGreat": 5,
                    "judgeGood": 0,
                    "judgeBoo": 0,
                    "judgeMiss": 0,
                    "judgeOk": 0,
                    "judgeNg": 0,
                    "timestamp": 1790388044481i64,
                    "flare": 0
                }]
            })
        );
    }
}
