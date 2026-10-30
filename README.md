# tgt

A simple TUI for Telegram

## Usage

### Requirements

`tgt` does not download or bundle any native library. Install these system-wide with your package manager before building:

| Dependency                                      | Required for              | Notes                                                                                                                                                              |
| ----------------------------------------------- | ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| [TDLib](https://github.com/tdlib/td) (`tdjson`) | always                    | Must be discoverable via `pkg-config` (`pkg-config --modversion tdjson`). Version must match the one expected by [tdlib-rs](https://github.com/Gigas002/tdlib-rs). |
| `pkg-config`                                    | always                    | Used to locate the native libraries.                                                                                                                               |
| `xdg-open` (`xdg-utils`)                        | opening media             | Media is opened with your default applications, e.g. mpv for videos and audio, an image viewer for photos.                                                         |
| D-Bus (`libdbus-1`)                             | `keychain` (default)      | Found with `pkg-config` (`dbus-1`); talks to the keychain.                                                                                                         |
| A Secret Service provider                       | `keychain` (default)      | E.g. GNOME Keyring or KeePassXC. Without one, `store = "auto"` leaves the TDLib database unencrypted.                                                              |

For example, on Arch Linux: `sudo pacman -S tdlib pkgconf xdg-utils dbus gnome-keyring`.

### Features

Build features can be combined (e.g. `cargo build --release --no-default-features --features media-preview` for previews without the keychain).

| Feature         | Description                                                                                                                                                 |
| --------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `default`       | Enables `keychain` and `media-preview`.                                                                                                                     |
| `keychain`      | Encrypt the TDLib database (your login session) with a key kept in the system keychain (Secret Service). See `[credentials]` in `config.toml`.              |
| `media-preview` | Image previews in the chat and the media viewer: photos, static stickers and the thumbnails of videos, GIFs, video messages and files.                       |

### Media

`tgt` doesn't play media itself. With `media-preview`, images (and the thumbnails of videos, GIFs and files) are previewed under their messages in the chat; select a message and:

| Key     | Action                                                                                                               |
| ------- | -------------------------------------------------------------------------------------------------------------------- |
| `o`     | Open the media (photo, video, GIF, audio, voice message, sticker or file) in its default application via `xdg-open`. |
| `v`     | Open the media viewer: file details and a preview. There `o` opens, `d` downloads.                                   |
| `s`     | Download the media or file (archives, documents, ...) to `app.download_dir`.                                         |
| `Enter` | Open the message actions menu, listing every action with its key.                                                    |

Files are fetched from Telegram first when needed; progress and errors are shown in the status bar. Downloads never overwrite: `file.zip` becomes `file (1).zip`. Programs and scripts (`.sh`, `.desktop`, `.AppImage`, `.jar`, ...) are never opened, only downloaded.

### Configuration

`tgt` works without any config file: defaults are bundled in the binary. Files are looked up in XDG directories:

| What                                       | Location                                                                       |
| ------------------------------------------ | ------------------------------------------------------------------------------ |
| `config.toml`, `keymap.toml`, `theme.toml` | `$XDG_CONFIG_HOME/tgt` (`~/.config/tgt`), or `--config-dir` / `TGT_CONFIG_DIR` |
| TDLib database                             | `$XDG_DATA_HOME/tgt/tdlib` (`~/.local/share/tgt/tdlib`)                        |
| Logs, TDLib log                            | `$XDG_STATE_HOME/tgt` (`~/.local/state/tgt`)                                   |
| TDLib database key                         | the system keychain, service `tgt` (see `[credentials]`)                       |
| Downloads                                  | `app.download_dir`, default `$XDG_DOWNLOAD_DIR` or `~/Downloads`               |

Settings are resolved as `CLI > environment > config.toml > defaults`. They are only changed by editing the files: `tgt` never writes to them and has no settings UI. `API_ID` and `API_HASH` override the Telegram credentials. User keybindings are merged over the defaults: binding a command replaces its default keys, and every other default stays.

| Command      | Description                                             |
| ------------ | ------------------------------------------------------- |
| `tgt`        | Open the TUI (logs in on the terminal first if needed). |
| `tgt logout` | Log out of Telegram and remove the database key.        |

Global options: `--config-dir`, `--log-dir`, `--log-level`, `-v`/`-q`.

Chats are only read and messages only sent from the TUI: there is no command-line or scripting interface to the account.

### Theme

`tgt` follows your terminal's colors: text uses the terminal's default foreground and background, accents use its ANSI palette, and secondary text is dimmed rather than given a fixed color. To change a color, add a `theme.toml` with Highlight Groups, like Vim's:

```toml
[highlight.FocusBorder]
foreground = "light_magenta"

[highlight.SelectedRow]
background = "#303030"

[ui.border]
default = "rounded"
```

A group inherits the fields it doesn't set from the group it links to, so changing `Muted` changes timestamps, hints and previews together. [`examples/theme.toml`](examples/theme.toml) lists every group with its default. Invalid entries are skipped with a warning.

To customize, copy the files you want to change from [`examples/`](examples/) into the config directory; missing files keep their defaults. To reset or uninstall, remove the directories above yourself, and the database key with `secret-tool clear service tgt`.
