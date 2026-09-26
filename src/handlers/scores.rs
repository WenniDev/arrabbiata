//! Turning one stage's result into a Tachi score, or refusing to.
//!
//! The guiding rule is that a stage this fork cannot fully account for is refused and
//! dumped rather than submitted on a guess. A wrong score submitted silently is worse than
//! a missing one, and the dump turns each refusal into something that can be diagnosed.

use crate::types::game::Note;
use crate::types::tachi::{DIFFICULTIES, FLARES, ImportScore, Judgements, Lamp, Optional};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    UnknownNotetype(i32),
    UnknownPlaystyle(i32),
    /// `notetype` and `playstyle` disagree about whether this was singles or doubles.
    PlaystyleConflict {
        notetype: i32,
        from_notetype: &'static str,
        playstyle: i32,
    },
    ScoreOutOfRange(i64),
    /// No lamp can be established: the combo broke, and `clearkind` is a value this fork
    /// has never seen.
    UnknownClearKind(i32),
    /// The judgements and `clearkind` disagree about what happened.
    LampConflict {
        from_judgements: Option<Lamp>,
        from_clearkind: Lamp,
        clearkind: i32,
    },
    /// A full combo whose `maxcombo` does not account for the notes hit.
    ComboMismatch { maxcombo: i64, expected: i64 },
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownNotetype(notetype) => write!(
                f,
                "notetype {notetype} names no chart this build knows \
                 (0-4 are singles BEGINNER to CHALLENGE, 5-8 doubles BASIC to CHALLENGE)"
            ),
            Self::UnknownPlaystyle(playstyle) => {
                write!(f, "playstyle {playstyle} is neither SINGLE (0) nor DOUBLE (1)")
            }
            Self::PlaystyleConflict {
                notetype,
                from_notetype,
                playstyle,
            } => write!(
                f,
                "notetype {notetype} is a {from_notetype} chart but playstyle says {}",
                if *playstyle == 0 { "SP" } else { "DP" }
            ),
            Self::ScoreOutOfRange(score) => {
                write!(f, "score {score} is outside the 0 to 1,000,000 Tachi accepts")
            }
            Self::UnknownClearKind(clearkind) => write!(
                f,
                "the combo broke, so the lamp must come from clearkind, and {clearkind} is not a value this build knows \
                 (known: 1 FAILED, 2 ASSIST, 3 CLEAR, 6 LIFE4, 7 FULL COMBO, 8 GREAT FULL COMBO)"
            ),
            Self::LampConflict {
                from_judgements,
                from_clearkind,
                clearkind,
            } => match from_judgements {
                Some(judged) => write!(
                    f,
                    "the judgements say {judged} but clearkind {clearkind} says {from_clearkind}"
                ),
                None => write!(
                    f,
                    "the combo broke, but clearkind {clearkind} claims {from_clearkind}"
                ),
            },
            Self::ComboMismatch { maxcombo, expected } => write!(
                f,
                "a full combo with maxcombo {maxcombo}, where its judgements account for {expected}"
            ),
        }
    }
}

pub struct Converted {
    pub playtype: &'static str,
    pub score: ImportScore,
}

/// The `clearkind` values established from captured play.
///
/// This is deliberately not a formula. The two full-combo values alone suggested `clearkind`
/// was Tachi's lamp index plus three, which a later failed play disproved -- it is 1, not 3
/// -- and LIFE4 landing on 6 rather than 4 rules out any other simple offset. Anything not
/// listed here is refused.
fn lamp_from_clearkind(clearkind: i32) -> Option<Lamp> {
    match clearkind {
        1 => Some(Lamp::Failed),
        2 => Some(Lamp::Assist),
        3 => Some(Lamp::Clear),
        6 => Some(Lamp::Life4),
        7 => Some(Lamp::FullCombo),
        8 => Some(Lamp::GreatFullCombo),
        _ => None,
    }
}

/// The full-combo tier, which the judgements determine on their own: the worst judgement
/// present names the lamp. Returns `None` when the combo broke, which the judgements cannot
/// tell apart from a fail.
fn lamp_from_judgements(note: &Note) -> Option<Lamp> {
    if note.combo_breaks() != 0 {
        return None;
    }

    Some(if note.judge_good > 0 {
        Lamp::FullCombo
    } else if note.judge_great > 0 {
        Lamp::GreatFullCombo
    } else if note.judge_perfect > 0 {
        Lamp::PerfectFullCombo
    } else {
        Lamp::MarvelousFullCombo
    })
}

/// Establishes the lamp from both sources and requires them to agree.
///
/// The judgements are authoritative for full combos, including the two tiers no capture has
/// produced a `clearkind` for. `clearkind` is authoritative for everything else, because
/// judgements alone cannot separate a fail from a clear.
fn lamp(note: &Note) -> Result<Lamp, Refusal> {
    let from_clearkind = lamp_from_clearkind(note.clearkind);

    match lamp_from_judgements(note) {
        Some(judged) => match from_clearkind {
            Some(known) if known != judged => Err(Refusal::LampConflict {
                from_judgements: Some(judged),
                from_clearkind: known,
                clearkind: note.clearkind,
            }),
            // An unrecognized clearkind is fine here: the judgements already settled it.
            _ => Ok(judged),
        },
        None => match from_clearkind {
            Some(known) if known.is_full_combo() => Err(Refusal::LampConflict {
                from_judgements: None,
                from_clearkind: known,
                clearkind: note.clearkind,
            }),
            Some(known) => Ok(known),
            None => Err(Refusal::UnknownClearKind(note.clearkind)),
        },
    }
}

/// The Flare rank reached, if any.
///
/// Flare is optional on Tachi's side and takes no part in a score's identity, so a rank this
/// build cannot place is dropped rather than made to refuse an otherwise good score. The
/// caller warns when that happens.
pub fn flare(note: &Note) -> Option<&'static str> {
    // Nothing to send for a play without a Flare: Tachi already defaults to 0.
    if note.playing_flare <= 0 {
        return None;
    }

    usize::try_from(note.playing_flare)
        .ok()
        .and_then(|rank| FLARES.get(rank).copied())
}

/// The chart a `notetype` names: both how it is played and how hard it is.
///
/// `notetype` runs straight through both playstyles rather than restarting, and doubles has
/// no BEGINNER, which is why the ladder is nine values and not ten. Established from a
/// doubles play reporting notetype 5 at level 3, matching the DP BASIC chart in Tachi's
/// seeds -- reading it as a difficulty alone happened to work for singles and silently
/// shifts everything by one for doubles.
fn chart(notetype: i32) -> Option<(&'static str, &'static str)> {
    let (playtype, difficulty) = match notetype {
        0..=4 => ("SP", DIFFICULTIES[notetype as usize]),
        // Doubles picks up where singles left off, at BASIC.
        5..=8 => ("DP", DIFFICULTIES[(notetype - 4) as usize]),
        _ => return None,
    };

    Some((playtype, difficulty))
}

pub fn convert(note: &Note) -> Result<Converted, Refusal> {
    let (playtype, difficulty) =
        chart(note.notetype).ok_or(Refusal::UnknownNotetype(note.notetype))?;

    // playstyle says the same thing a second time, so require it to agree.
    let expected = match note.playstyle {
        0 => "SP",
        1 => "DP",
        other => return Err(Refusal::UnknownPlaystyle(other)),
    };
    if playtype != expected {
        return Err(Refusal::PlaystyleConflict {
            notetype: note.notetype,
            from_notetype: playtype,
            playstyle: note.playstyle,
        });
    }

    if !(0..=1_000_000).contains(&note.score) {
        return Err(Refusal::ScoreOutOfRange(note.score));
    }

    let lamp = lamp(note)?;

    // A full combo means every note was part of the combo, so maxcombo has to equal the
    // judgements that make one up. O.K. is not among them: a captured full combo with 21 of
    // them had a maxcombo matching the other judgements exactly.
    if lamp.is_full_combo() {
        let expected = note.combo_notes();
        if note.maxcombo != expected {
            return Err(Refusal::ComboMismatch {
                maxcombo: note.maxcombo,
                expected,
            });
        }
    }

    let time_achieved = if note.endtime > 0 {
        note.endtime
    } else {
        std::time::UNIX_EPOCH
            .elapsed()
            .map(|elapsed| elapsed.as_millis() as i64)
            .unwrap_or_default()
    };

    Ok(Converted {
        playtype,
        score: ImportScore {
            match_type: "inGameID",
            identifier: note.mcode.to_string(),
            difficulty,
            lamp,
            score: note.score,
            time_achieved,
            judgements: Judgements {
                marvelous: note.judge_marvelous,
                perfect: note.judge_perfect,
                great: note.judge_great,
                good: note.judge_good,
                ok: note.judge_ok,
                miss: note.judge_miss,
            },
            optional: Optional {
                flare: flare(note),
                // Tachi rejects a non-positive exScore rather than storing zero.
                ex_score: (note.exscore > 0).then_some(note.exscore),
                fast: Some(note.fastcount),
                slow: Some(note.slowcount),
                max_combo: Some(note.maxcombo),
            },
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AFRONOVA, BEGINNER 5. Great Full Combo, clearkind 8.
    fn afronova() -> Note {
        Note {
            stagenum: 1,
            mcode: 124,
            notetype: 0,
            level: 5,
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
            ..Note::default()
        }
    }

    /// Abyss, EXPERT 10. Good Full Combo, clearkind 7.
    fn abyss() -> Note {
        Note {
            stagenum: 2,
            mcode: 257,
            notetype: 3,
            level: 10,
            clearkind: 7,
            score: 966_510,
            exscore: 790,
            maxcombo: 309,
            fastcount: 46,
            slowcount: 66,
            judge_marvelous: 197,
            judge_perfect: 88,
            judge_great: 23,
            judge_good: 1,
            endtime: 1_790_388_180_000,
            ..Note::default()
        }
    }

    /// Arrabbiata, DIFFICULT 13. Cleared with 11 misses, clearkind 3.
    fn arrabbiata() -> Note {
        Note {
            stagenum: 2,
            mcode: 37_270,
            notetype: 2,
            level: 13,
            clearkind: 3,
            score: 877_490,
            exscore: 804,
            maxcombo: 260,
            fastcount: 30,
            slowcount: 178,
            judge_marvelous: 157,
            judge_perfect: 123,
            judge_great: 84,
            judge_good: 1,
            judge_miss: 11,
            judge_ok: 1,
            endtime: 1_790_389_699_000,
            ..Note::default()
        }
    }

    /// Tohoku EVOLVED, CHALLENGE 18. Failed out, clearkind 1.
    fn failed() -> Note {
        Note {
            stagenum: 1,
            mcode: 37_789,
            notetype: 4,
            level: 18,
            clearkind: 1,
            judge_miss: 17,
            endtime: 1_790_389_526_000,
            ..Note::default()
        }
    }

    #[test]
    fn converts_the_four_captured_plays() {
        let cases = [
            (afronova(), "BEGINNER", Lamp::GreatFullCombo, 981_120, Some(283)),
            (abyss(), "EXPERT", Lamp::FullCombo, 966_510, Some(790)),
            (arrabbiata(), "DIFFICULT", Lamp::Clear, 877_490, Some(804)),
            // A failed play scores zero, and a zero exScore is left out entirely.
            (failed(), "CHALLENGE", Lamp::Failed, 0, None),
        ];

        for (note, difficulty, lamp, score, ex_score) in cases {
            let converted = convert(&note).expect("should convert");
            assert_eq!(converted.playtype, "SP");
            assert_eq!(converted.score.difficulty, difficulty);
            assert_eq!(converted.score.lamp, lamp);
            assert_eq!(converted.score.score, score);
            assert_eq!(converted.score.optional.ex_score, ex_score);
            assert_eq!(converted.score.identifier, note.mcode.to_string());
            assert_eq!(converted.score.time_achieved, note.endtime);
        }
    }

    #[test]
    fn judgements_name_the_full_combo_tier() {
        let tiers = [
            // clearkind moves with the tier, or the cross-check refuses the pair -- which is
            // exactly what it is there to do.
            (Note { judge_good: 1, clearkind: 7, ..afronova() }, Lamp::FullCombo),
            (afronova(), Lamp::GreatFullCombo),
            (Note { judge_great: 0, clearkind: 9, ..afronova() }, Lamp::PerfectFullCombo),
            (
                Note { judge_great: 0, judge_perfect: 0, clearkind: 10, ..afronova() },
                Lamp::MarvelousFullCombo,
            ),
        ];

        for (note, expected) in tiers {
            // maxcombo has to keep up, or the combo check fires first.
            let note = Note { maxcombo: note.combo_notes(), ..note };
            assert_eq!(convert(&note).map(|c| c.score.lamp), Ok(expected));
        }
    }

    #[test]
    fn perfect_and_marvelous_combos_pass_without_a_known_clearkind() {
        // clearkind 9 and 10 have never been captured. The judgements settle these on their
        // own, so an unrecognized value must not refuse them.
        let note = Note { judge_great: 0, clearkind: 9, maxcombo: 103, ..afronova() };
        assert_eq!(convert(&note).map(|c| c.score.lamp), Ok(Lamp::PerfectFullCombo));
    }

    #[test]
    fn a_broken_combo_with_an_unknown_clearkind_is_refused() {
        let note = Note { clearkind: 5, ..arrabbiata() };
        assert_eq!(convert(&note).err(), Some(Refusal::UnknownClearKind(5)));
    }

    /// An assisted clear, refused until its clearkind was known. The payload said what it
    /// was: `opt_cut`, `opt_freeze` and `opt_jump` all 1 where every other capture had 0 --
    /// CUT drops notes, and turning off jumps and freezes simplifies the chart -- while
    /// `opt_gauge` was 0 and `life` -1, so it was the assists and not the gauge.
    #[test]
    fn an_assisted_clear_is_recognized() {
        let note = Note {
            stagenum: 1,
            mcode: 37_498,
            notetype: 4,
            level: 17,
            clearkind: 2,
            score: 547_630,
            exscore: 784,
            maxcombo: 179,
            judge_marvelous: 187,
            judge_perfect: 87,
            judge_great: 49,
            judge_good: 1,
            judge_miss: 1,
            endtime: 1_790_394_757_034,
            ..Note::default()
        };

        let converted = convert(&note).expect("should convert");
        assert_eq!(converted.score.lamp, Lamp::Assist);
        assert_eq!(converted.score.difficulty, "CHALLENGE");
        // (187 + 0) * 3 + 87 * 2 + 49 = 784
        assert_eq!(converted.score.optional.ex_score, Some(784));
    }

    /// A LIFE4 clear, which the hook refused before its clearkind was known. The payload
    /// corroborated it twice over: `life` was 4 where every other capture had -1, and
    /// `opt_gauge` was 2.
    #[test]
    fn a_life4_clear_is_recognized() {
        let note = Note {
            stagenum: 2,
            mcode: 38_104,
            notetype: 3,
            level: 12,
            clearkind: 6,
            score: 909_980,
            exscore: 1157,
            maxcombo: 286,
            judge_marvelous: 242,
            judge_perfect: 163,
            judge_great: 105,
            judge_good: 1,
            judge_miss: 2,
            endtime: 1_790_393_709_000,
            ..Note::default()
        };

        let converted = convert(&note).expect("should convert");
        assert_eq!(converted.score.lamp, Lamp::Life4);
        // (242 + 0) * 3 + 163 * 2 + 105 = 1157
        assert_eq!(converted.score.optional.ex_score, Some(1157));
    }

    #[test]
    fn disagreement_between_judgements_and_clearkind_is_refused() {
        // Judgements say a Great Full Combo, clearkind says a plain clear.
        let note = Note { clearkind: 3, ..afronova() };
        assert!(matches!(
            convert(&note).err(),
            Some(Refusal::LampConflict { from_judgements: Some(Lamp::GreatFullCombo), .. })
        ));

        // The combo broke, but clearkind claims a full combo.
        let note = Note { clearkind: 7, ..arrabbiata() };
        assert!(matches!(
            convert(&note).err(),
            Some(Refusal::LampConflict { from_judgements: None, .. })
        ));
    }

    #[test]
    fn a_full_combo_whose_maxcombo_does_not_add_up_is_refused() {
        let note = Note { maxcombo: 50, ..afronova() };
        assert!(matches!(convert(&note).err(), Some(Refusal::ComboMismatch { .. })));
    }

    /// Bad Maniacs, DIFFICULT 13: a Great Full Combo with 21 O.K. judgements whose maxcombo
    /// matched the other judgements exactly. This is what established that O.K. does not
    /// count towards a combo, and it also exercises O.K. being worth 3 in the EX score:
    /// (303 + 21) * 3 + 96 * 2 + 18 = 1182.
    #[test]
    fn ok_judgements_do_not_count_towards_a_combo() {
        let note = Note {
            stagenum: 1,
            mcode: 38_753,
            notetype: 2,
            level: 13,
            clearkind: 8,
            score: 982_420,
            exscore: 1182,
            maxcombo: 417,
            fastcount: 41,
            slowcount: 73,
            judge_marvelous: 303,
            judge_perfect: 96,
            judge_great: 18,
            judge_ok: 21,
            endtime: 1_790_391_772_617,
            ..Note::default()
        };

        assert_eq!(note.combo_notes(), 417, "O.K. is not part of a combo");
        let converted = convert(&note).expect("should convert");
        assert_eq!(converted.score.lamp, Lamp::GreatFullCombo);
        assert_eq!(converted.score.optional.ex_score, Some(1182));

        // Counting O.K. towards the combo would have put maxcombo at 438, which is now
        // refused rather than quietly tolerated.
        let note = Note { maxcombo: 438, ..note };
        assert_eq!(
            convert(&note).err(),
            Some(Refusal::ComboMismatch { maxcombo: 438, expected: 417 })
        );
    }

    #[test]
    fn out_of_range_inputs_are_refused() {
        assert_eq!(
            convert(&Note { notetype: 9, ..afronova() }).err(),
            Some(Refusal::UnknownNotetype(9))
        );
        assert_eq!(
            convert(&Note { playstyle: 2, ..afronova() }).err(),
            Some(Refusal::UnknownPlaystyle(2))
        );
        assert_eq!(
            convert(&Note { score: 1_000_001, ..afronova() }).err(),
            Some(Refusal::ScoreOutOfRange(1_000_001))
        );
    }

    /// 3y3s on EXPERT 17, cleared at Flare II under Floating Flare -- which walks down from
    /// EX until a rank passes, and reports the one that did.
    #[test]
    fn a_flare_rank_indexes_tachis_own_ladder() {
        let note = Note {
            stagenum: 1,
            mcode: 38_546,
            notetype: 3,
            level: 17,
            clearkind: 3,
            score: 710_110,
            exscore: 1055,
            maxcombo: 287,
            fastcount: 15,
            slowcount: 601,
            judge_marvelous: 105,
            judge_perfect: 158,
            judge_great: 415,
            judge_good: 43,
            judge_ok: 3,
            judge_miss: 6,
            playing_flare: 2,
            endtime: 1_790_392_638_683,
            ..Note::default()
        };

        let converted = convert(&note).expect("should convert");
        assert_eq!(converted.score.optional.flare, Some("II"));
        assert_eq!(converted.score.lamp, Lamp::Clear);
        // (105 + 3) * 3 + 158 * 2 + 415 = 1055
        assert_eq!(converted.score.optional.ex_score, Some(1055));
    }

    #[test]
    fn the_flare_ladder_runs_from_none_to_ex() {
        let at = |rank| flare(&Note { playing_flare: rank, ..afronova() });

        // Flare 0 is Tachi's own default, so there is nothing to send.
        assert_eq!(at(0), None);
        assert_eq!(at(1), Some("I"));
        assert_eq!(at(9), Some("IX"));
        assert_eq!(at(10), Some("EX"));
    }

    #[test]
    fn a_flare_rank_beyond_the_ladder_is_dropped_not_refused() {
        // Flare is optional and takes no part in a score's identity, so an unplaceable rank
        // must not cost the whole score.
        let note = Note { playing_flare: 11, ..afronova() };
        let converted = convert(&note).expect("the score should still convert");
        assert_eq!(converted.score.optional.flare, None);
        assert_eq!(converted.score.score, 981_120);
    }

    #[test]
    fn notetype_names_the_playstyle_as_well_as_the_difficulty() {
        // BABY BABY GIMME YOUR LOVE on doubles BASIC, refused while notetype was read as a
        // difficulty on its own. The level settles it: notetype 5 came with level 3, and
        // DP BASIC is level 3 in Tachi's seeds where SP BASIC is 2.
        let note = Note {
            stagenum: 1,
            mcode: 182,
            notetype: 5,
            level: 3,
            clearkind: 3,
            score: 812_150,
            exscore: 164,
            maxcombo: 34,
            judge_marvelous: 38,
            judge_perfect: 16,
            judge_great: 18,
            judge_good: 1,
            judge_miss: 7,
            playstyle: 1,
            endtime: 1_790_398_044_963,
            ..Note::default()
        };

        let converted = convert(&note).expect("should convert");
        assert_eq!(converted.playtype, "DP");
        assert_eq!(converted.score.difficulty, "BASIC");
        // (38 + 0) * 3 + 16 * 2 + 18 = 164
        assert_eq!(converted.score.optional.ex_score, Some(164));
    }

    #[test]
    fn the_notetype_ladder_runs_through_both_playstyles() {
        let at = |notetype| {
            let playstyle = if notetype >= 5 { 1 } else { 0 };
            convert(&Note { notetype, playstyle, ..afronova() })
                .map(|c| (c.playtype, c.score.difficulty))
        };

        assert_eq!(at(0), Ok(("SP", "BEGINNER")));
        assert_eq!(at(4), Ok(("SP", "CHALLENGE")));
        // Doubles picks up at BASIC: there is no doubles BEGINNER chart.
        assert_eq!(at(5), Ok(("DP", "BASIC")));
        assert_eq!(at(8), Ok(("DP", "CHALLENGE")));
        assert_eq!(at(9), Err(Refusal::UnknownNotetype(9)));
    }

    #[test]
    fn a_notetype_that_contradicts_playstyle_is_refused() {
        // notetype 5 is a doubles chart; playstyle 0 claims singles.
        let note = Note { notetype: 5, playstyle: 0, ..afronova() };
        assert_eq!(
            convert(&note).err(),
            Some(Refusal::PlaystyleConflict {
                notetype: 5,
                from_notetype: "DP",
                playstyle: 0
            })
        );
    }

    /// End to end over a captured payload: the game's own JSON in, Tachi's JSON out. This
    /// is what catches a serde field name that does not match what the game sends, which
    /// the struct-level tests above cannot see.
    #[test]
    fn a_captured_payload_becomes_the_expected_tachi_import() {
        use crate::types::fixtures::USERSAVE;
        use crate::types::game::Envelope;
        use crate::types::tachi::{Import, ImportMeta};

        let envelope: Envelope =
            serde_json::from_str(USERSAVE).expect("the captured payload should parse");

        assert_eq!(envelope.eacnet.info.game_id, "ddr");
        assert_eq!(envelope.eacnet.request.method, "usergamedata_advanced");

        let save = &envelope.eacnet.request.data.data;
        assert_eq!(save.mode, "usersave");
        assert!(!save.isgameover);

        // Five slots arrive; only the filled one is a play.
        let played: Vec<_> = save.note.iter().filter(|note| !note.is_empty()).collect();
        assert_eq!(played.len(), 1);

        let converted = convert(played[0]).expect("a cleared play should convert");
        let import = Import {
            meta: ImportMeta {
                game: "ddr",
                playtype: converted.playtype,
                service: "arrabbiata".to_string(),
                version: None,
            },
            scores: vec![converted.score],
        };

        let json: serde_json::Value = serde_json::to_value(&import).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "meta": { "game": "ddr", "playtype": "SP", "service": "arrabbiata" },
                "scores": [{
                    "matchType": "inGameID",
                    "identifier": "37270",
                    "difficulty": "DIFFICULT",
                    "lamp": "CLEAR",
                    "score": 877490,
                    "timeAchieved": 1790389696371i64,
                    "judgements": {
                        "MARVELOUS": 157, "PERFECT": 123, "GREAT": 84,
                        "GOOD": 1, "OK": 1, "MISS": 11
                    },
                    "optional": { "exScore": 804, "fast": 30, "slow": 178, "maxCombo": 260 }
                }]
            }),
            "the import Tachi receives should match the results screen"
        );

        // version is left out entirely rather than sent empty.
        assert!(json["meta"].get("version").is_none());
    }
}
