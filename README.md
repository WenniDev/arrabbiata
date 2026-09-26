# Arrabbiata

Little DDR GRAND PRIX hook to submit your scores to a Tachi instance while you are playing.

Forked from [mikado](https://github.com/adamaq01/mikado), which does the same for SDVX.

## Features

- Submit scores to a Tachi instance after each stage
- Singles and doubles, with judgements, EX score, Flare, fast/slow and max combo
- Refuse anything that does not add up, and write the payload out instead of guessing

## Installation

- Download the latest release from the [releases page](https://github.com/WenniDev/arrabbiata/releases/latest)
- Put `arrabbiata.dll` in `<game>\game\modules\`, next to `avs2-core.dll`
- Add a line for it at the end of `chainload.txt`, after `cluedo.dll`
- Start the game once, then put your Tachi API key in `arrabbiata.toml`

## Tips

- The configuration file is created in the game's working directory, `<game>\game\` — one
  folder above the DLL
- Your Tachi API key needs the `submit_score` permission
- Set `general.submit` to `false` for a dry run: each score is worked out and printed in full,
  but nothing is sent
- A refused score is written to `arrabbiata-dumps\`. Please report those — see
  [docs/protocol.md](docs/protocol.md)

## License

MIT
