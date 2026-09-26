//! The AVS property hooks, and what to do with what they see.

use std::cell::Cell;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Result;
use log::{debug, error, info, warn};

use crate::handlers::scores;
use crate::sys::{property_mem_write, property_query_size, property_set_flag};
use crate::types::game::{Envelope, Note};
use crate::types::tachi::{Import, ImportMeta};
use crate::{CONFIGURATION, TACHI_IMPORT_URL, dump, helpers};

/// AVS picks a property's serialization from flags held on the property itself. Setting
/// 0x800 while clearing 0x008 switches the output from the native kbin form to JSON. Both
/// are always restored afterwards, so the game sees the property exactly as it left it.
const FLAG_JSON: u32 = 0x800;
const FLAG_KBIN: u32 = 0x008;

/// How many recently submitted plays to remember. A stage is saved twice -- once when it
/// ends and again at game over -- so this only has to outlive one session's worth of stages.
const RECENT_PLAYS: usize = 32;

static SEEN_DESTROY: AtomicU64 = AtomicU64::new(0);
static SEEN_WRITE: AtomicU64 = AtomicU64::new(0);
static SUBMITTED: AtomicU64 = AtomicU64::new(0);
static DRY_RUN: AtomicU64 = AtomicU64::new(0);
static REFUSED: AtomicU64 = AtomicU64::new(0);

/// Identifies a play by chart and end time. The two saves of one stage share all three, so
/// this catches the repeat without relying on `isgameover`, whose meaning has only been
/// observed and not documented.
type PlayId = (u32, i32, i64);

static RECENT: Mutex<Vec<PlayId>> = Mutex::new(Vec::new());

thread_local! {
    /// Reading a property means serializing it, which calls back into the very functions
    /// being hooked. Without this the first capture would recurse until the stack ran out.
    static CAPTURING: Cell<bool> = const { Cell::new(false) };
}

pub fn init() -> Result<()> {
    let config = &CONFIGURATION.dump;

    if config.on_destroy {
        crochet::enable!(property_destroy_hook).map_err(|err| {
            anyhow::anyhow!(
                "Could not hook property_destroy in avs2-core.dll: {err:#}. \
                 If this is a load-order problem, move arrabbiata.dll further down chainload.txt."
            )
        })?;
    }

    if config.on_write {
        crochet::enable!(property_mem_write_hook).map_err(|err| {
            anyhow::anyhow!("Could not hook property_mem_write in avs2-core.dll: {err:#}")
        })?;
    }

    if !config.on_destroy && !config.on_write {
        warn!("Both dump.on_destroy and dump.on_write are off, so nothing will be seen");
        return Ok(());
    }

    let no_key = CONFIGURATION.tachi.api_key.as_deref().unwrap_or("").is_empty()
        && CONFIGURATION.profiles.is_empty();

    if !CONFIGURATION.general.submit {
        info!(
            "Dry run: general.submit is off, so each score is worked out and printed in full \
             but nothing is sent"
        );
    } else if no_key {
        info!(
            "Dry run: no Tachi API key is set, so each score is worked out and printed in full \
             but nothing is sent. Set tachi.api_key in arrabbiata.toml to submit."
        );
    } else {
        info!("Submitting scores to {}", TACHI_IMPORT_URL.as_str());
    }

    if config.all {
        info!(
            "dump.all is on: every property is written to '{}'",
            config.directory.display()
        );
    }

    Ok(())
}

pub fn release() -> Result<()> {
    info!(
        "Seen {} properties via property_destroy and {} via property_mem_write; \
         submitted {} scores, {} dry runs, refused {}, wrote {} dumps",
        SEEN_DESTROY.load(Ordering::Relaxed),
        SEEN_WRITE.load(Ordering::Relaxed),
        SUBMITTED.load(Ordering::Relaxed),
        DRY_RUN.load(Ordering::Relaxed),
        REFUSED.load(Ordering::Relaxed),
        dump::written()
    );

    if crochet::is_enabled!(property_destroy_hook) {
        crochet::disable!(property_destroy_hook)
            .map_err(|err| anyhow::anyhow!("Could not unhook property_destroy: {err:#}"))?;
    }
    if crochet::is_enabled!(property_mem_write_hook) {
        crochet::disable!(property_mem_write_hook)
            .map_err(|err| anyhow::anyhow!("Could not unhook property_mem_write: {err:#}"))?;
    }

    Ok(())
}

#[crochet::hook("avs2-core.dll", "XCgsqzn0000091")]
pub unsafe fn property_destroy_hook(property: *mut ()) -> i32 {
    if property.is_null() {
        return 0;
    }

    SEEN_DESTROY.fetch_add(1, Ordering::Relaxed);
    unsafe { capture(property, "destroy") };

    call_original!(property)
}

/// The serialization path a request takes on its way out. A `usersave` passes through here
/// exactly once, where `property_destroy` sees it twice, which is what makes this the place
/// to submit from.
#[crochet::hook("avs2-core.dll", "XCgsqzn00000b8")]
pub unsafe fn property_mem_write_hook(property: *mut (), data: *mut u8, size: u32) -> i32 {
    if !property.is_null() {
        SEEN_WRITE.fetch_add(1, Ordering::Relaxed);
        unsafe { capture(property, "write") };
    }

    call_original!(property, data, size)
}

unsafe fn capture(property: *mut (), source: &str) {
    let already_capturing = CAPTURING.with(|flag| flag.replace(true));
    if already_capturing {
        return;
    }

    if let Err(err) = unsafe { process(property, source) } {
        error!("Could not process a property seen via {source}: {err:#}");
    }

    CAPTURING.with(|flag| flag.set(false));
}

unsafe fn process(property: *mut (), source: &str) -> Result<()> {
    let config = &CONFIGURATION.dump;

    let Some(json) = (unsafe { serialize(property, true) }) else {
        return Ok(());
    };
    let text = String::from_utf8_lossy(&json);

    let root = root_name(&text);
    let method = json_field(&text, "method");
    let mode = json_field(&text, "mode");

    // A save is submitted from the write path only. It reaches property_destroy twice, and
    // submitting from both would depend on deduplication rather than merely being checked
    // by it.
    let is_usersave = root.as_deref() == Some("eacnet")
        && method.as_deref() == Some("usergamedata_advanced")
        && mode.as_deref() == Some("usersave");

    // Handled regardless of whether submission is on: with it off, the payload is still
    // parsed, converted and reported, which is the only way to tell that the whole chain
    // works before pointing it at a real account.
    if is_usersave && source == "write" {
        handle_usersave(&json);
    }

    if config.all {
        let kbin = config
            .write_kbin
            .then(|| unsafe { serialize(property, false) })
            .flatten();
        dump_payload(source, &root, &method, &mode, &text, &json, kbin.as_deref())?;
    }

    Ok(())
}

fn dump_payload(
    source: &str,
    root: &Option<String>,
    method: &Option<String>,
    mode: &Option<String>,
    text: &str,
    json: &[u8],
    kbin: Option<&[u8]>,
) -> Result<()> {
    let config = &CONFIGURATION.dump;

    if !config.roots.is_empty() {
        let keep = root
            .as_ref()
            .is_some_and(|root| config.roots.iter().any(|wanted| wanted == root));
        if !keep {
            return Ok(());
        }
    }

    if !config.filter.is_empty()
        && !config
            .filter
            .iter()
            .any(|needle| text.contains(needle.as_str()))
    {
        return Ok(());
    }

    let label = [
        Some(source.to_string()),
        root.clone(),
        json_field(text, "service"),
        method.clone(),
        mode.clone(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join("-");

    let files = dump::write(&label, Some(json), kbin)?;
    debug!("Dumped {label} ({} bytes) -> {}", json.len(), files.join(", "));

    Ok(())
}

/// Parses a save and submits whatever stage it carries.
///
/// Nothing here returns an error to the caller: a save this fork cannot make sense of must
/// not disturb a running game, so every failure is logged and, where it might be a score,
/// written out for later.
fn handle_usersave(json: &[u8]) {
    let envelope = match serde_json::from_slice::<Envelope>(json) {
        Ok(envelope) => envelope,
        Err(err) => {
            error!("A usersave did not parse: {err}");
            refuse(json, "unparseable");
            return;
        }
    };

    // Every request says which game it came from, which is what this hook gates on --
    // Konasute has no avs2-ea3.dll to read a boot node from.
    let info = &envelope.eacnet.info;
    if !info.game_id.is_empty() && info.game_id != "ddr" {
        debug!("Ignoring a usersave from '{}'", info.game_id);
        return;
    }

    // Routing used a cheap text scan to get here. Now that the payload is properly parsed,
    // confirm it really is what that scan claimed before acting on it.
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

    let api_key = CONFIGURATION.api_key_for(&save.refid);
    if api_key.is_none() {
        warn!(
            "No API key covers refid {}, so this is a dry run: the score is worked out and \
             reported but not sent anywhere",
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

        handle_note(note, api_key, json);
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

/// Works one stage out and either sends it or reports what would have been sent.
///
/// `api_key` is `None` when nothing covers this player, which together with
/// `general.submit` being off is what makes a dry run. A dry run still parses, converts,
/// validates and refuses exactly as a real run does -- only the POST is skipped -- so it is
/// a genuine rehearsal rather than a different code path.
fn handle_note(note: &Note, api_key: Option<&str>, json: &[u8]) {
    let converted = match scores::convert(note) {
        Ok(converted) => converted,
        Err(refusal) => {
            REFUSED.fetch_add(1, Ordering::Relaxed);
            error!(
                "Refusing to submit mcode {} ({}): {refusal}",
                note.mcode, note.basename
            );
            refuse(json, "refused");
            return;
        }
    };

    let import = Import {
        meta: ImportMeta {
            game: "ddr",
            playtype: converted.playtype,
            service: "arrabbiata".to_string(),
            version: CONFIGURATION.tachi.version.clone(),
        },
        scores: vec![converted.score],
    };

    // Flare takes no part in a score's identity on Tachi's side, so a rank outside the
    // eleven it knows is dropped rather than made to refuse the score. Say so loudly: a
    // report is all it would take to place it.
    if note.playing_flare != 0 && scores::flare(note).is_none() {
        warn!(
            "mcode {} reports flare {}, which is outside the ranks Tachi knows, so the score \
             goes without it. Please report this value.",
            note.mcode, note.playing_flare
        );
    }

    let summary = format!(
        "{} {} on {} {} {} (mcode {}, rank {})",
        import.scores[0].lamp,
        import.scores[0].score,
        note.basename,
        import.scores[0].difficulty,
        note.level,
        note.mcode,
        note.rank
    );
    let Some(api_key) = api_key.filter(|_| CONFIGURATION.general.submit) else {
        DRY_RUN.fetch_add(1, Ordering::Relaxed);
        info!(
            "Would submit {summary}:\n{}",
            serde_json::to_string_pretty(&import)
                .unwrap_or_else(|err| format!("(could not render the import: {err})"))
        );
        return;
    };
    let api_key = api_key.to_string();

    // The game is already waiting on its own network call here; adding a blocking POST to
    // that wait would show up as a stall on the save screen.
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

fn refuse(json: &[u8], reason: &str) {
    if !CONFIGURATION.dump.on_refusal {
        return;
    }

    match dump::write(&format!("usersave-{reason}"), Some(json), None) {
        Ok(files) => warn!("Wrote the payload to {} for diagnosis", files.join(", ")),
        Err(err) => error!("Could not write the refused payload: {err:#}"),
    }
}

/// Serializes the property, either as JSON or in its native kbin form.
unsafe fn serialize(property: *mut (), json: bool) -> Option<Vec<u8>> {
    unsafe {
        if json {
            property_set_flag(property, FLAG_JSON, FLAG_KBIN);
        }

        let size = property_query_size(property);
        let max_size = CONFIGURATION.dump.max_size;

        let result = if size <= 0 {
            None
        } else if size as usize > max_size {
            warn!("Skipping a {size} byte property, over the dump.max_size limit of {max_size}");
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

        if json {
            property_set_flag(property, FLAG_KBIN, FLAG_JSON);
        }

        result
    }
}

/// The name of the property's outermost node, read off the JSON rather than by walking
/// nodes, so a root this fork has never heard of is still named correctly.
fn root_name(text: &str) -> Option<String> {
    let start = text.find('"')? + 1;
    let end = text[start..].find('"')? + start;
    let name = &text[start..end];

    (!name.is_empty() && name.len() < 64).then(|| name.to_string())
}

/// Reads `"key" : "value"` out of AVS's JSON output.
///
/// Used only for routing and labelling, where a cheap scan beats deserializing a payload
/// that is usually of no interest. The payload that matters is parsed properly with serde.
fn json_field(text: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let after = &text[text.find(&needle)? + needle.len()..];

    // Only accept a string value belonging to this key: a quote must come after the colon
    // and before the line ends, otherwise this key held an object, a number or nothing.
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
        assert_eq!(json_field(SAVE, "service").as_deref(), Some("local"));
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
