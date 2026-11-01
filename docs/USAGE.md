# Using tgt

How to run `tgt`, move around, read and write messages, and configure it. For installing and building, see the [README](../README.md).

## Running

| Command      | Description                                             |
| ------------ | ------------------------------------------------------- |
| `tgt`        | Open the TUI (logs in on the terminal first if needed). |
| `tgt logout` | Log out of Telegram and remove the database key.        |

Global options: `--config-dir`, `--log-dir`, `--log-level`, `-v`/`-q`.

Chats are only read and messages only sent from the TUI: there is no command-line or scripting interface to the account.

## Navigation

Like [concord](https://github.com/chojs23/concord): `left`/`right` move between the chat list, the chat and the prompt, `up`/`down` select inside a pane, and `i` in the chat starts writing (`esc` goes back to the chat). A pane always has the focus, the chat list when `tgt` starts; `esc` closes a popup or leaves the prompt.

| Key                         | Action                                           |
| --------------------------- | ------------------------------------------------ |
| `alt+1` / `alt+2` / `alt+3` | Focus the chat list, the chat, the prompt.       |
| `alt+right` / `alt+left`    | Make the focused pane wider / narrower.          |
| `alt+up` / `alt+down`       | Make the focused pane taller / shorter.          |
| `space`                     | Open the [leader menu](#leader-menu).            |
| `alt+f1`                    | List every key.                                  |

### Leader menu

`space` opens the leader menu, which lists what comes next. Its entries are the `[leader_menu]` section of `keymap.toml`.

| Key     | Action                                       |
| ------- | -------------------------------------------- |
| `1`     | Hide or show the chat list.                  |
| `2`     | Hide or show the topic list.                 |
| `3`     | Hide or show the archive.                    |
| `a`     | Actions of the selected message.             |
| `space` | Search chats.                                |
| `/`     | Search messages.                             |
| `p`     | Pinned messages.                             |
| `u`     | Upload a file, captioned by the prompt.      |
| `s`     | Send a sticker.                              |
| `c`     | Actions of the open chat.                    |
| `i`     | Profile of the open chat.                    |
| `m`     | Your own profile.                            |
| `e`     | [Edit your profile](#editing-your-profile).  |
| `d`     | [Active sessions](#active-sessions).         |
| `t`     | [Downloads](#downloads).                     |
| `n`     | Open a chat by username.                     |
| `b`     | [Blocked users](#blocking-and-reporting).    |
| `?`     | List every key.                              |

## Chats

### Topics

Groups with topics (forums) open into a topic list, a pane right of the chat list: `enter` on a topic shows its messages in the chat, and messages you send go to that topic. The topic list resizes like the other panes, `alt+t` focuses it and `alt+m` hides or shows it.

### Archive

Archived chats are behind the `📦 Archived chats` row at the top of the chat list, which names them and counts the unread ones: `enter` on it opens the archive, a pane right of the chat list, and `enter` there opens an archived chat. Opening a chat from the chat list closes the archive again; `alt+a` focuses it and `alt+b` hides or shows it.

### Chat actions

In the chat list and the archive, these keys act on the selected chat (leader `c` opens the actions of the open chat):

| Key | Action                                                                                         |
| --- | ---------------------------------------------------------------------------------------------- |
| `a` | List the chat's actions with their keys.                                                       |
| `m` | Mute or unmute.                                                                                |
| `A` | Archive or unarchive.                                                                          |
| `P` | Pin or unpin.                                                                                  |
| `K` / `J` | Move a pinned chat up / down among the pinned chats (also `shift+up` / `shift+down`).    |
| `u` | Mark as read or unread.                                                                        |
| `B` | Block or unblock the person or bot of a private chat.                                          |
| `!` | [Report](#blocking-and-reporting) the chat.                                                    |
| `D` | Leave a group or channel, or delete any other chat; press `D` again or `enter` to confirm.     |
| `i` | Show the chat's [profile](#profiles).                                                          |
| `n` | [Open a chat by username](#opening-a-chat-by-username).                                        |

### Profiles

`i` in the chat list or the archive shows the selected chat's profile, leader `i` the open chat's and leader `m` your own:

- a person or bot: name, usernames, phone number, bio, birthday, last seen and ID;
- a group or channel: usernames, description and member count.

The bio or description is loaded from Telegram when the profile opens. `j`/`k` scroll a long profile; `esc`, `q` or `i` close it. In your own profile, `e` [edits it](#editing-your-profile).

### Editing your profile

Leader `e` (or `e` in your own profile) edits your first and last name, bio and username. `tab`/`shift+tab` (or `down`/`up`, or a click) move between the fields, `enter` saves what changed and `esc` closes without saving. The fields can be edited once your bio has loaded. The first name can't be empty, and a username has 5 to 32 Latin letters, digits and underscores, starting with a letter; leave the username empty to remove it. Only your editable username can be changed, not collectible ones. If Telegram refuses a change, for example a taken username or a bio that's too long, the popup says why and moves to that field. When you opened the editor from your profile, saving shows your profile again.

### Active sessions

Leader `d` lists the devices and apps logged in to your account, in Telegram's order with tgt's own session first: each shows the app and its version, the device and system, where it logged in from and when it was last active. `j`/`k` move, `D` (or `delete`) logs the highlighted session out and `T` logs out every session but tgt's; both ask for the same key a second time, as the popup's title says. tgt's own session is ended with `tgt logout` instead. A new sign-in that hasn't been confirmed yet is marked `(new, unconfirmed)`: `c` confirms it as yours, or log it out if it isn't.

When someone logs in to your account and the login is still unconfirmed, the status bar warns about it, naming the device and place, e.g. `New sign-in on Pixel 8 from Berlin, Germany. Not you? Review sessions with space d.`

### Blocking and reporting

`B` in the chat list or the archive blocks the person or bot of the selected private chat, or unblocks them; leader `b` lists everyone you've blocked, and `u` or `enter` there unblocks the highlighted one.

`!` reports the selected chat, or in the chat the selected message, to Telegram's moderators. Telegram asks for a reason, sometimes narrowing it down over a few lists (`1`-`9` pick directly, `j`/`k` or the arrows move, `enter` picks), and then maybe for details, which you type and send with `enter`. `esc` stops at any point without reporting. Some chats can only be reported along with their messages: report one of the messages instead.

### Opening a chat by username

`n` in the chat list or the archive (leader `n` elsewhere) asks for a public username. Type `@name` or `name`, or paste a `t.me/name` link, and `enter` opens the person's, bot's, group's or channel's chat, even if it isn't in your chat list yet. An unknown or invalid username is reported in the popup; `esc` closes it.

## Messages

In the prompt, `enter` sends; `alt+enter` or `ctrl+j` start a new line (also `shift+enter` in terminals with the kitty keyboard protocol, e.g. kitty, foot, WezTerm, Ghostty).

### Drafts

What you write and don't send stays with its chat (or forum topic) as a draft, kept by Telegram and shared with your other devices: switching chats saves it and brings back the next chat's, and so does leaving the prompt. In the chat and topic lists, a chat with a draft shows `Draft:` and its text instead of its last message. A draft written on another device appears in the prompt unless you've changed the text here since. Editing a message doesn't touch the draft: once the edit is sent or cancelled, the draft comes back. Cancelling a reply keeps what you wrote.

`alt+s` opens the sticker picker: your recent stickers and installed sets as tabs (`tab`/`shift+tab`) over a grid of stickers (arrows or `h`/`j`/`k`/`l`), drawn as images with the `media-preview` feature and as their emoji otherwise. `enter` sends the selected one, as a reply when you're replying.

### Message actions

Select a message in the chat (`up`/`down`) and press a key, or `enter` for the actions menu, which lists every action with its key and dims the ones that don't apply. The keys follow [concord](https://github.com/chojs23/concord)'s:

| Key | Action                                                                                             |
| --- | -------------------------------------------------------------------------------------------------- |
| `y` | Copy the text.                                                                                     |
| `r` | React: pick an emoji the chat allows; picking one of yours removes it. `1`-`9`, `0` pick directly. |
| `R` | Reply.                                                                                             |
| `e` | Edit your message, or the caption of your photo, video, file or other media.                       |
| `d` | Delete your message for everyone (`D`: only for you).                                              |
| `o` | Open a link of the message in the browser, picked from a list when there are several.              |
| `x` | Open the media (photo, video, GIF, audio, voice message, sticker or file) via `xdg-open`.          |
| `s` | Download the media or file (archives, documents, ...) to `app.download_dir`.                       |
| `g` | Go to the message it replies to.                                                                   |
| `P` | Pin or unpin the message for everyone.                                                             |
| `L` | Copy the message's link (supergroups and channels).                                                |
| `!` | Report the message to Telegram's moderators (not your own).                                        |
| `f` | [Forward](#forwarding) the message to another chat.                                                |

Reactions are shown under their messages, yours highlighted, and pinned messages get a 📌.

### Forwarding

`f` on a selected message (or "forward message" in its actions menu) asks where to send it: type part of a chat's name to narrow your chats and archived chats down, best match first, move with `up`/`down` (or `ctrl+p`/`ctrl+n`, or a click) and press `enter` to forward. `tab` toggles **without the original author**, which sends a copy as your own message instead of a "Forwarded from" one. Forwarding to a forum goes to its General topic. Messages from chats that protect their content can't be forwarded, and the menu dims the action for them. The status bar says where the message went, or why Telegram refused, e.g. when you can't write in that chat.

### Media and files

To send a file, write its caption in the prompt (or leave it empty), then press `alt+u` (leader `u`) and pick the file: it's sent with the prompt's text as its caption, as a reply if you were replying, and the prompt is cleared. While you're editing a message, the text stays and the file goes without a caption. Files are sent as documents, so photos keep their full quality. In the upload explorer, `a` attaches the highlighted file and keeps the explorer open to pick more; `enter` then sends the attached files together with the highlighted one. `e` on one of your media messages edits its caption; send it empty to remove the caption. Stickers and round videos have none. If Telegram refuses an upload or an edit, the status bar says why.

You can also drop files on the terminal window: most terminals paste the paths of dropped files, and a paste that is only paths of existing files (quoted, with escaped spaces or as `file://` URIs) attaches them to the prompt instead of typing them. `alt+v` attaches the image on the clipboard, e.g. a screenshot, saved as a PNG (this needs the `media-preview` feature). The prompt's title lists what's attached: `enter` sends the files, the first with the prompt's text as its caption, grouped as albums of up to 10 (split evenly, so 11 files are albums of 6 and 5), and `esc` turns them back into the pasted text, for when you meant to paste a path. Files aren't attached while editing a message, and switching chats drops them. Dropping needs `app.paste_support`, which is on by default.

`tgt` doesn't play media itself. With `media-preview`, images (and the thumbnails of videos, GIFs and files) are previewed under their messages. Files are fetched from Telegram first when needed; progress and errors are shown in the status bar.

- Downloads are listed by leader `t`, see [Downloads](#downloads).
- Downloads never overwrite: `file.zip` becomes `file (1).zip`.
- Programs and scripts (`.sh`, `.desktop`, `.AppImage`, `.jar`, ...) are never opened, only downloaded.
- Only web, e-mail, phone and `tg:` links are opened; others, like `file:` links, are refused.

### Downloads

Leader `t` lists the files you've downloaded with `s` or opened with `x` since tgt started, latest first, with their progress: `⬇` downloading, `⏸` paused, `✓` done, `✗` cancelled or failed. `p` pauses a download, keeping what was downloaded, and resumes a paused (or failed) one from where it stopped; once done, it's opened or saved as you first asked, even if you've switched chats. `x` cancels a download and deletes what was downloaded of it, `enter` opens a finished file and `c` clears finished downloads from the list. Image previews in the chat aren't listed.

## Configuration

`tgt` works without any config file: defaults are bundled in the binary. Files are looked up in XDG directories:

| What                                       | Location                                                                       |
| ------------------------------------------ | ------------------------------------------------------------------------------ |
| `config.toml`, `keymap.toml`, `theme.toml` | `$XDG_CONFIG_HOME/tgt` (`~/.config/tgt`), or `--config-dir` / `TGT_CONFIG_DIR` |
| TDLib database                             | `$XDG_DATA_HOME/tgt/tdlib` (`~/.local/share/tgt/tdlib`)                        |
| Logs, TDLib log                            | `$XDG_STATE_HOME/tgt` (`~/.local/state/tgt`)                                   |
| TDLib database key                         | the system keychain, service `tgt` (see `[credentials]`)                       |
| Downloads                                  | `app.download_dir`, default `$XDG_DOWNLOAD_DIR` or `~/Downloads`               |

Settings are resolved as `CLI > environment > config.toml > defaults`. They are only changed by editing the files: `tgt` never writes to them and has no settings UI. `API_ID` and `API_HASH` override the Telegram credentials.

To customize, copy the files you want to change from [`examples/`](../examples/) into the config directory; missing files keep their defaults.

### Keybindings

User keybindings in `keymap.toml` are merged over the defaults: binding a command replaces its default keys, and every other default stays. Each section applies to one part of the UI (`[chat_list]`, `[chat]`, `[prompt]`, `[leader_menu]`, ...); [`examples/keymap.toml`](../examples/keymap.toml) lists every default binding and command.

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

A group inherits the fields it doesn't set from the group it links to, so changing `Muted` changes timestamps, hints and previews together. [`examples/theme.toml`](../examples/theme.toml) lists every group with its default. Invalid entries are skipped with a warning.

## Resetting and uninstalling

To reset or uninstall, remove the directories listed under [Configuration](#configuration) yourself, and the database key with `secret-tool clear service tgt`.
