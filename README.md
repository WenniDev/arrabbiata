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
already how Dynasty (`cluedo.dll`) gets in. Nothing needs to be injected by hand.

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

Three requests matter, as mapped out from the Dynasty server's DDR implementation and its
request schemas:

| Request | Carries |
| :-- | :-- |
| `log_2.save` | Per-stage play metadata: `mcode`, `notetype`, `playstyle`, `playside`, `stagenum`. No score. |
| `playerdata_2.usergamedata_send` | Profile records `COMMON`, `OPTION`, `LAST`, `RIVAL` |
| `playerdata_2.usergamedata_advanced`, mode `usersave` | **The scores.** Structure undocumented. |

`usergamedata_send` records are Base64-encoded UTF-8 CSV inside `<d>` elements. Each row starts
with a 64-bit hex bitmask, then a type name, then the fields: set bits in the mask give the
column indices the fields land in, so a row is a sparse update of a 64-column record rather than
a fixed layout. That is the profile, not the scores.

### The score payload

Nothing documents this one -- the Dynasty handler is a stub that acknowledges the request and
discards it, and its schema leaves the body as `xs:any` -- so it was established from captured
traffic. It turns out to be a plain named-field node tree, not CSV:

```
eacnet/request/data/data/
  mode = "usersave", refid, ddrcode, name, playside, playstyle, isgameover, ...
  note[5]              <- a fixed five-slot array; only slot 0 is ever filled
    stagenum  mcode  notetype  level  rank  clearkind
    score  exscore  maxcombo  life  fastcount  slowcount
    judge_marvelous  judge_perfect  judge_great  judge_good  judge_boo  judge_miss
    judge_ok  judge_ng
    calorie  ghost  ghostsize  opt_*  basename  title_b64  artist_b64
    bpmMax  bpmMin  series  genreFlag  gr_voltage  gr_stream  gr_chaos  gr_freeze  gr_air
    playing_flare  endtime  folder
```

Three things about it shape how phase 2 has to work:

- **One stage per request.** `note` has five slots but only the first carries data, so a save
  covers the stage just played rather than the session so far.
- **The last stage is sent twice.** A save fires after each stage with `isgameover` false, and
  once more at game over with `isgameover` true, repeating the final stage verbatim. Submitting
  on every save would double-count it.
- **`level` is a free checksum.** It restates the chart's difficulty rating, so a parse that
  disagrees with Tachi's chart level for that `mcode` and `notetype` is wrong and should be
  refused rather than submitted.

### Field mappings, verified against captured play

`mcode` is exactly Tachi's `inGameID` for `ddr`, confirmed independently by `basename`:

| Played | Game sent | Tachi seed |
| :-- | :-- | :-- |
| AFRONOVA, BEGINNER, 5 | `mcode` 124, `basename` "afro", `notetype` 0, `level` 5 | `inGameID` 124, "afro", BEGINNER is 5 |
| Arrabbiata, DIFFICULT, 13 | `mcode` 37270, `basename` "arra", `notetype` 2, `level` 13 | `inGameID` 37270, "arra", DIFFICULT is 13 |
| Abyss, EXPERT, 10 | `mcode` 257, `basename` "abys", `notetype` 3, `level` 10 | `inGameID` 257, "abys", EXPERT is 10 |
| Tohoku EVOLVED, CHALLENGE, 18 | `mcode` 37789, `basename` "toho1", `notetype` 4, `level` 18 | `inGameID` 37789, "toho1", CHALLENGE is 18 |

So `matchType` is `inGameID`, and `notetype` follows Tachi's own difficulty order: 0 BEGINNER,
1 BASIC, 2 DIFFICULT, 3 EXPERT, 4 CHALLENGE -- all confirmed by play except BASIC, which follows
from the ordering. `playstyle` 0 is SINGLE. Songs as recent as Arrabbiata are already in Tachi's
seeds, so GRAND PRIX's library being ahead of them has not been a problem so far.

Judgements map one to one, and EX score checks out against them: Marvelous and O.K. are worth 3,
Perfect 2, Great 1. Both captures satisfy it, which is a useful sanity check on a parse.

Misses land in `judge_miss`. `judge_boo` and `judge_ng` stayed zero even on a play with eleven
misses and a failed-out play with seventeen, so they map to nothing on Tachi's side.

`clearkind` is a ladder, observed at four points:

| Value | Lamp | Seen on |
| :-- | :-- | :-- |
| 1 | `FAILED` | score 0, 17 misses, bailed out |
| 3 | `CLEAR` | 877,490 with 11 misses |
| 7 | `FULL COMBO` | 1 good, no misses |
| 8 | `GREAT FULL COMBO` | no goods, 5 greats, no misses |

2, 4, 5, 6 and anything above 8 remain unobserved, so an unrecognized value is refused rather
than guessed into a lamp. Note that `clearkind` is *not* Tachi's lamp index offset by a constant
-- that was the obvious guess from the two full-combo values, and the failed play disproved it.

`rank` is the grade as an index into Tachi's own grade list, descending: 0 is AAA, 1 AA+, 4 A+,
15 E. Confirmed on four plays. It is not needed -- Tachi derives grade from score -- but it makes
a free cross-check on a parse.

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
- [x] Establish the score payload layout from real play
- [x] Confirm chart matching: `mcode` is Tachi's `inGameID`
- [ ] Observe the rest of the `clearkind` ladder: fails, plain clears, LIFE4, Perfect and
      Marvelous full combos
- [ ] Phase 2 — parse and submit to Tachi as `ddr:SP` / `ddr:DP`, refusing anything that does not
      validate rather than submitting a guess

There is no `cardmng` on Konasute. The player is identified in the payload itself, by `refid` and
`ddrcode`, so profiles are keyed off those rather than off an E000 card number as upstream does.

Tachi has no `grandprix` version id for `ddr` — its versions are `a`, `a20`, `a20plus`, `a3`,
`konaste` and `world`. `konaste` is the natural fit for a Konasute title, but which one GRAND PRIX
scores should be filed under is still to be settled.

## License

MIT, as upstream. See [LICENSE](LICENSE).
