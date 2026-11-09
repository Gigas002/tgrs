# Change Log
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/) and this project adheres to [Semantic Versioning](http://semver.org/).

## [Unreleased]

### Removed

- `app.frame_rate`: the screen is redrawn when something changes instead of at a fixed rate.
- In-app audio playback (`audio` and `voice-message` features, `rodio`, `ogg`, `audiopus`, ALSA and libopus): media is opened in external applications instead.
- Theme selector popup, theme switching and the remembered theme in `$XDG_STATE_HOME/tgrs/theme`: settings are only changed by editing `config.toml`.
- Palette themes (`themes/*.toml`, the bundled catppuccin, github, gruvbox, monokai, nord, onedark, tokyo and default themes), `--theme`, `app.theme` and `app.theme_enable`: tgrs follows the terminal's colors, and `theme.toml` overrides them.
- Photo viewer and the save-as file explorer: images are previewed in the chat, media is opened in external applications (`x`) and downloaded to `app.download_dir` (`s`).
- Commands `show_theme_selector`, `hide_theme_selector`, `switch_theme`, `view_photo_message`, `show_photo_viewer`, `hide_photo_viewer`, `photo_viewer_previous`, `photo_viewer_next`, `play_voice_message`, `start_file_download`, `hide_file_download_explorer`, `pinned_popup_view_photo` and `pinned_popup_play_voice`; keymap sections `[theme_selector]`, `[photo_viewer]` and `[file_download_explorer]`.
- Config keys `app.photo_max_dimension` and `app.photo_viewer_popup_size`: chat previews have a fixed size.

- Legacy config locations (`~/.tgrs`, `.tgrs-dev`, `.data`); only XDG directories are used.
- `app.toml`, `logger.toml` and `telegram.toml`, replaced by a single `config.toml`. `theme_filename`, `take_api_*_from_telegram_config` and the log file name options are gone.
- `--logout` flag, replaced by the `logout` subcommand.
- Commands that manage files: defaults apply when a config file is missing, and users copy, edit and remove the files themselves.
- Sending messages from the command line (`--send-message`): Telegram is only used through the TUI, so scripts and other programs can't act on the account.
- Writing to the user's config: keymaps are no longer upgraded in place and switching themes no longer rewrites `app.toml`.
- Dependencies `config`, `dirs`, `tracing-error` and the `env-filter` feature; `image` limited to GIF, JPEG, PNG and WebP decoding.
- CI workflows `ci-linux`, `cd` and `cleanup-caches`.

- Support for Windows, macOS and Android; `tgrs` now targets Linux only.
- Features that download, build or bundle native dependencies: `download-tdlib`, `local-tdlib`, `pkg-config`, `static`, `static-download`, `chafa-static` and `chafa-dyn`. TDLib (`tdjson`) and libdbus must be installed system-wide; TDLib is located via `pkg-config`.
- CI workflows for Windows, macOS and Android.
- Docker image (`Dockerfile`, Docker CI/publish workflows) and Nix flake.
- `build.rs`: release builds no longer copy default configs into the user's config directory; defaults are bundled in the binary and used when a file is missing.
- Rust examples (`examples/*.rs`).

### Changed

- Renamed the project from `tgt` to `tgrs`: the crates are `tgrs` and `libtgrs`, the binary is `tgrs`, the config, data and state directories are `$XDG_*_HOME/tgrs`.
- Telegram requests (sending, typing, read marks, drafts, history, pins, search) run in the background, so a slow network no longer freezes typing or drawing.
- Only the visible messages and chats are laid out on each frame.
- The prompt wraps long lines on screen and grows with them; its height follows the text.
- The chat and topic lists show the weekday for messages of the last week and the date for older ones, instead of the full date and time.
- The sessions popup tells the key that confirms a new sign-in when one is selected.

- Message keys follow concord's: `r` reacts (was reply, now `R`), `o` opens a URL (opening media moved to `x`). The actions menu starts with copy and uses the concord-style list (selection marker, key column, modal border).
- Edited messages and reaction changes are redrawn right away.
- The title bar is concord's one-line header (` tgrs - v1.0.0  Connected as Name `) and the status bar one line of key hints or the latest status message; the pressed key and terminal size are no longer shown.
- Function keys are shown as `F1` instead of `F(1)`.
- Chats in the chat list are separated by a blank line.
- A pane always has the focus: the chat list when `tgrs` starts (the chat while the chat list is hidden). `esc` closes a popup or leaves the prompt, but no longer leaves nothing focused.
- Muting or unmuting a chat elsewhere dims or undims it in the chat list right away.
- The chat list, the topic list and the archive are padded on top and on the right as much as on the left and the bottom; the chat is padded on top like under its newest message.
- Navigation like concord: `left`/`right` move focus between the chat list, chat and prompt (`right` in the chat list no longer opens the chat, `enter` does; `left` in the chat no longer unselects), `i` in the chat focuses the prompt and `esc` there returns to the chat. `alt+arrows` resize the focused pane instead of moving focus (`resize_pane_wider`, `resize_pane_narrower`, `resize_pane_taller`, `resize_pane_shorter`, `focus_next_pane`, `focus_previous_pane` commands); the chat list can be up to half the width.
- Leader menu like concord's: `space` lists the `[leader_menu]` keys and runs the pressed one; `space` `1` hides or shows the chat list (focus moves to the chat). `space` can be used in keymaps.
- The prompt and the message search no longer inherit plain-key bindings from `[core_window]` (like `q` and `space`), and typing in the chat search no longer triggers them.
- Enter sends the message, like concord; Alt+Enter, Ctrl+J and, in terminals with the kitty keyboard protocol (now requested at start), Shift+Enter and Ctrl+Enter insert a newline. Empty messages aren't sent.
- Split into a workspace: `libtgrs` (TDLib client, store and models, no UI) and the `tgrs` binary.
- Restructured `tgrs` into `cli`, `config`, `settings`, `logger`, `app`, `credentials`, `media`, `ui` and `utils` modules with a single settings resolver (`CLI > environment > config.toml > defaults`).
- Images, videos, GIFs and stickers are previewed under their messages in the chat with the new `media-preview` feature (on by default; build with `--no-default-features` to drop it), rendered by `ratatui-image` without chafa; `image` and `ratatui-image` are optional.
- CI, dependabot, `deny.toml` and `.typos.toml` follow the [rust-template](https://github.com/Gigas002/rust-template) setup; TDLib is built and cached by a composite action.

- Default config files moved from `config/` to `examples/`.
- Migrated to Rust edition 2024 and Cargo resolver v3.
- Replaced `lazy_static` with `std::sync::LazyLock`; trimmed unused dependency features (`config`, `tokio`, `futures`, `chrono`, `rodio`, `ratatui-image`, `arboard`).
- Switched to the [Gigas002/tdlib-rs](https://github.com/Gigas002/tdlib-rs) fork (TDLib 1.8.68, system `tdjson` via `pkg-config`).

### Added

- Logging in with a QR code: answer `qr` to the phone number prompt and scan the code shown in the terminal with a phone logged in to Telegram. `libtgrs`'s `Authenticator::phone_number` is replaced by `login`, which returns a `Login`.
- The mouse works in the message actions, reaction and link pickers and the leader menu: the wheel moves the highlight, a click highlights an entry, a second click runs it and a click outside closes the menu. The wheel also moves through pinned messages and the upload explorer, and a click outside closes them.
- The leader menu highlights an entry: `up`/`down` move it and `enter` runs it. Commands `leader_menu_previous`, `leader_menu_next` and `leader_menu_confirm`.
- `telegram.use_test_dc`: use Telegram's test servers, for development.
- `notifications` feature (default): desktop notifications for new messages through TDLib's notification API, so muted chats, mention-only settings and messages already read elsewhere stay silent; the open chat is never notified about. `[notifications] enabled` and `show_preview` in `config.toml`; the text is also hidden for chats that hide previews in Telegram.
- Clearing chat history: `X` in the chat list (or the chat actions) deletes every message of a chat for you, keeping the chat, and the chat actions also clear it for everyone where Telegram allows it (private chats, groups you run); both ask for a second press. Commands `clear_chat_history` and `clear_chat_history_for_everyone`.
- Silent messages: `alt+q` in the prompt sends the next message, file, album or sticker without sound for the recipients (the prompt's title says `🔕 without sound` until it's sent, and `alt+q` again turns it off). Command `prompt_toggle_silent`.
- Privacy settings: leader `P` shows who sees your last seen time, phone number, profile photo, bio, birthday and link in forwards, who can call you, invite you to groups or find you by number, and whether others see when you read their messages. `enter` moves a setting to its next audience (everybody, my contacts, nobody), keeping its exceptions, and `a`, `n` and `r` make the open private chat's person always, never or no longer an exception. Commands `show_privacy`, `hide_privacy`, `privacy_next`, `privacy_previous`, `privacy_cycle`, `privacy_always`, `privacy_never` and `privacy_follow`; keymap section `[privacy]`.
- Typing indicators: the open chat's title shows who is typing, recording, sending a file or choosing a sticker (`typing…` in private chats, `Ferris is typing…` in groups, only the open topic's in forums), and typing in the prompt tells the chat, every 4 seconds while it goes on (not for edits). `[privacy]` in `config.toml`: `send_typing`, and `appear_online` to stay offline (your last seen time shows) while tgrs runs.
- Notification settings: leader `N` shows Telegram's notification settings, which apply on every device: notifications, message previews, mentions and pinned messages for private chats, groups and channels, and the open chat's own ones, which cycle through `default`, on and off. Commands `show_notification_settings`, `hide_notification_settings`, `notification_settings_next`, `notification_settings_previous` and `notification_settings_toggle`; keymap section `[notification_settings]`. New `[notifications]` keys: `sound` (a freedesktop sound name, never played for silent messages), `quiet_hours` (e.g. `"23:00-07:00"`, may pass midnight), `open_on_click` (clicking a notification opens its chat, default on) and `unread_in_title` (the terminal's title counts unread messages in unmuted chats, e.g. `tgrs (3)`, and is restored on exit).
- Storage and auto-download settings: leader `S` shows the disk space used by downloaded files, the database and TDLib's log, with the 10 chats that have the most files; `C` deletes every cached file and `D` the highlighted chat's (each on a second press). `[media]` in `config.toml`: `auto_download_previews` and `max_preview_size_mb` decide which chat previews load without being asked (downloaded ones always show), and `cache_max_size_mb` / `cache_max_age_days` trim the cache at startup. Commands `show_storage`, `hide_storage`, `storage_next`, `storage_previous`, `storage_clear_all` and `storage_clear_chat`; keymap section `[storage]`.
- Downloads: leader `t` lists the files downloaded (`s`) or opened (`x`) from messages, latest first, with live progress from TDLib (downloaded and total size, percent, state). `p` pauses a download, keeping its data, or resumes it, which then saves or opens the file as first asked; `x` cancels and deletes the partial file, `enter` opens a finished one and `c` clears finished ones. A paused or cancelled download is no longer reported as failed. Commands `show_downloads`, `hide_downloads`, `downloads_next`, `downloads_previous`, `downloads_toggle_pause`, `downloads_cancel`, `downloads_open` and `downloads_clear_finished`; keymap section `[downloads]`.
- Albums: several files sent at once (dropped, pasted, or attached with `a` in the upload explorer, which stays open to pick more) go as albums of documents, at most 10 each and split evenly; the first file carries the caption and the first album the reply. The upload explorer's hint counts the attached files.
- Drop or paste files into the prompt: a paste that is only paths of existing files, as terminals paste dropped files (quoted, escaped or `file://` URIs), attaches them instead of typing them, and `alt+v` attaches the image on the clipboard as a PNG (with `media-preview`). The prompt's title lists the attachments; `enter` sends them, the first with the prompt's text as its caption and as the reply when replying, and `esc` turns them back into text. Command `prompt_paste_image`.
- Captions: a file picked in the upload explorer is sent with the prompt's text as its caption (none if the prompt is empty) and as a reply when replying, and the prompt and its draft are cleared; an edit in progress stays and isn't used. `e` on your own media message edits its caption, and sending it empty removes it; stickers and round videos have none, and the message actions say so. Failed uploads and edits are reported in the status bar instead of being dropped silently.
- Forward messages: `f` on the selected message (or "forward message" in the message actions) opens a popup to pick the destination among your chats and archived chats by typing part of its name, best match first; `enter` forwards, and `tab` toggles sending a copy without the original author. Forum chats get it in their General topic; protected messages can't be forwarded and the menu says so. Commands `forward_message`, `hide_forward`, `forward_next`, `forward_previous`, `forward_toggle_copy` and `forward_confirm`; keymap section `[forward]`.
- Active sessions: leader `d` lists the devices and apps logged in to your account (app, device, place, last active); `D` logs the highlighted one out and `T` every other one, each after a second press, and `c` confirms a new sign-in as yours. tgrs's own session is left to `tgrs logout`. A new, unconfirmed sign-in is announced in the status bar with its device and place. Commands `show_sessions`, `hide_sessions`, `sessions_next`, `sessions_previous`, `sessions_terminate`, `sessions_terminate_others` and `sessions_confirm`; keymap section `[sessions]`.
- Edit your profile: leader `e` (or `e` in your own profile) edits your first and last name, bio and username in a popup; `tab`/`shift+tab` move between fields, `enter` saves only what changed and `esc` closes. A first name is required and usernames are checked before saving; Telegram's refusals (a taken username, a bio that's too long) are shown in the popup at their field. Saving from your profile shows it again. Commands `show_edit_profile`, `hide_edit_profile`, `edit_profile_next_field`, `edit_profile_previous_field` and `edit_profile_confirm`; keymap section `[edit_profile]`.
- Drafts: what's written in the prompt and not sent is kept as the draft of its chat or forum topic and synced with Telegram, so it's saved when switching chats or leaving the prompt, comes back with the chat, and follows you to other devices; a draft changed elsewhere is loaded unless edited here. Chats and topics with a draft show `Draft:` and its text in the chat and topic lists. Editing a message leaves the draft alone, and cancelling a reply keeps the text instead of clearing it.
- Reorder pinned chats: `K`/`J` (or `shift+up`/`shift+down`) in the chat list and the archive move the selected pinned chat up or down among the pinned chats of its list, and the selection goes along. Commands `move_chat_up` and `move_chat_down`.
- Blocking and reporting: `B` in the chat list and the archive blocks or unblocks the person or bot of a private chat, and leader `b` lists everyone you've blocked to unblock them (`u` or `enter`). `!` reports the selected chat, or in the chat the selected message, step by step as Telegram asks: a reason, then maybe details. Both are also in the chat and message actions menus. Commands `toggle_chat_block`, `report_chat`, `report_message`, `show_blocked_list`, `hide_blocked_list`, `blocked_list_next`, `blocked_list_previous`, `blocked_list_unblock`, `hide_report`, `report_next`, `report_previous` and `report_confirm`; keymap sections `[report]` and `[blocked_list]`.
- Open a chat by username: `n` in the chat list and the archive (leader `n` elsewhere) asks for a public username (`@name`, `name` or a `t.me/name` link) and opens the person's, bot's, group's or channel's chat, even one not in the chat list; an unknown or invalid username is reported in the popup. Commands `show_find_chat`, `hide_find_chat` and `find_chat_confirm`; keymap section `[find_chat]`.
- Profiles: `i` in the chat list and the archive (or leader `i` for the open chat) shows a chat's profile, read only: a person's or bot's name, usernames, phone number, bio and birthday, last seen and ID, or a group's or channel's usernames, description and member count; leader `m` shows your own. The bio or description is loaded from Telegram when the profile opens. Commands `show_profile`, `show_my_profile`, `hide_profile`, `profile_scroll_up` and `profile_scroll_down`; keymap section `[profile]`.
- Chat actions: `a` in the chat list and the archive (or leader `c` for the open chat) lists what can be done with a chat, and each has its own key: mute/unmute (`m`), archive/unarchive (`A`), pin/unpin (`P`), mark as read/unread (`u`) and leave a group or channel or delete any other chat (`D`, confirmed by pressing it again). Chats marked as unread show an empty unread badge. Commands `show_chat_actions`, `hide_chat_actions`, `chat_actions_next`, `chat_actions_previous`, `chat_actions_confirm`, `toggle_chat_mute`, `toggle_chat_archive`, `toggle_chat_pin`, `toggle_chat_read` and `leave_chat`; keymap section `[chat_actions]`.
- Sticker picker (`alt+s`, leader `s`): the recent stickers and the installed sets as tabs over a grid of stickers, previewed as images with `media-preview` (animated ones by their thumbnail) or as their emoji; `enter` sends the selected sticker to the open chat or topic, as a reply when replying. Commands `show_sticker_picker`, `hide_sticker_picker`, `sticker_picker_left`, `sticker_picker_right`, `sticker_picker_up`, `sticker_picker_down`, `sticker_picker_next_set`, `sticker_picker_previous_set` and `sticker_picker_send`; keymap section `[sticker_picker]`.
- Archived chats: a `📦 Archived chats` row at the top of the chat list names them and counts the unread ones; opening it shows them in a resizable pane right of the chat list, like a forum's topics, and opening a chat from the chat list closes it. Commands `focus_archive_list` (`alt+a`), `toggle_archive_list` (`alt+b`, leader `3`), `archive_list_next`, `archive_list_previous` and `archive_list_open`; keymap section `[archive_list]`.
- Groups with topics (forums): opening one shows its topics in a resizable pane right of the chat list; the chat shows the open topic's messages, and sending, uploads, search and pinned messages stay in that topic. Commands `focus_topic_list` (`alt+t`), `toggle_topic_list` (`alt+m`, leader `2`), `topic_list_next`, `topic_list_previous` and `topic_list_open`; keymap section `[topic_list]`.
- The two-step verification password is masked with `*` when logging in on the terminal (Backspace and Ctrl+U edit it, Ctrl+C quits).
- Concord-like media handling: `x` opens the media of the selected message (photo, video, GIF, audio, voice/video message, sticker or file) in the default application via `xdg-open`, `s` downloads to `app.download_dir` (default `$XDG_DOWNLOAD_DIR` or `~/Downloads`) without overwriting files. Programs and scripts are refused by `x` and can only be downloaded.
- Message actions menu (`Enter` in the chat) listing every message action with its key; unavailable actions are dimmed.
- `keychain` feature (default): the TDLib database is encrypted with a key kept in the system keychain (Secret Service); `[credentials] store = "auto" | "keychain" | "plain"`. Existing databases are encrypted on the next start; `tgrs logout` removes the key.
- Captions, videos, GIFs, video messages and stickers are shown in the chat with their duration or size.
- Subcommand `logout`; options `--config`, `--keymap`, `--theme` (paths to `config.toml`, `keymap.toml` and `theme.toml`), `--log-dir`, `--log-level` and `-v`/`-q`.
- Concord-style theme: the UI follows the terminal's colors (default foreground/background and ANSI palette, dim for secondary text). `theme.toml` overrides named Highlight Groups that inherit from each other, border shapes per surface (`[ui.border]`) and the selection marker (`[ui.indicator]`); invalid entries are skipped with a warning.
- Concord-style message layout: consecutive messages from one sender within 7 minutes share a header (name, time, `(edited)`, ✓/✓✓ for sent/read), day separators, a ` New ` divider before the first unread message, replies as a muted `↪ Name preview` line, attachments as `[photo] 1.2 MiB` labels, and the selected message framed by a rounded card instead of a highlighted background. Links, mentions and code are drawn with the `MessageLink`, `MentionOther` and `InlineCode` groups.
- Reactions: shown under their messages (yours highlighted, custom emoji as `✦`), and `r` opens a picker of the emoji the chat allows; picking one of yours removes it, `1`-`9` and `0` pick directly.
- Message actions like concord's: open URL (`o`, picked from a list when there are several), go to the replied message (`g`), pin/unpin (`P`), copy the message link (`L`). Pinned messages show 📌. Only web, e-mail, phone and `tg:` links are opened.
- `[picker]` keymap section for the reaction and URL picker.
- Concord-style panes: every pane is fully bordered with its title in the border (`Chats`, the chat's icon, name and status, `Message <chat>`), the focused one highlighted, and a hint while no chat is open. The composer shows a ` n / 4096 ` character count while typing.
- Chat list rows like concord's channel list: the `▸` selection marker, an icon per chat type (👤 person, 🤖 bot, 🔖 saved messages, 🔒 secret chat, 👥 group, 📢 channel), a green dot for online contacts, the time of the last message on the right, and under the name a `You:`/`Name:` preview with badges for pinned chats (📌), unread mentions (`@`) and the unread count; muted chats are dimmed.
- `docs/ARCHITECTURE.md`.

- `static` feature to statically link `tdjson` (use with `download-tdlib` or `local-tdlib`); no runtime `tdjson` dependency needed. Powered by **tdlib-rs v1.4.0**.
- CI: `static` feature combinations (`local-tdlib,static`, `download-tdlib,static`) in Linux, macOS, and Windows workflows.
- CI: Android cross-compilation workflow (`aarch64-linux-android`, `x86_64-linux-android`) with static linking.
- Command guide popup (`Alt+F1`) listing keybindings from `keymap.toml`.
- Theme switcher with dynamic theme discovery; theme choice can persist (see docs for release vs debug behaviour).
- Server-side chat message search (TDLib), search overlay, jump-to-message, and `Alt+C` to restore default message order; search errors surface in the status bar.
- Photo viewer for images with keyboard navigation, lazy-loaded history, layout improvements, and keybinding hints loaded from config.
- Mouse support: click-to-focus chat list, chat, and prompt; scroll in list and chat; click chat list items to open a chat (fixed row height for hit testing).
- Multi-line messages in the chat (send and display) and related editing fixes.
- Voice and audio message playback (optional build feature; see README).
- Voice/video call placeholders in the chat list so those chats do not show as empty.
- Pin indicator (📌) for pinned chats in the chat list.
- Pinned messages for the open chat: pinned strip with preview and shortcut hint, `Shift+Tab` / `BackTab` to open a pinned-messages popup (from chat or chat list), browse pins, jump to a pin with Enter, open photo/voice from the popup; `Alt+S` save-as flow for photos/documents with a Ratatui file explorer; `Alt+U` file upload explorer with Ratatui theming.
- Config and data paths aligned with XDG Base Directory (with migration/fixes for prior layout).
- GitHub Actions: Docker workflow; Windows ARM CI; TDLib **1.3.0** with updated **tdlib-rs**, Dockerfile and CI adjustments; Linux image/chafa-related workflow fixes.

### Changed

- Upgraded **tdlib-rs** to v1.4.0 (new `static` feature, `ureq` replaces `reqwest`, Android support, `#[link]` attribute removed from tdjson FFI).
- UI render-on-demand to reduce idle CPU while keeping Telegram-driven updates responsive.
- Focus tracking uses atomics instead of a mutex for lock-free reads/writes.
- Hardcoded keys moved into `keymap.toml`; keymaps merge defaults with user config and load dynamically (including photo viewer hints).
- Chat message list uses bottom-anchored layout to avoid large gaps; reply target highlighting; reply/edit rules (e.g. no editing others’ messages).
- Dependency and toolchain updates (e.g. `rodio` 0.22, `ratatui-image`, `toml`, `clap`, Docker actions, and other crates).

### Fixed

- Keys typed while the first image preview loaded could be lost, and image previews could fall back to blocks in terminals with graphics: the terminal is now asked how it draws images at start-up, before keys are read.
- Editing a message or restoring a draft put the cursor before the text instead of after it.
- Copying selected text in the prompt crashed when the clipboard refused it; copying a message now says why it failed.
- Loading older history kept asking TDLib when it returned no older messages.
- The status bar showed the chat's hints (Space menu, q quit, Enter message actions) everywhere, also in the prompt where those keys type or send; it now follows the focus, with the shortest key of each action.
- The first click in an unfocused pane only focused it: one click now opens the chat or acts on the item.
- Closing the command guide moved the focus to the chat list instead of back where it was opened.
- The leader menu lost its last entries in short terminals; it now uses two columns when it doesn't fit.
- Group chat previews read `Bob Samp…Hello` when the name was cut, and `…: He…` in a narrow list; the name now keeps its `: `, or is left out when there's no room.
- Search results ran a caption into its media label (`📷 PhotoA caption`).
- Editing, deleting or copying a message said nothing when it couldn't be done.
- The command guide's first section was called `None state (global)`, and pane toggles described themselves as `chat_list`, `archive_list` and `topic_list`.
- Online users' profiles read `Last seen  online`.
- Menus showed `Ctrl+c copy message` instead of the shorter `y`.
- Long messages were sent with line breaks where the prompt wrapped them.
- Esc didn't leave the chat search, so `q` and other keys were typed into it.
- An unfocused prompt with a multi-line draft asked for a smaller size on every frame.
- TDLib's start-up log no longer floods the terminal before login: its verbosity and log file are set before the first client is created.
- Closing the input (Ctrl+D) at a login prompt quits instead of asking again forever.
- Keys bound to capital letters (`D`, `R`, `P`, `L`, or `shift+x`) never matched: terminals report them with Shift, which is now dropped for letters.
- Text links show their text instead of the URL, and mentions of users without a username show the name instead of a user id.
- Formatting after emoji landing on the wrong characters: TDLib's UTF-16 entity offsets are converted to characters.
- Senders whose name isn't known yet are shown as `Unknown` instead of an empty header.
- Stack overflow in the prompt when wrapping at the window edge (`insert_newline` no longer recurses through `insert('\n')`).
- Occasional startup hangs and chat list refresh hangs.
- Chat search reliability and clearer failure feedback via the status bar.
- `q` incorrectly triggering quit while typing in the prompt (context-aware key handling).
- The prompt send shortcut is configurable as `prompt_send_message`, allowing terminals that reserve `Alt+Enter` to use another key.
- Chat list: scrolling, unread badge refresh when opening chats, refresh on launch, wrong unread count when a message arrives in the open chat, updates after server-side deletes, and `DeleteMessages` from cache vs real deletes.
- Duplicate last message when scrolling; chat history performance regressions; config creation/loading issues; popup and list quirks.
- Reply flow, reply styling, and copying messages with history restore.
- Photo viewer: focus return to chat on close; dynamic keymap text in the viewer.
- Docker build with newer `rodio` API; path/config bugs on the XDG rework; theme readability (`item_message_content` and post-processing).
- Tests and CI (incl. Windows `ratatui-image` picker, timeouts, `Alt+C` test on Windows).

## [1.0.0] - 2024-08-09

### Added

#### Telegram API
- project configuration for APIs
- Authentication
- Receive messages
- Send messages
- Handle updates from server
- Handle login
- Handle logout
- Handle view message
- Change user status to online and offline
- Display messages with time of arrival
- Display if messages have been read
- Edit message
- Delete message
- Copy message
- Reply message
- Handle message edited

#### Config
- Logger config
- Keybindings config
- App config
- Theme config
- Telegram config

#### CI/CD
- CI for Linux
- CI for MacOS Intel
- CI for MacOS arm64
- CI for Windows
- CD for release

### Changed

### Fixed
