use std::cell::Cell;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Result;
use log::{debug, error, info, warn};

use crate::handlers::scores;
use crate::sys::{property_mem_write, property_query_size, property_set_flag};
use crate::types::game::{Envelope, Note};
use crate::types::tachi::{Import, ImportMeta};
use crate::{CONFIGURATION, TACHI_IMPORT_URL, helpers, upscore};

/// Setting 0x800 and clearing 0x008 switches AVS's output to JSON; `serialize` restores both.
const FLAG_JSON: u32 = 0x800;
const FLAG_KBIN: u32 = 0x008;

/// Above this nothing is read: a save is tens of kilobytes, the music database near a megabyte.
const MAX_SIZE: usize = 1024 * 1024;

/// How long to follow a queued import, which Tachi usually runs in well under a second.
const POLL_ATTEMPTS: u32 = 20;
const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(500);

/// A stage is saved twice, so this only has to outlive one session's worth of stages.
const RECENT_PLAYS: usize = 32;

/// A session has a player or two; this only bounds a stream of refids nobody expected.
const KNOWN_PLAYERS: usize = 8;

static SEEN: AtomicU64 = AtomicU64::new(0);
static SUBMITTED: AtomicU64 = AtomicU64::new(0);
static REFUSED: AtomicU64 = AtomicU64::new(0);

/// Identifies a play by chart and end time, so the game-over repeat is caught.
type PlayId = (u32, i32, i64);

static RECENT: Mutex<Vec<PlayId>> = Mutex::new(Vec::new());

/// Refids already named in the log, so each player is announced once a session.
static ANNOUNCED: Mutex<Vec<String>> = Mutex::new(Vec::new());

thread_local! {
    /// Serializing a property re-enters the hook; without this guard the first read recurses.
    static CAPTURING: Cell<bool> = const { Cell::new(false) };
}

pub fn init() -> Result<()> {
    crochet::enable!(property_mem_write_hook).map_err(|err| {
        anyhow::anyhow!(
            "Could not hook property_mem_write in avs2-core.dll: {err:#}. \
             If avs2-core.dll was not loaded yet, inject arrabbiata.dll later."
        )
    })?;

    if CONFIGURATION.has_api_key() {
        info!("Sending scores to Tachi at {}", TACHI_IMPORT_URL.as_str());
    } else {
        warn!("No Tachi API key is set, so no score goes there. Edit arrabbiata.toml");
    }

    if CONFIGURATION.has_upscore_code() {
        info!("Sending scores to Upscore at {}", CONFIGURATION.upscore.url);
    } else {
        warn!("No Upscore code is set, so no score goes there. Edit arrabbiata.toml");
    }

    Ok(())
}

pub fn release() -> Result<()> {
    info!(
        "Seen {} properties; Tachi took {}, Upscore took {}, refused {}",
        SEEN.load(Ordering::Relaxed),
        SUBMITTED.load(Ordering::Relaxed),
        upscore::sent(),
        REFUSED.load(Ordering::Relaxed)
    );

    if crochet::is_enabled!(property_mem_write_hook) {
        crochet::disable!(property_mem_write_hook)
            .map_err(|err| anyhow::anyhow!("Could not unhook property_mem_write: {err:#}"))?;
    }

    Ok(())
}

/// A `usersave` passes through here exactly once, which makes it the place to submit from.
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

    // Only player data names the refid that [profiles] match, so only it may announce one.
    let is_player_data = root_name(&text).as_deref() == Some("eacnet")
        && json_field(&text, "method").as_deref() == Some("usergamedata_advanced");

    if is_player_data {
        announce(&text);
    }

    if is_player_data && json_field(&text, "mode").as_deref() == Some("usersave") {
        handle_usersave(&json);
    }

    CAPTURING.with(|flag| flag.set(false));
}

/// Names a refid once a session. The game loads player data four ways, each under its own.
fn announce(text: &str) {
    let Some(refid) = json_field(text, "refid").filter(|refid| !refid.is_empty()) else {
        return;
    };

    debug!(
        "Player data for refid {refid}, mode '{}'",
        json_field(text, "mode").unwrap_or_default()
    );

    let mut announced = ANNOUNCED.lock().unwrap_or_else(|err| err.into_inner());
    if announced.contains(&refid) {
        return;
    }

    if announced.len() >= KNOWN_PLAYERS {
        announced.remove(0);
    }
    announced.push(refid.clone());

    // Only a save names the refid that picks the credentials, so this passes no judgement.
    info!("Refid {refid} signed in");
}

/// Parses a save and submits what it carries; failures are logged and dropped, never raised.
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

    // Upscore runs whether or not a Tachi key is configured: the two outputs are independent.
    let api_key = CONFIGURATION.api_key_for(&save.refid);
    let upscore_code = CONFIGURATION.upscore_code_for(&save.refid);

    // A save nothing covers must still say so: silence reads as a hook that is not working.
    if api_key.is_none() && upscore_code.is_none() {
        warn!(
            "Nothing covers refid {}, so this save goes nowhere. Put it in a [profiles] entry",
            save.refid
        );
    }

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

        // One description for both, so the same play reads the same whoever reports on it.
        let summary = scores::describe(note);

        if let Some(code) = upscore_code {
            upscore::send(note, &summary, code);
        }

        if let Some(api_key) = api_key {
            submit(note, api_key, &summary);
        }
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

fn submit(note: &Note, api_key: &str, summary: &str) {
    let converted = match scores::convert(note) {
        Ok(converted) => converted,
        Err(refusal) => {
            REFUSED.fetch_add(1, Ordering::Relaxed);
            error!("Refusing to send {summary} to Tachi: {refusal}");
            return;
        }
    };

    // Flare takes no part in a score's identity, so an unplaceable rank is dropped, not refused.
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

    let api_key = api_key.to_string();
    let summary = summary.to_string();

    // A blocking POST would show up as a stall on the save screen.
    std::thread::spawn(move || {
        let response = match helpers::post(&TACHI_IMPORT_URL, &api_key, &import) {
            Ok(response) => response,
            Err(err) => {
                error!("Could not reach Tachi for {summary}: {err:#}");
                return;
            }
        };

        if response.get("success").and_then(|v| v.as_bool()) == Some(false) {
            error!(
                "Tachi refused {summary}: {}",
                response
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("no reason given")
            );
            return;
        }

        match finished_import(&response) {
            Ok(document) => report(&summary, &document),
            Err(err) => warn!("Tachi queued {summary}, but never said what it did: {err:#}"),
        }
    });
}

/// Waits for a queued import to run, since accepting one says nothing about importing it.
fn finished_import(response: &serde_json::Value) -> Result<serde_json::Value> {
    let Some(url) = response.pointer("/body/url").and_then(|v| v.as_str()) else {
        // An import Tachi ran on the spot answers with the document itself.
        return Ok(response.get("body").cloned().unwrap_or_default());
    };

    for _ in 0..POLL_ATTEMPTS {
        let polled = helpers::get(url)?;

        if polled.get("success").and_then(|v| v.as_bool()) == Some(false) {
            anyhow::bail!(
                "{}",
                polled
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("no reason given")
            );
        }

        if polled.pointer("/body/importStatus").and_then(|v| v.as_str()) == Some("completed") {
            return polled
                .pointer("/body/import")
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("a completed import carried no document"));
        }

        std::thread::sleep(POLL_INTERVAL);
    }

    anyhow::bail!("still queued after {POLL_ATTEMPTS} polls")
}

/// Logs what Tachi made of the score. No error and no score means it already had it.
fn report(summary: &str, document: &serde_json::Value) {
    if let Some(message) = document.pointer("/errors/0/message").and_then(|v| v.as_str()) {
        error!("Tachi could not import {summary}: {message}");
        return;
    }

    let kept = document
        .get("scoreIDs")
        .and_then(|v| v.as_array())
        .map_or(0, Vec::len);

    if kept == 0 {
        info!("Tachi already had {summary}");
    } else {
        SUBMITTED.fetch_add(1, Ordering::Relaxed);
        info!("Tachi took {summary}");
    }
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

/// Reads `"key" : "value"` out of AVS's JSON, for routing only; the payload goes through serde.
fn json_field(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let after = &text[text.find(&needle)? + needle.len()..];

    // Only a string value on this key's own line counts.
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

    /// The refid is what names a player in the log and picks their key out of [profiles].
    #[test]
    fn reads_the_refid_a_request_carries() {
        assert_eq!(json_field(SAVE, "refid").as_deref(), Some("ABCD0123"));
        // A request without one, such as a shop lookup, must name nobody.
        assert_eq!(json_field(r#"{"eacnet" : {"refid" : ""}}"#, "refid"), None);
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
