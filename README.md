# Arrabbiata

Little DDR GRAND PRIX hook to submit your scores to a Tachi instance while you are playing.

Forked from [mikado](https://github.com/adamaq01/mikado), which does the same for SDVX.

## Features

- Submit scores to a Tachi instance after each stage
- Singles and doubles

## Installation

- Download the latest release from the [releases page](https://github.com/WenniDev/arrabbiata/releases/latest)
- Put it in your game's `game\modules` folder, next to `avs2-core.dll`
- Add a line for it at the end of `chainload.txt`
- Start the game once, then set your API key in the config file

## Tips

- The configuration file is created in the `game` folder at startup if it doesn't already exist
- Your Tachi API key needs the `submit_score` permission
- Set `enable` to `false` in `arrabbiata.toml` to turn the hook off without removing it

## License

MIT
