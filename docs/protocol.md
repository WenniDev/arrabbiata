# The GRAND PRIX protocol

Reference for how DanceDanceRevolution GRAND PRIX reports a score, and how Arrabbiata maps it
onto Tachi. Worked out from captured play; nothing else documents it.

## What it refuses

A stage is submitted only when everything about it adds up. Otherwise it is refused and the
reason logged, naming the value that could not be placed — a wrong score submitted silently is
worse than a missing one.

The lamp comes from `clearkind` alone, through the table below. A value not in it is refused.

Also refused: a difficulty or playstyle out of range, the two disagreeing about singles or
doubles, a score outside what Tachi accepts, and a full combo whose `maxcombo` does not account
for its judgements.

Flare is sent when there is one. `playing_flare` indexes Tachi's own ladder — 1 is Flare I, 10 is
Flare EX — and 0 is left out, since Tachi already defaults to it. A rank outside that range warns
and the score goes without it: Flare is optional and takes no part in a score's identity, so an
unplaceable rank should not cost the whole score. **If you see that warning, please report the
value.**

Floating Flare needs no special handling. It walks down from EX until a rank passes, and the
payload reports the rank that did.

### Version

Tachi has no `grandprix` version for `ddr`, only `a`, `a20`, `a20plus`, `a3`, `konaste` and
`world`. `tachi.version` is left unset, so `meta.version` is omitted from the import and Tachi
resolves charts across versions rather than filing scores under one this fork picked. Set it if
you know which you want. Whether it should be set at all is the one open question left.

### Players

There is no `cardmng` on Konasute — zero occurrences across 466 captured properties covering
login and profile load. A player is identified by the `refid` their own save carries, which is
what `[profiles]` keys off, rather than by an E000 card number as upstream does.

## The envelope

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
a fixed layout. That is the profile, not the scores.

### The score payload

Nothing documents this one -- the server handler is a stub that acknowledges the request and
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

So `matchType` is `inGameID`.

`notetype` names the chart: **both** how it is played and how hard it is. It runs straight
through singles into doubles rather than restarting, and doubles has no BEGINNER, so the ladder
is nine values:

| 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 |
| :-- | :-- | :-- | :-- | :-- | :-- | :-- | :-- | :-- |
| SP BEGINNER | SP BASIC | SP DIFFICULT | SP EXPERT | SP CHALLENGE | DP BASIC | DP DIFFICULT | DP EXPERT | DP CHALLENGE |

Reading it as a difficulty on its own happens to work for singles and shifts everything by one
for doubles. A doubles play settled it: notetype 5 arrived with `level` 3, and DP BASIC is level 3
in Tachi's seeds where SP BASIC is 2. `playstyle` says the same thing a second time — 0 singles,
1 doubles — and is required to agree.

Songs as recent as Arrabbiata are already in Tachi's seeds, so GRAND PRIX's library being ahead of
them has not been a problem so far.

Judgements map one to one, and EX score checks out against them: Marvelous and O.K. are worth 3,
Perfect 2, Great 1. Every capture satisfies it, which is a useful sanity check on a parse.

O.K. does not count towards a combo, though it does count towards EX score. A captured Great Full
Combo with 21 of them reported a `maxcombo` of 417 against 303 Marvelous, 96 Perfect and 18 Great
-- exactly the other judgements, with the O.K. judgements excluded.

Misses land in `judge_miss`. `judge_boo` and `judge_ng` stayed zero even on a play with eleven
misses and a failed-out play with seventeen, so they map to nothing on Tachi's side.

`clearkind` is a ladder, observed at four points:

| Value | Lamp | Seen on |
| :-- | :-- | :-- |
| 1 | `FAILED` | a bail-out at score 0, and a full play that died at 814,710 |
| 2 | `ASSIST` | `opt_cut`, `opt_freeze` and `opt_jump` all 1, on a normal gauge |
| 3 | `CLEAR` | 877,490 with 11 misses |
| 6 | `LIFE4` | 909,980 with 2 misses, `life` 4 and `opt_gauge` 2 |
| 7 | `FULL COMBO` | 1 good, no misses |
| 8 | `GREAT FULL COMBO` | no goods, 5 greats, no misses |

| 9 | `PERFECT FULL COMBO` | continues the run above; unobserved |
| 10 | `MARVELOUS FULL COMBO` | continues the run above; unobserved |

4 and 5 remain unobserved and are refused. `clearkind` is *not* Tachi's lamp index offset by a
constant — LIFE4 is fourth in Tachi's order and lands on 6 — so the table stays a table.

A LIFE4 play also reports `life` as 4 where every other capture has -1, and an assisted one sets
`opt_cut`, `opt_freeze` and `opt_jump` where every other capture leaves them at 0. None of these
is used: one capture cannot say whether `life` counts the gauge's lives or the ones left at the
end, nor which assists are enough to make a play assisted.

Tachi has no lamp for a RISKY clear, and the game reports one under the same `clearkind` values
as any other gauge.

`rank` is the grade as an index into Tachi's own grade list, descending: 0 is AAA, 1 AA+, 4 A+,
15 E. It is not needed, since Tachi derives grade from score, and it would make a poor
cross-check against a score-derived grade: a failed play sends 15 regardless, one capture pairing
it with a score of 814,710 that would otherwise grade A. Tachi caps a failed score's grade the
same way, so there is nothing to do about it -- but a parse must not treat the disagreement as an
error.

## Why this was captured rather than read

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
  installed directly from `DllMain`, which relies on being injected once AVS is loaded.
- **Game identity** is `VGP:J:A:A:<ext>` rather than SOUND VOLTEX's `KFC`. Builds from
  `2026012800` through `2026061700` have been observed, with `spec` seen as `A`, `B` and `C`.
- **Cloudlink PB injection is gone.** It is a SOUND VOLTEX feature with no GRAND PRIX equivalent,
  so `src/cloudlink/` and the `property_mem_read` hook were dropped.
- **A bad config no longer kills the game.** Upstream calls `std::process::exit(1)`; a tool that
  only observes traffic reports the problem and falls back to defaults.

