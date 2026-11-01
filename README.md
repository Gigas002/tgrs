# tgt

A simple TUI for Telegram

## Installation

### Requirements

`tgt` does not download or bundle any native library. Install these system-wide with your package manager before building:

| Dependency                                      | Required for              | Notes                                                                                                                                                              |
| ----------------------------------------------- | ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| [TDLib](https://github.com/tdlib/td) (`tdjson`) | always                    | Must be discoverable via `pkg-config` (`pkg-config --modversion tdjson`). Version must match the one expected by [tdlib-rs](https://github.com/Gigas002/tdlib-rs). |
| `pkg-config`                                    | always                    | Used to locate the native libraries.                                                                                                                               |
| `xdg-open` (`xdg-utils`)                        | opening media             | Media is opened with your default applications, e.g. mpv for videos and audio, an image viewer for photos.                                                         |
| D-Bus (`libdbus-1`)                             | `keychain` (default)      | Found with `pkg-config` (`dbus-1`); talks to the keychain.                                                                                                         |
| A Secret Service provider                       | `keychain` (default)      | E.g. GNOME Keyring or KeePassXC. Without one, `store = "auto"` leaves the TDLib database unencrypted.                                                              |
| A notification server                           | `notifications` (default) | E.g. mako, dunst or your desktop's own; reached over D-Bus.                                                                                                        |

For example, on Arch Linux: `sudo pacman -S tdlib pkgconf xdg-utils dbus gnome-keyring`.

### Build features

Build features can be combined (e.g. `cargo build --release --no-default-features --features media-preview` for previews without the keychain).

| Feature         | Description                                                                                                                                                 |
| --------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `default`       | Enables `keychain`, `media-preview` and `notifications`.                                                                                                    |
| `keychain`      | Encrypt the TDLib database (your login session) with a key kept in the system keychain (Secret Service). See `[credentials]` in `config.toml`.              |
| `media-preview` | Image previews in the chat: photos, static stickers and the thumbnails of videos, GIFs, video messages and files.                                            |
| `notifications` | Desktop notifications for new messages, following each chat's Telegram notification settings. See `[notifications]` in `config.toml`.                       |

## Quick start

```sh
cargo build --release
./target/release/tgt
```

The first run logs in on the terminal (phone number, code and, if set, your two-step verification password), then opens the TUI. Move between panes with `left`/`right`, open a chat with `enter`, and press `space` for the leader menu or `alt+f1` to list every key. `tgt logout` logs out again.

## Documentation

- [Usage](docs/USAGE.md): navigation, chats, messages, media, configuration, keybindings and themes.
- [Architecture](docs/ARCHITECTURE.md): how the code is organized.
- [Feature matrix](docs/TELEGRAM_FEATURE_MATRIX.md): what Telegram features `tgt` supports.
