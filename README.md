# tgrs

A simple TUI for Telegram

Originally a fork of [tgt](https://github.com/FedericoBruzzone/tgt) by Federico Bruzzone.

## Installation

### Requirements

`tgrs` does not download or bundle any native library. Install these system-wide with your package manager before building:

| Dependency                                      | Required for              | Notes                                                                                                                                                              |
| ----------------------------------------------- | ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| [TDLib](https://github.com/tdlib/td) (`tdjson`) | always                    | Must be discoverable via `pkg-config` (`pkg-config --modversion tdjson`). Version must match the one expected by [tdlib-rs](https://github.com/Gigas002/tdlib-rs). |
| `pkg-config`                                    | always                    | Used to locate the native libraries.                                                                                                                               |
| `xdg-desktop-portal` and a backend              | `file-chooser` (default)  | The desktop's file dialog for sending files, e.g. `xdg-desktop-portal-gtk`, `-kde` or `-cosmic`; without one, the built-in file explorer is used.                 |
| `xdg-open` (`xdg-utils`)                        | opening media             | Media is opened with your default applications, e.g. mpv for videos and audio, an image viewer for photos.                                                         |
| D-Bus (`libdbus-1`)                             | `keychain`, `file-chooser` | Found with `pkg-config` (`dbus-1`); talks to the keychain and the desktop portal.                                                                                                       |
| A Secret Service provider                       | `keychain` (default)      | E.g. GNOME Keyring or KeePassXC. Without one, `store = "auto"` leaves the TDLib database unencrypted.                                                              |
| A notification server                           | `notifications` (default) | E.g. mako, dunst or your desktop's own; reached over D-Bus.                                                                                                        |

### Build features

Build features can be combined (e.g. `cargo build --release --no-default-features --features media-preview` for previews without the keychain).

| Feature         | Description                                                                                                                                    |
| --------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| `default`       | Enables `file-chooser`, `keychain`, `media-preview` and `notifications`.                                                                       |
| `file-chooser`  | Pick files to send in the desktop's file dialog, through the XDG desktop portal on D-Bus. See `app.file_chooser` in `config.toml`.             |
| `keychain`      | Encrypt the TDLib database (your login session) with a key kept in the system keychain (Secret Service). See `[credentials]` in `config.toml`. |
| `media-preview` | Image previews in the chat: photos, static stickers and the thumbnails of videos, GIFs, video messages and files.                              |
| `notifications` | Desktop notifications for new messages, following each chat's Telegram notification settings. See `[notifications]` in `config.toml`.          |

## Documentation

- [Usage](docs/USAGE.md): navigation, chats, messages, media, configuration, keybindings and themes.
- [Architecture](docs/ARCHITECTURE.md): how the code is organized.
- [Feature matrix](docs/TELEGRAM_FEATURE_MATRIX.md): what Telegram features `tgrs` supports.
