use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Result;
use log::{error, info, warn};

use crate::CONFIGURATION;
use crate::sys::{
    NodeType, property_clear_error, property_mem_write, property_node_refer, property_query_size,
    property_search, property_set_flag,
};

/// AVS picks a property's serialization from flags held on the property itself. Setting
/// 0x800 while clearing 0x008 switches the output from the native XML form to JSON, which
/// is what makes these dumps readable. Both flags are always restored afterwards, so the
/// game sees the property exactly as it left it.
const FLAG_JSON: u32 = 0x800;
const FLAG_XML: u32 = 0x008;

/// The two root nodes an e-amusement exchange can have: requests are wrapped in `call`,
/// responses in `response`. Anything AVS destroys without one of these is internal state,
/// not traffic, and is skipped.
const ROOTS: &[&str] = &["call", "response"];

/// Service names probed only so dump files get a readable name. Discovery does not depend
/// on this list being right or complete: when nothing matches, the dump is still written
/// and the real service name is plainly visible in its contents.
const KNOWN_SERVICES: &[&str] = &[
    "playerdata",
    "usergamedata",
    "game",
    "cardmng",
    "player",
    "local",
    "local2",
    "info",
    "system",
    "eacoin",
    "facility",
    "package",
    "message",
    "pcbtracker",
    "pcbevent",
    "dlstatus",
    "lobby",
    "matching",
    "userdata",
    "traceroute",
];

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub fn init() -> Result<()> {
    crochet::enable!(property_destroy_hook).map_err(|err| {
        anyhow::anyhow!(
            "Could not hook property_destroy in avs2-core.dll: {err:#}. \
             If this is a load-order problem, move arrabbiata.dll further down chainload.txt."
        )
    })?;

    info!(
        "Dumping e-amusement properties to '{}'",
        CONFIGURATION.dump.directory.display()
    );

    Ok(())
}

pub fn release() -> Result<()> {
    if crochet::is_enabled!(property_destroy_hook) {
        crochet::disable!(property_destroy_hook)
            .map_err(|err| anyhow::anyhow!("Could not unhook property_destroy: {err:#}"))?;
    }

    Ok(())
}

#[crochet::hook("avs2-core.dll", "XCgsqzn0000091")]
pub unsafe fn property_destroy_hook(property: *mut ()) -> i32 {
    if property.is_null() {
        return 0;
    }

    // `call_original!` brings its own `unsafe`, so the unsafe calls are wrapped one by one
    // rather than putting the whole body in a block the macro would then nest inside.
    let Some(root) = (unsafe { find_root(property) }) else {
        return call_original!(property);
    };

    if let Err(err) = unsafe { dump(property, root) } {
        error!("Could not dump a '{root}' property: {err:#}");
    }

    call_original!(property)
}

/// Returns the name of this property's e-amusement root node, or `None` if it has none.
///
/// A failed `property_search` leaves an error recorded on the property, so every miss is
/// cleared before moving on -- otherwise the game would later see a property AVS considers
/// to be in an error state.
unsafe fn find_root(property: *mut ()) -> Option<&'static str> {
    unsafe {
        for &root in ROOTS {
            let path = format!("/{root}\0");
            if !property_search(property, std::ptr::null(), path.as_ptr()).is_null() {
                return Some(root);
            }
            property_clear_error(property);
        }
    }

    None
}

/// Best-effort service and method names, used only to label the dump file.
unsafe fn identify(property: *mut (), root: &str) -> (Option<String>, Option<String>) {
    unsafe {
        for &service in KNOWN_SERVICES {
            let path = format!("/{root}/{service}\0");
            let node = property_search(property, std::ptr::null(), path.as_ptr());
            if node.is_null() {
                property_clear_error(property);
                continue;
            }

            return (
                Some(service.to_string()),
                read_attribute(property, node, b"method@\0"),
            );
        }
    }

    (None, None)
}

unsafe fn read_attribute(property: *mut (), node: *const (), attribute: &[u8]) -> Option<String> {
    let mut buffer = [0u8; 256];
    let result = unsafe {
        property_node_refer(
            property,
            node,
            attribute.as_ptr(),
            NodeType::NodeAttr,
            buffer.as_mut_ptr() as *mut (),
            buffer.len() as u32,
        )
    };
    if result < 0 {
        unsafe { property_clear_error(property) };
        return None;
    }

    let end = buffer
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(buffer.len());
    let value = String::from_utf8_lossy(&buffer[..end]).to_string();

    (!value.is_empty()).then_some(value)
}

/// Serializes the property, either as JSON or in its native form.
///
/// Returns `None` rather than an error when AVS refuses: a property we cannot read is not
/// a reason to disturb a running game.
unsafe fn serialize(property: *mut (), json: bool) -> Option<Vec<u8>> {
    unsafe {
        if json {
            property_set_flag(property, FLAG_JSON, FLAG_XML);
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
            property_set_flag(property, FLAG_XML, FLAG_JSON);
        }

        result
    }
}

unsafe fn dump(property: *mut (), root: &str) -> Result<()> {
    // The JSON form is what makes a dump readable, but the 0x800 flag was only ever
    // confirmed against SOUND VOLTEX's AVS build. If this one ignores it, the native form
    // is still a perfectly good dump, so it is always produced as a fallback rather than
    // losing the property entirely.
    let json = unsafe { serialize(property, true) };
    let xml = if CONFIGURATION.dump.write_xml || json.is_none() {
        unsafe { serialize(property, false) }
    } else {
        None
    };

    let Some(readable) = json.as_ref().or(xml.as_ref()) else {
        return Ok(());
    };

    // The filter matches against the serialized form, so a user can narrow to a service or
    // method by name without knowing how the payload is structured.
    let filter = &CONFIGURATION.dump.filter;
    if !filter.is_empty() {
        let haystack = String::from_utf8_lossy(readable);
        if !filter.iter().any(|needle| haystack.contains(needle.as_str())) {
            return Ok(());
        }
    }

    let bytes = readable.len();
    let (service, method) = unsafe { identify(property, root) };

    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let label = [Some(root.to_string()), service.clone(), method.clone()]
        .into_iter()
        .flatten()
        .map(sanitize)
        .collect::<Vec<_>>()
        .join("-");
    let stem = format!(
        "{sequence:05}_{}_{label}",
        chrono::Local::now().format("%H%M%S%.3f")
    );

    let directory = &CONFIGURATION.dump.directory;
    std::fs::create_dir_all(directory)
        .map_err(|err| anyhow::anyhow!("Could not create '{}': {err}", directory.display()))?;
    let mut files = Vec::new();
    for (contents, extension) in [(&json, "json"), (&xml, "xml")] {
        if let Some(contents) = contents {
            let name = format!("{stem}.{extension}");
            std::fs::write(directory.join(&name), contents)?;
            files.push(name);
        }
    }

    info!(
        "Dumped {} ({bytes} bytes) -> {}",
        match (&service, &method) {
            (Some(service), Some(method)) => format!("{root}/{service}.{method}"),
            (Some(service), None) => format!("{root}/{service}"),
            _ => format!("{root} (unrecognized service)"),
        },
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
