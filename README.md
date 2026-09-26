# Arrabbiata

A hook for **DanceDanceRevolution GRAND PRIX** (Konasute), forked from
[mikado](https://github.com/adamaq01/mikado) by adamaq01, which does the same job for SOUND VOLTEX.

> **Status: phase 1 of 2.** Right now this only *observes* e-amusement traffic and writes it to
> disk. It does not submit anything anywhere yet. Score submission to Tachi lands in phase 2,
> once the dumps have pinned down how GRAND PRIX actually encodes a score — see
> [Why a dump first](#why-a-dump-first).

## Installation

1. Build it (or grab a release), giving you `arrabbiata.dll`.
2. Drop it in `<game>\game\modules\`, next to `avs2-core.dll`.
3. Add a line to `<game>\game\modules\chainload.txt`:

   ```
   d3d9_x64.dll
   cluedo.dll
   arrabbiata.dll
   ```

4. Start the game. `arrabbiata.toml` is created next to the DLL on first run, and dumps land in
   `arrabbiata-dumps\` as paired `.json` and `.xml` files.

Loading goes through [konasute_chainload](https://github.com/Radioo/konasute_chainload), which is
already how other DLLs get in. Nothing needs to be injected by hand.

## Configuration

`arrabbiata.toml`, created on first run:

| Key | Meaning |
| :-- | :-- |
| `general.enable` | Set to `false` to install no hooks at all |
| `dump.directory` | Where dumps are written |
| `dump.write_xml` | Also write the native XML form, which preserves node types |
| `dump.filter` | Only dump properties containing one of these substrings; empty dumps everything |
| `dump.max_size` | Properties above this byte count are skipped instead of written |

Leave `filter` empty for discovery. Score payloads are large, so don't lower `max_size`.

## The protocol

GRAND PRIX wraps its traffic in an **`eacnet`** envelope, not the `call` wrapper arcade titles
use. Service and method are plain string elements rather than attributes:

```
eacnet/request/{ service, module, method, data/{ client_key, info/version, data/{ ... } } }
```

Three requests matter, as mapped out from a server's DDR implementation and its
request schemas:

| Request | Carries |
| :-- | :-- |
| `log_2.save` | Per-stage play metadata: `mcode`, `notetype`, `playstyle`, `playside`, `stagenum`. No score. |
| `playerdata_2.usergamedata_send` | Profile records `COMMON`, `OPTION`, `LAST`, `RIVAL` |
| `playerdata_2.usergamedata_advanced`, mode `usersave` | **The scores.** Structure undocumented. |

`usergamedata_send` records are Base64-encoded UTF-8 CSV inside `<d>` elements. Each row starts
with a 64-bit hex bitmask, then a type name, then the fields: set bits in the mask give the
column indices the fields land in, so a row is a sparse update of a 64-column record rather than
a fixed layout.

The score payload under `usersave` is the one piece nothing documents -- the server handler is a
stub that acknowledges the request and discards it, and its schema leaves the body as `xs:any`.
Establishing that layout is what this dump exists for.

## Why a dump first

GRAND PRIX does not encode scores the way SOUND VOLTEX does, so mikado's parsing layer could not
be adapted — it had to be replaced. Reverse engineering `ddr-konaste.exe` established the shape
but not the detail:

- `sequence::network::SavePlayerDataActor` drives saving: a 6-state machine that shows
  `NOW SAVING`, retries up to 5 times, then gives up with `Retry Failed.` It handles player 1 and
  player 2 separately, and fires two distinct requests — a `0xb878`-byte profile blob, and a
  `0x1ae0`-byte score blob that is exactly 4 records of `0x6b8`, one per stage.
- The payload is built by chaining hundreds of `append(buf, value); append(buf, U",")` calls.
  It is **CSV**, in UTF-32 wide strings, each record prefixed by a name and a schema version —
  the long-standing `usergamedata` shape. `mdx02_rs_exscore` is one such record name.

What static analysis *cannot* recover is the field order, because the executable resolves its
imports at runtime and builds the record-name strings into zero-initialized memory at startup. A
positional CSV parser written against a guessed field order does not fail loudly; it silently
submits wrong scores. So the field order gets established from real traffic first.

This DLL is not throwaway. The final hook is this code plus a parser and an HTTP client.

## What was reused unchanged

Every AVS ordinal mikado depends on is present in GRAND PRIX's `avs2-core.dll`, and each one
self-identifies through its own log strings, confirming the mapping rather than assuming it:

| Ordinal | Function |
| :-- | :-- |
| `XCgsqzn0000091` | `property_destroy` |
| `XCgsqzn00000b7` | `property_mem_read` |
| `XCgsqzn000009a` | `property_set_flag` |
| `XCgsqzn000009d` | `property_clear_error` |
| `XCgsqzn000009f` | `property_query_size` |
| `XCgsqzn00000a1` | `property_search` |
| `XCgsqzn00000a7` | `property_node_name` |
| `XCgsqzn00000af` | `property_node_refer` |
| `XCgsqzn00000b8` | `property_mem_write` |

So `src/sys.rs` and `src/log.rs` carry over as-is.

## How it differs from upstream

- **No `avs2-ea3.dll` on Konasute**, so upstream's boot hook has no target. The property hook is
  installed directly from `DllMain` instead; `chainload.txt` loads us after AVS is up.
- **Game identity** is `VGP:J:A:A:<ext>` rather than SOUND VOLTEX's `KFC`. Builds from
  `2026012800` through `2026061700` have been observed, with `spec` seen as `A`, `B` and `C`.
- **Cloudlink PB injection is gone.** It is a SOUND VOLTEX feature with no GRAND PRIX equivalent,
  so `src/cloudlink/` and the `property_mem_read` hook were dropped.
- **A bad config no longer kills the game.** Upstream calls `std::process::exit(1)`; a tool that
  only observes traffic reports the problem and falls back to defaults.

## Roadmap

- [x] Phase 1 — dump e-amusement properties to disk
- [ ] Establish the CSV record layout from real dumps
- [ ] Confirm whether `cardmng` exists on Konasute, which decides how profiles are keyed
- [ ] Phase 2 — parse and submit to Tachi as `ddr:SP` / `ddr:DP`, validating every record before
      submitting it

Tachi has no `grandprix` version id for `ddr` yet — its versions are `a`, `a20`, `a20plus`, `a3`,
`konaste` and `world` — so which one GRAND PRIX scores are filed under is still to be settled.

## License

MIT, as upstream. See [LICENSE](LICENSE).
