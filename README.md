# Arrabbiata

Little DDR GRAND PRIX hook to send your scores to Tachi and Upscore while you are playing.

Forked from [mikado](https://github.com/adamaq01/mikado), which does the same for SDVX.

## Features

- Submit scores to a Tachi instance after each stage
- Send the same plays to [Upscore](https://upscore.dance/)
- Singles and doubles

## Installation

- Download the latest release from the [releases page](https://github.com/WenniDev/arrabbiata/releases/latest)
- Drop `d3d9.dll` in the folder holding `ddr-konaste.exe`
- Start the game once, then set your Tachi and Upscore API keys in the config file and restart

## Tips

- Loading other DLLs too? Use [d3d9_chainload](https://github.com/adamaq01/d3d9_chainload)
- The configuration file is created one folder above the DLL at startup, if it isn't there already
- Your Tachi API key needs the `submit_score` permission
- Set `enable` to `false` in `arrabbiata.toml` to turn the hook off without removing it

## License

MIT
