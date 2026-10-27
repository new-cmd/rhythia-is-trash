<div align="center">

# rhythia is trash

[![ci](https://img.shields.io/github/actions/workflow/status/sjdsjadjajdjsa/rhythia-is-trash/ci.yml?branch=main&label=ci&logo=github&logoColor=white)](https://github.com/sjdsjadjajdjsa/rhythia-is-trash/actions/workflows/ci.yml)
[![release](https://img.shields.io/github/v/release/sjdsjadjajdjsa/rhythia-is-trash?color=ff0059)](https://github.com/sjdsjadjajdjsa/rhythia-is-trash/releases/latest)
[![license](https://img.shields.io/github/license/sjdsjadjajdjsa/rhythia-is-trash?color=green)](LICENSE)
[![platform](https://img.shields.io/badge/platform-Linux%20%7C%20Windows-lightgrey)](#install)
[![made with](https://img.shields.io/badge/made_with-Rust-orange?logo=rust)](https://www.rust-lang.org)

A fast Rhythia-style 3D aim rhythm game, rewritten from scratch in Rust.

![Playing the built-in demo song in the Hot Pink look: score, multiplier and accuracy panels around the grid, a note flying in](docs/images/hot-pink.webp)

</div>

Notes fly at you out of a 3D space. Move your mouse over them as they reach the grid. There's no clicking, only aiming. It plays maps from Rhythia, Sound Space and osu!, and it can make a map from any song you drop on it.

## What you get

- **Your maps:** `.sspm` (v1 and v2), `.phxm`, `.rhm`, `.txt`, `.osu` and `.osz`. Drag files or folders onto the window and they're checked and copied in.
- **Any song:** drop an `.mp3`, `.ogg`, `.wav` or `.flac` and it charts the beats for you.
- **A demo song built in**, so it works the moment you open it.
- **Nine looks and three spaces:** Hot Pink, Acid, Ice, Amber, Violet, Clarity, Mint, Mono and Competitive, over a grid, a tunnel or an open void. Or set your own wallpaper.
- **Low latency:** input is read as late as possible each frame. The bottom line shows frame, audio and pacing times live.
- **Offset calibration:** tap along to find your audio offset, then fine-tune it in game with `[` and `]`.
- **Scores and replays:** your best on each map is kept. Press `V` on the results screen to watch your run back.
- **Original sounds and visuals:** hit sounds are generated in code, and no art, sounds or code from the original game are included.

## Screenshots

| Song list | Ice, tunnel |
| --- | --- |
| ![The song list with the built-in demo](docs/images/library.webp) | ![The Ice look in the tunnel space](docs/images/ice-tunnel.webp) |

| Violet, void | Acid, grid |
| --- | --- |
| ![The Violet look in the void space](docs/images/violet-void.webp) | ![The Acid look on the grid](docs/images/acid.webp) |

## Install

Download the newest version from [Releases](https://github.com/sjdsjadjajdjsa/rhythia-is-trash/releases/latest).

| | Download | Run |
| --- | --- | --- |
| **Linux** | `rhythia-is-trash-linux-x86_64.tar.gz` | Unpack it and run `./rhythm` from inside the folder. Needs glibc 2.35 or newer (Ubuntu 22.04, Fedora 36, Arch and so on). |
| **Windows** | `rhythia-is-trash-windows-x86_64.zip` | Unzip it anywhere and run `rhythm.exe`. |

Windows will say the publisher is unknown, as the program isn't signed: click "More info", then "Run anyway".

Your maps, settings and scores are kept next to the program. On Windows, if that folder can't be written to (for example under Program Files), they go to `%APPDATA%\rhythia-is-trash` instead.

## Controls

| Where | Keys |
| --- | --- |
| **Song list** | `Enter` play, `Tab` settings, `Del` remove a song, `Z` undo, `Esc` quit |
| **Playing** | mouse aims, `Esc` pause, `R` restart, `[` `]` offset, `-` `=` sensitivity |
| **Paused** | `Esc` or `Space` resume, `R` restart, `Q` back to the list |
| **Results** | `R` or `Space` play again, `V` watch the replay, `Esc` back |
| **Settings** | `Q` `E` switch tabs, arrows change values, `C` calibrate offset, `R` undo changes |
| **Anywhere** | `F2` glow, `F3` space, `F11` fullscreen, `F12` screenshot |

## Build it yourself

Needs [Rust](https://rustup.rs) 1.88 or newer. On Linux you also need the ALSA, X11, OpenGL and D-Bus headers. On Debian or Ubuntu:

```
sudo apt install build-essential pkg-config libasound2-dev libx11-dev libxi-dev libgl1-mesa-dev libdbus-1-dev
```

Then:

```
cargo run --release
```

`cargo run --release -- path/to/map.sspm` opens a map directly. `RHYTHM_PERF=1` prints frame, CPU and audio timing when a song ends.

[vendor/miniquad](vendor/miniquad) is a patched copy of miniquad 0.4.11. The patches add file-manager drag and drop on X11 and Windows, and late input sampling. [design_system](design_system) has notes on how everything is built and measured.

## License

[MIT](LICENSE). Noto Sans is bundled under the [SIL Open Font License](assets/fonts/NOTICE.txt). This is a fan project, not affiliated with Rhythia.
