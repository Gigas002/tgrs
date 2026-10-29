# tgt

A simple TUI for Telegram

## Usage

### Requirements

`tgt` does not download or bundle any native library. Install these system-wide with your package manager before building:

| Dependency                                      | Required for              | Notes                                                                                                                                                              |
| ----------------------------------------------- | ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| [TDLib](https://github.com/tdlib/td) (`tdjson`) | always                    | Must be discoverable via `pkg-config` (`pkg-config --modversion tdjson`). Version must match the one expected by [tdlib-rs](https://github.com/Gigas002/tdlib-rs). |
| `pkg-config`                                    | always                    | Used to locate the native libraries.                                                                                                                               |
| ALSA (`libasound`)                              | `audio` (default)         | Audio output.                                                                                                                                                      |
| [Opus](https://opus-codec.org/) (`libopus`)     | `voice-message` (default) | Telegram voice notes (OGG Opus).                                                                                                                                   |
| [chafa](https://github.com/hpjansson/chafa)     | `chafa-dyn` (optional)    | Image rendering in the photo viewer.                                                                                                                               |

For example, on Arch Linux: `sudo pacman -S tdlib pkgconf alsa-lib opus chafa`.

### Features

Build features can be combined (e.g. `cargo build --release --features chafa-dyn`).

| Feature         | Description                                                                                                                                                                         |
| --------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `default`       | Enables `audio` and `voice-message`.                                                                                                                                                |
| `audio`         | Audio playback (e.g. MP3) via rodio.                                                                                                                                                |
| `voice-message` | Play Telegram voice notes (OGG Opus). Requires the system `libopus`. Use `--no-default-features` to disable it; the app will then show a message when you try to play a voice note. |
| `chafa-dyn`     | Enable [chafa](https://github.com/hpjansson/chafa)-based image rendering in the photo viewer. Requires the chafa library installed on the system.                                   |

### Configuration

`tgt` works without any config file: defaults are bundled in the binary. Files are looked up in XDG directories:

| What                                          | Location                                                                       |
| --------------------------------------------- | ------------------------------------------------------------------------------ |
| `config.toml`, `keymap.toml`, `themes/*.toml` | `$XDG_CONFIG_HOME/tgt` (`~/.config/tgt`), or `--config-dir` / `TGT_CONFIG_DIR` |
| TDLib database                                | `$XDG_DATA_HOME/tgt/tdlib` (`~/.local/share/tgt/tdlib`)                        |
| Logs, TDLib log, last picked theme            | `$XDG_STATE_HOME/tgt` (`~/.local/state/tgt`)                                   |

Settings are resolved as `CLI > environment > config.toml > defaults`. `API_ID` and `API_HASH` override the Telegram credentials. User keybindings are merged over the defaults: binding a command replaces its default keys, and every other default stays.

| Command                                 | Description                                               |
| --------------------------------------- | --------------------------------------------------------- |
| `tgt`                                   | Open the TUI (logs in on the terminal first if needed).   |
| `tgt send CHAT MESSAGE`                 | Send one message to the only chat matching `CHAT`.        |
| `tgt logout`                            | Log out of Telegram.                                      |
| `tgt init-config [--force]`             | Write the default config files into the config directory. |
| `tgt clear [-c] [-d] [-s] [--all] [-y]` | Remove the config, data and/or state directories.         |

Global options: `--config-dir`, `--log-dir`, `--log-level`, `-v`/`-q`, `--theme`.

Default config files, including the bundled themes, live in [`examples/`](examples/).
