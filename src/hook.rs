//! The AVS property hook, and what to do with what it sees.

use std::cell::Cell;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Result;
use log::{debug, error, info, warn};

use crate::handlers::scores;
use crate::sys::{property_mem_write, property_query_size, property_set_flag};
use crate::types::game::{Envelope, Note};
use crate::types::tachi::{Import, ImportMeta};
use crate::{CONFIGURATION, TACHI_IMPORT_URL, helpers};

/// AVS picks a property's serialization from flags held on the property itself: setting
/// 0x800 and clearing 0x008 switches the native kbin output to JSON. `serialize` restores
/// both afterwards, so the game sees the property exactly as it left it.
const FLAG_JSON: u32 = 0x800;
const FLAG_KBIN: u32 = 0x008;

/// Properties above this are not read. A save is a few tens of kilobytes; the music database
/// that also passes through here is nearly a megabyte and is of no interest.
const MAX_SIZE: usize = 1024 * 1024;

/// How many recently submitted plays to remember. A stage is saved twice -- once when it
/// ends and again at game over -- so this only has to outlive one session's worth of stages.
const RECENT_PLAYS: usize = 32;

static SEEN: AtomicU64 = AtomicU64::new(0);
static SUBMITTED: AtomicU64 = AtomicU64::new(0);
static REFUSED: AtomicU64 = AtomicU64::new(0);

/// Identifies a play by chart and end time. Both saves of one stage share all three, so the
/// repeat is caught without trusting `isgameover`.
type PlayId = (u32, i32, i64);

static RECENT: Mutex<Vec<PlayId>> = Mutex::new(Vec::new());

thread_local! {
    /// Serializing a property calls back into the hooked function; without this guard the
    /// first read recurses until the stack runs out.
    static CAPTURING: Cell<bool> = const { Cell::new(false) };
}

pub fn init() -> Result<()> {
    crochet::enable!(property_mem_write_hook).map_err(|err| {
        anyhow::anyhow!(
            "Could not hook property_mem_write in avs2-core.dll: {err:#}. \
             If avs2-core.dll was not loaded yet, inject arrabbiata.dll later."
        )
    })?;

    let no_key = CONFIGURATION.tachi.api_key.as_deref().unwrap_or("").is_empty()
        && CONFIGURATION.profiles.is_empty();
    if no_key {
        warn!("No Tachi API key is set, so nothing will be submitted. Edit arrabbiata.toml");
    } else {
        info!("Submitting scores to {}", TACHI_IMPORT_URL.as_str());
    }

    Ok(())
}

pub fn release() -> Result<()> {
    info!(
        "Seen {} properties; submitted {} scores, refused {}",
        SEEN.load(Ordering::Relaxed),
        SUBMITTED.load(Ordering::Relaxed),
        REFUSED.load(Ordering::Relaxed)
    );

    if crochet::is_enabled!(property_mem_write_hook) {
        crochet::disable!(property_mem_write_hook)
            .map_err(|err| anyhow::anyhow!("Could not unhook property_mem_write: {err:#}"))?;
    }

    Ok(())
}

/// The serialization path a request takes on its way out. A `usersave` passes through here
/// exactly once, which is what makes it the place to submit from.
#[crochet::hook("avs2-core.dll", "XCgsqzn00000b8")]
pub unsafe fn property_mem_write_hook(property: *mut (), data: *mut u8, size: u32) -> i32 {
    if !property.is_null() {
        SEEN.fetch_add(1, Ordering::Relaxed);
        unsafe { capture(property) };
    }

    call_original!(property, data, size)
}

unsafe fn capture(property: *mut ()) {
    let already_capturing = CAPTURING.with(|flag| flag.replace(true));
    if already_capturing {
        return;
    }

    let Some(json) = (unsafe { serialize(property) }) else {
        CAPTURING.with(|flag| flag.set(false));
        return;
    };
    let text = String::from_utf8_lossy(&json);

    let is_usersave = root_name(&text).as_deref() == Some("eacnet")
        && json_field(&text, "method").as_deref() == Some("usergamedata_advanced")
        && json_field(&text, "mode").as_deref() == Some("usersave");

    if is_usersave {
        handle_usersave(&json);
    }

    CAPTURING.with(|flag| flag.set(false));
}

/// Parses a save and submits whatever stage it carries.
///
/// Returns nothing: a save that cannot be made sense of must not disturb a running game, so
/// failures are logged and dropped.
fn handle_usersave(json: &[u8]) {
    let envelope = match serde_json::from_slice::<Envelope>(json) {
        Ok(envelope) => envelope,
        Err(err) => {
            error!("A usersave did not parse: {err}");
            return;
        }
    };

    let info = &envelope.eacnet.info;
    if !info.game_id.is_empty() && info.game_id != "ddr" {
        debug!("Ignoring a usersave from '{}'", info.game_id);
        return;
    }

    // Routing here was a text scan; confirm against the parsed payload before acting.
    let request = &envelope.eacnet.request;
    if request.method != "usergamedata_advanced" {
        debug!(
            "Ignoring {}.{} despite it looking like a save",
            request.module, request.method
        );
        return;
    }

    let save = &envelope.eacnet.request.data.data;
    if save.mode != "usersave" {
        debug!("Ignoring usergamedata_advanced mode '{}'", save.mode);
        return;
    }

    let Some(api_key) = CONFIGURATION.api_key_for(&save.refid) else {
        warn!(
            "No API key covers refid {}, so its scores are not submitted",
            save.refid
        );
        return;
    };

    debug!(
        "Save from {} (ddrcode {}, refid {}) on {}, at game over: {}",
        save.name, save.ddrcode, save.refid, info.soft_version, save.isgameover
    );
    for note in save.note.iter() {
        if note.is_empty() {
            continue;
        }
        if !claim(note) {
            debug!(
                "Already handled mcode {} notetype {} at {}, skipping the repeat",
                note.mcode, note.notetype, note.endtime
            );
            continue;
        }

        submit(note, api_key);
    }
}

/// Records a play as handled, returning false if it already was.
fn claim(note: &Note) -> bool {
    let id: PlayId = (note.mcode, note.notetype, note.endtime);

    let mut recent = RECENT.lock().unwrap_or_else(|err| err.into_inner());
    if recent.contains(&id) {
        return false;
    }

    if recent.len() >= RECENT_PLAYS {
        recent.remove(0);
    }
    recent.push(id);

    true
}

fn submit(note: &Note, api_key: &str) {
    let converted = match scores::convert(note) {
        Ok(converted) => converted,
        Err(refusal) => {
            REFUSED.fetch_add(1, Ordering::Relaxed);
            error!(
                "Refusing to submit mcode {} ({}): {refusal}",
                note.mcode, note.basename
            );
            return;
        }
    };

    // Flare takes no part in a score's identity, so an unplaceable rank is dropped rather
    // than refusing the score.
    if note.playing_flare != 0 && scores::flare(note).is_none() {
        warn!(
            "mcode {} reports flare {}, which is outside the ranks Tachi knows, so the score \
             goes without it. Please report this value.",
            note.mcode, note.playing_flare
        );
    }

    let import = Import {
        meta: ImportMeta {
            game: "ddr",
            playtype: converted.playtype,
            service: "arrabbiata".to_string(),
            version: CONFIGURATION.tachi.version.clone(),
        },
        scores: vec![converted.score],
    };

    let summary = format!(
        "{} {} on {} {} {} {} (mcode {})",
        import.scores[0].lamp,
        import.scores[0].score,
        note.basename,
        import.meta.playtype,
        import.scores[0].difficulty,
        note.level,
        note.mcode
    );
    let api_key = api_key.to_string();

    // A blocking POST would show up as a stall on the save screen.
    std::thread::spawn(move || match helpers::post(&TACHI_IMPORT_URL, &api_key, &import) {
        Ok(response) => {
            if response.get("success").and_then(|v| v.as_bool()) == Some(false) {
                error!(
                    "Tachi rejected {summary}: {}",
                    response
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("no reason given")
                );
            } else {
                SUBMITTED.fetch_add(1, Ordering::Relaxed);
                info!("Submitted {summary}");
            }
        }
        Err(err) => error!("Could not submit {summary}: {err:#}"),
    });
}

/// Serializes the property as JSON.
unsafe fn serialize(property: *mut ()) -> Option<Vec<u8>> {
    unsafe {
        property_set_flag(property, FLAG_JSON, FLAG_KBIN);

        let size = property_query_size(property);
        let result = if size <= 0 || size as usize > MAX_SIZE {
            None
        } else {
            let mut buffer = vec![0u8; size as usize];
            let written = property_mem_write(property, buffer.as_mut_ptr(), buffer.len() as u32);

            if written < 0 {
                None
            } else {
                // The queried size is an upper bound; AVS reports what it actually wrote.
                let written = written as usize;
                if written > 0 && written <= buffer.len() {
                    buffer.truncate(written);
                }
                while buffer.last() == Some(&0) {
                    buffer.pop();
                }

                Some(buffer)
            }
        };

        property_set_flag(property, FLAG_KBIN, FLAG_JSON);

        result
    }
}

/// The property's outermost node name.
fn root_name(text: &str) -> Option<String> {
    let start = text.find('"')? + 1;
    let end = text[start..].find('"')? + start;
    let name = &text[start..end];

    (!name.is_empty() && name.len() < 64).then(|| name.to_string())
}

/// Reads `"key" : "value"` out of AVS's JSON output.
///
/// For routing only; the payload that matters is parsed with serde.
fn json_field(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let after = &text[text.find(&needle)? + needle.len()..];

    // Only a string value on this key's own line counts; anything else means the key held an
    // object, a number or nothing.
    let colon = after.find(':')?;
    let value = &after[colon + 1..];
    let line_end = value.find('\n').unwrap_or(value.len());
    let start = value.find('"')? + 1;
    if start > line_end {
        return None;
    }
    let end = value[start..].find('"')? + start;

    (end > start).then(|| value[start..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::{json_field, root_name};

    /// Shaped like AVS's own JSON output, down to the spaces around the colons.
    const SAVE: &str = r#"{
  "eacnet" : {
    "request" : {
      "service" : "local",
      "module" : "playerdata_2",
      "method" : "usergamedata_advanced",
      "data" : {
        "client_key" : "",
        "data" : {
          "mode" : "usersave",
          "refid" : "ABCD0123",
          "datanum" : 4
        }
      }
    }
  }
}"#;

    #[test]
    fn recognizes_a_usersave_from_its_envelope() {
        assert_eq!(root_name(SAVE).as_deref(), Some("eacnet"));
        assert_eq!(
            json_field(SAVE, "method").as_deref(),
            Some("usergamedata_advanced")
        );
        assert_eq!(json_field(SAVE, "mode").as_deref(), Some("usersave"));
    }

    #[test]
    fn ignores_keys_that_do_not_hold_a_string() {
        assert_eq!(json_field(SAVE, "datanum"), None);
        assert_eq!(json_field(SAVE, "request"), None);
        assert_eq!(json_field(SAVE, "client_key"), None);
    }

    #[test]
    fn does_not_match_a_longer_key_that_starts_the_same() {
        assert_eq!(json_field(r#"{"service_id" : "x"}"#, "service"), None);
    }

    #[test]
    fn missing_keys_and_malformed_input_do_not_panic() {
        assert_eq!(json_field(SAVE, "nonexistent"), None);
        assert_eq!(json_field("", "service"), None);
        assert_eq!(root_name(""), None);
        assert_eq!(root_name("{"), None);
    }
}
