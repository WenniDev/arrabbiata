//! A real `usersave` payload, kept so the parsing path is tested against what the game
//! actually sends rather than against a hand-built struct.
//!
//! Taken from a capture of Arrabbiata on DIFFICULT 13 -- cleared with 11 misses, which the
//! results screen recorded as 877,490 with an EX score of 804. The identifying fields
//! (`refid`, `ddrcode`, `name`, `client_key`, `token`) are replaced with placeholders; every
//! field the hook reads is verbatim, including the second, empty `note` slot.

pub const USERSAVE: &str = r#"{
  "eacnet": {
    "request": {
      "service": "local",
      "module": "playerdata_2",
      "method": "usergamedata_advanced",
      "data": {
        "client_key": "00000000-0000-0000-0000-000000000000",
        "data": {
          "mode": "usersave",
          "refid": "0000000000000000",
          "ddrcode": 100000000,
          "name": "PLAYER",
          "isgameover": false,
          "note": [
            {
              "stagenum": 2,
              "mcode": 37270,
              "notetype": 2,
              "level": 13,
              "rank": 4,
              "clearkind": 3,
              "score": 877490,
              "exscore": 804,
              "maxcombo": 260,
              "life": -1,
              "fastcount": 30,
              "slowcount": 178,
              "judge_marvelous": 157,
              "judge_perfect": 123,
              "judge_great": 84,
              "judge_good": 1,
              "judge_boo": 0,
              "judge_miss": 11,
              "judge_ok": 1,
              "judge_ng": 0,
              "playstyle": 0,
              "playing_flare": 0,
              "endtime": 1790389696371,
              "basename": "arra"
            },
            {
              "stagenum": 0,
              "mcode": 0,
              "notetype": 0,
              "level": 0,
              "rank": 0,
              "clearkind": 0,
              "score": 0,
              "exscore": 0,
              "maxcombo": 0,
              "life": 0,
              "fastcount": 0,
              "slowcount": 0,
              "judge_marvelous": 0,
              "judge_perfect": 0,
              "judge_great": 0,
              "judge_good": 0,
              "judge_boo": 0,
              "judge_miss": 0,
              "judge_ok": 0,
              "judge_ng": 0,
              "playstyle": 0,
              "playing_flare": 0,
              "endtime": 0,
              "basename": ""
            }
          ]
        }
      }
    },
    "info": {
      "game_id": "ddr",
      "soft_version": "VGP:J:A:A:2026082600",
      "token": "00000000-0000-0000-0000-000000000000",
      "retry_count": 0
    }
  }
}"#;
