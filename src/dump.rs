use std::cell::Cell;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Result;
use log::{error, info, warn};

use crate::CONFIGURATION;
use crate::sys::{property_mem_write, property_query_size, property_set_flag};

/// AVS picks a property's serialization from flags held on the property itself. Setting
/// 0x800 while clearing 0x008 switches the output from the native kbin form to JSON, which
/// is what makes these dumps readable. Both flags are always restored afterwards, so the
/// game sees the property exactly as it left it.
const FLAG_JSON: u32 = 0x800;
const FLAG_KBIN: u32 = 0x008;

/// Envelope fields lifted into dump filenames. GRAND PRIX wraps its traffic in an `eacnet`
/// envelope where these are plain string elements -- `eacnet/request/service` and so on --
/// rather than the `method@` attributes the arcade `call` envelope uses.
const LABEL_FIELDS: &[&str] = &["service", "method", "mode"];

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
static SEEN_DESTROY: AtomicU64 = AtomicU64::new(0);
static SEEN_WRITE: AtomicU64 = AtomicU64::new(0);
static SKIPPED_DUPLICATE: AtomicU64 = AtomicU64::new(0);

/// Hash of the last property written out, so a property AVS serializes more than once --
/// typically a sizing pass followed by the real write -- does not land on disk twice.
static LAST_HASH: Mutex<Option<u64>> = Mutex::new(None);

thread_local! {
    /// Capturing a property means serializing it, which calls back into the very functions
    /// being hooked. Without this the first dump would recurse until the stack ran out.
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
        warn!("Both dump.on_destroy and dump.on_write are off, so nothing will be captured");
        return Ok(());
    }

    info!(
        "Dumping properties to '{}' (on_destroy: {}, on_write: {})",
        config.directory.display(),
        config.on_destroy,
        config.on_write
    );
    if config.roots.is_empty() && config.filter.is_empty() {
        info!("No root or content filter set, so every property AVS handles is captured");
    }

    Ok(())
}

pub fn release() -> Result<()> {
    // These counts are the whole point of a discovery run: they separate "the hook never
    // fired" from "the hook fired constantly but nothing matched the filters".
    info!(
        "Seen {} properties via property_destroy and {} via property_mem_write; wrote {} dumps, skipped {} repeats",
        SEEN_DESTROY.load(Ordering::Relaxed),
        SEEN_WRITE.load(Ordering::Relaxed),
        SEQUENCE.load(Ordering::Relaxed),
        SKIPPED_DUPLICATE.load(Ordering::Relaxed)
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

/// The serialization path a request takes on its way out, and the reason this hook exists:
/// `property_destroy` turned out to see only a handful of properties per session, none of
/// them outgoing traffic.
#[crochet::hook("avs2-core.dll", "XCgsqzn00000b8")]
pub unsafe fn property_mem_write_hook(property: *mut (), data: *mut u8, size: u32) -> i32 {
    if !property.is_null() {
        SEEN_WRITE.fetch_add(1, Ordering::Relaxed);
        unsafe { capture(property, "write") };
    }

    call_original!(property, data, size)
}

/// Serializes and writes a property out, unless it is filtered, a repeat, or we are already
/// inside a capture on this thread.
unsafe fn capture(property: *mut (), source: &str) {
    let already_capturing = CAPTURING.with(|flag| flag.replace(true));
    if already_capturing {
        return;
    }

    if let Err(err) = unsafe { dump(property, source) } {
        error!("Could not dump a property seen via {source}: {err:#}");
    }

    CAPTURING.with(|flag| flag.set(false));
}

/// Serializes the property, either as JSON or in its native kbin form.
///
/// Returns `None` rather than an error when AVS refuses: a property we cannot read is not
/// a reason to disturb a running game.
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
/// Going through the text rather than a second pass over the property API keeps this honest
/// about what actually got dumped, and sidesteps addressing a node by absolute path.
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

fn hash(bytes: &[u8]) -> u64 {
    use std::hash::{DefaultHasher, Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

unsafe fn dump(property: *mut (), source: &str) -> Result<()> {
    let config = &CONFIGURATION.dump;

    let json = unsafe { serialize(property, true) };
    let kbin = if config.write_kbin || json.is_none() {
        unsafe { serialize(property, false) }
    } else {
        None
    };

    let Some(readable) = json.as_ref().or(kbin.as_ref()) else {
        return Ok(());
    };

    let text = String::from_utf8_lossy(readable);
    let root = root_name(&text);

    // An empty roots list keeps everything, which is what a discovery run wants. Once the
    // interesting root is known, naming it here cuts the noise without a rebuild.
    if !config.roots.is_empty() {
        let keep = root
            .as_ref()
            .is_some_and(|root| config.roots.iter().any(|wanted| wanted == root));
        if !keep {
            return Ok(());
        }
    }

    // The content filter matches the serialized form, so a user can narrow to a service or
    // method by name without knowing how the payload is structured.
    if !config.filter.is_empty() && !config.filter.iter().any(|needle| text.contains(needle.as_str()))
    {
        return Ok(());
    }

    // AVS commonly serializes the same property twice in a row, once to size the buffer and
    // once for real. Only the first of those is worth keeping.
    let digest = hash(readable);
    {
        let mut last = LAST_HASH.lock().unwrap_or_else(|err| err.into_inner());
        if *last == Some(digest) {
            SKIPPED_DUPLICATE.fetch_add(1, Ordering::Relaxed);
            return Ok(());
        }
        *last = Some(digest);
    }

    let mut parts = vec![source.to_string()];
    parts.extend(root.clone());
    parts.extend(LABEL_FIELDS.iter().filter_map(|key| json_field(&text, key)));
    let label = parts
        .into_iter()
        .map(sanitize)
        .collect::<Vec<_>>()
        .join("-");

    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let stem = format!(
        "{sequence:05}_{}_{label}",
        chrono::Local::now().format("%H%M%S%.3f")
    );

    let directory = &config.directory;
    std::fs::create_dir_all(directory)
        .map_err(|err| anyhow::anyhow!("Could not create '{}': {err}", directory.display()))?;

    let mut files = Vec::new();
    for (contents, extension) in [(&json, "json"), (&kbin, "kbin")] {
        if let Some(contents) = contents {
            let name = format!("{stem}.{extension}");
            std::fs::write(directory.join(&name), contents)?;
            files.push(name);
        }
    }

    info!(
        "Dumped {label} ({} bytes) -> {}",
        readable.len(),
        files.join(", ")
    );

    Ok(())
}

fn sanitize(value: String) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{json_field, root_name, sanitize};

    /// Shaped like AVS's own JSON output, down to the spaces around the colons.
    const SAVE: &str = r#"{
  "eacnet" : {
    "request" : {
      "service" : "playerdata_2",
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
    fn reads_the_outermost_node_name() {
        assert_eq!(root_name(SAVE).as_deref(), Some("eacnet"));
    }

    #[test]
    fn reads_envelope_fields() {
        assert_eq!(json_field(SAVE, "service").as_deref(), Some("playerdata_2"));
        assert_eq!(
            json_field(SAVE, "method").as_deref(),
            Some("usergamedata_advanced")
        );
        assert_eq!(json_field(SAVE, "mode").as_deref(), Some("usersave"));
    }

    #[test]
    fn ignores_keys_that_do_not_hold_a_string() {
        // A number, and an object whose first string belongs to a nested key.
        assert_eq!(json_field(SAVE, "datanum"), None);
        assert_eq!(json_field(SAVE, "request"), None);
    }

    #[test]
    fn does_not_match_a_longer_key_that_starts_the_same() {
        assert_eq!(json_field(r#"{"service_id" : "x"}"#, "service"), None);
    }

    #[test]
    fn treats_an_empty_string_value_as_absent() {
        assert_eq!(json_field(SAVE, "client_key"), None);
    }

    #[test]
    fn missing_keys_and_malformed_input_do_not_panic() {
        assert_eq!(json_field(SAVE, "nonexistent"), None);
        assert_eq!(json_field("", "service"), None);
        assert_eq!(root_name(""), None);
        assert_eq!(root_name("{"), None);
    }

    #[test]
    fn filenames_keep_only_safe_characters() {
        assert_eq!(sanitize("playerdata_2".into()), "playerdata_2");
        assert_eq!(sanitize("a/b\\c:d".into()), "a_b_c_d");
    }
}
