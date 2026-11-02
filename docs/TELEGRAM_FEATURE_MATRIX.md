# Telegram and tgt Feature Matrix

Comparison snapshot: **2026-11-01**. This document helps define project goals; it is not a promise of full Telegram compatibility.

## Status key

- `[x]` — the basic workflow is implemented and available in the `tgt` interface.
- `[~]` — partially supported, limited, or requires an action outside `tgt`.
- `[ ]` — no user-facing workflow was found in `tgt`. **TODO**, **MAYBE**, and **NOT PLANNED** at the start of an item show its current planning direction. **PARTIAL** and **POLISH** mark improvements to existing workflows.

This matrix evaluates the `tgt` application, not TDLib or the Telegram API. `tgt` is a Linux TUI. Features tied to cameras, address books, mobile notifications, and similar platform capabilities are included as official-client features and marked accordingly. Official clients do not have one identical feature set across platforms, so this document covers the main Telegram workflows for individual users, groups/channels, and account settings. Niche and platform-specific differences can be added as project goals are defined.

## 1. Account and sign-in

### Authentication and sessions

- [x] Sign in with a phone number and Telegram code in the terminal.
- [x] Sign in with a two-step verification password; the password is masked.
- [x] Sign in with a code delivered by Telegram or SMS; TDLib handles the code.
- [x] Sign in with an email code and confirm a new email address; TDLib authentication states are supported.
- **PARTIAL** [~] Confirm a sign-in from another device; `tgt` displays a link, but confirmation must be completed on that device.
- **NOT PLANNED** [ ] Register a new account with a name. TDLib exposes the registration state, but the UI flow is not implemented.
- [x] Sign out with `tgt logout`.
- **MAYBE** [ ] Use multiple accounts and switch between them quickly.
- [x] View active sessions, terminate sessions, and receive alerts about new sign-ins. Leader `d` lists sessions; `D` logs one out and `T` all others (each pressed twice), `c` confirms a new sign-in, and unconfirmed sign-ins are announced in the status bar. Connected websites and business bots aren't listed.
- **TODO** [ ] Use passkeys or QR sign-in and manage trusted devices.

### Profile and contacts

- **PARTIAL** [~] View or edit the name, bio, username, and profile photo. Your own and other chats' profiles (name, usernames, phone, bio/description, birthday, members) can be viewed, and leader `e` (or `e` in your own profile) edits your first/last name, bio and editable username. Profile photos, birthdays and collectible usernames can't be changed.
- **NOT PLANNED** [ ] Add, import, sync, or remove contacts.
- [x] Find a user by username and open a new private chat. `n` (or leader `n`) takes `@name`, `name` or a `t.me/name` link and opens the person's, bot's, group's or channel's chat.
- **NOT PLANNED** [ ] Change the phone number, delete the account, or manage account data.
- **NOT PLANNED** [ ] Set visibility for the phone number, photo, last-seen status, calls, and forwarded messages.
- [x] Block users, view the blocked-user list, or report a user/message. `B` blocks or unblocks, leader `b` lists blocked users to unblock them, and `!` reports a chat or message through Telegram's reason/details steps.
- **MAYBE** [ ] Use chat folders and rules for including or excluding chats.

## 2. Chats and navigation

### Chat list

- [x] View private chats, groups, channels, bots, and Saved Messages, including chat type, last message, and time.
- [x] View unread messages and mark a chat read or unread.
- [x] Pin, mute, and archive chats.
- [x] View archived chats and restore a chat from the archive.
- [x] Search the local chat list.
- [x] Pin and unpin chats in the list, and reorder pinned chats (`K`/`J`) in the main list and the archive. Folders are tracked separately below.
- **MAYBE** [ ] Use folders, categories, tabs, and chat-list filters.
- **NOT PLANNED** [ ] Select multiple chats and apply bulk actions.
- **MAYBE** [ ] Create a new chat, group, or channel from the interface.
- **PARTIAL** [~] View chat/profile context, shared media, and a detailed information panel. A read-only profile popup shows a user's or group's details; shared media and a persistent information panel are not supported.

### History and chat state

- [x] Load message history and fetch older messages.
- [x] Update history when new messages arrive and synchronize through TDLib.
- [x] Show dates, times, authors, edit markers, and read status for outgoing messages.
- [x] Show the first-unread-message marker when TDLib provides it.
- [x] Jump to a replied-to message and show a short reply preview.
- [x] Show pinned-message indicators, list pinned messages, and jump to a message.
- [x] List forum topics, select a topic, and view its message history.
- [x] Keep unsent text as a per-chat (and per-topic) draft synced with Telegram, and show `Draft:` in the chat and topic lists.
- [x] Clear chat history, clear history for both sides, or delete a chat with selectable options. `X` (or the chat actions) clears the history for you, the chat actions also clear it for everyone where Telegram allows it, and `D` leaves or deletes the chat; each asks for a second press.
- **NOT PLANNED** [ ] Export history, browse shared media, or filter media by type.
- **NOT PLANNED** [ ] Use a read-later chat or custom chat sorting rules.

## 3. Messages

### Text and formatting

- [x] Send text messages, including multiline messages.
- [x] Edit your own messages.
- [x] Delete your own messages for everyone or only for yourself.
- [x] Reply to a message and jump to the replied-to message.
- [x] Copy message text.
- **MAYBE** [~] Display formatting: links, mentions, and code are highlighted; support for all Telegram entities and creating formatted text manually is limited.
- [x] Display media captions and send a file with a caption: the prompt's text captions the file picked in the upload explorer (`alt+u`), also as a reply. Captions are plain text; formatting entities aren't created.
- **PARTIAL** [~] Forward messages with author/signature options and chat selection. `f` forwards the selected message to a chat picked by name, optionally as a copy without the author; forwarding several messages at once, choosing a forum topic as the destination and removing captions are not supported.
- **MAYBE** [ ] Quote text fragments or compose hidden text, spoilers, strikethrough, underline, and other formatting.
- **PARTIAL** [~] Send silently, schedule messages, repeat on a schedule, or set reminders. `alt+q` in the prompt sends the next message, file, album or sticker without sound; scheduling, repeating and reminders are not supported.
- **MAYBE** [ ] Manage cloud drafts, queue multiple messages, and control the send queue.
- **MAYBE** [ ] Protect content or prevent forwarding for a chat or message.

### Search, reactions, and interactions

- [x] Search messages on the server in the open chat.
- **MAYBE** [~] Filter search by sender, attachment type, or date range. Basic search exists; the full official-client filter set is not confirmed.
- **MAYBE** [ ] Search messages across all chats.
- [x] React to messages with reactions allowed in the chat, remove your reaction, and view counts.
- [x] Pin or unpin messages when Telegram permissions allow it.
- [x] Copy links to messages in supergroups and channels.
- [x] Open links from messages in the system application for web, email, phone, and `tg:` links.
- **MAYBE** [~] Display mentions and hashtags; dedicated navigation and typing suggestions were not found.
- **MAYBE** [ ] Comment on channel posts through the linked discussion group.
- **MAYBE** [ ] Vote in polls and quizzes or manage polls you created.
- **MAYBE** [ ] Reply as a channel or anonymous administrator.
- **PARTIAL** [~] Edit sent media, albums, and captions. `e` edits (or removes) the caption of your own media message; replacing the media itself and albums are not supported.
- **MAYBE** [ ] Report or translate a message.

## 4. Media and files

### Receiving and sending

- [x] Select a file path and send the file as a document.
- [x] Send stickers from recent stickers and installed sets.
- [x] Download media/files to the downloads directory without overwriting existing files.
- [x] Open media with the system application (`xdg-open`); executable files and scripts are not run.
- **PARTIAL** [~] Preview images, static stickers, and thumbnails for video/GIF/video messages/files, depending on the `media-preview` build feature; full playback uses an external application.
- **PARTIAL** [~] Show voice/video messages and audio as attachments that can be opened in an external player; recording and playback inside the client are unavailable.
- **NOT PLANNED** [ ] Use the camera or gallery, or record a voice/video message.
- **NOT PLANNED** [ ] Send photos/videos with an editor for cropping, drawing, compression, and captions.
- **PARTIAL** [~] Send media groups/albums, search GIFs, or search media inline. Several files (dropped, pasted or attached with `a` in the upload explorer) are sent as albums of documents, up to 10 each; photo/video albums (compressed), GIF search and inline media search are not supported.
- [x] Drag files with the mouse or paste images from the clipboard. Files dropped on the terminal (pasted as paths) are attached to the prompt, and `alt+v` attaches the clipboard image (with `media-preview`); it depends on the terminal pasting dropped files' paths.
- **PARTIAL** [~] Pause, resume, cancel, or prioritize downloads; use a unified download manager. Leader `t` lists downloads with progress; `p` pauses/resumes, `x` cancels and `enter` opens. Priorities, uploads and keeping the list across restarts are not supported.
- **PARTIAL** [~] Configure auto-downloads, traffic limits, photo/video quality, and cache cleanup. `[media]` turns automatic chat previews off or limits their size and trims the cache at startup by size and age; leader `S` shows the storage used per chat and clears the whole cache or one chat's files. Traffic limits/network statistics and photo/video quality aren't supported (TDLib offers no upload quality setting for documents).
- **NOT PLANNED** [ ] Stream video/audio or use a built-in document viewer.

### Stickers, GIFs, and emoji

- [x] Use recent stickers and installed sets in the picker.
- **PARTIAL** [~] Display stickers as static images/thumbnails; animated and video stickers do not play.
- **PARTIAL** [~] Use reactions allowed by the chat; there is no separate emoji panel or search.
- **MAYBE** [ ] Search for and install sticker sets, manage favorites, and manage sets.
- **MAYBE** [ ] Use animated/custom emoji, search GIFs, or create GIFs.
- **NOT PLANNED** [ ] Create or edit stickers, masks, emoji packs, and sets in the app.

## 5. Groups, forums, and channels

### Participation and administration

- [x] Read and send messages in accessible groups and channels.
- [x] Read and send messages in an open forum topic.
- [x] Show unread status, mentions, and pinned messages when present in the available data.
- **MAYBE** [ ] Create groups/channels, invite members, and manage invite links.
- **MAYBE** [ ] View members, roles, admins, and permissions; remove or restrict members.
- **MAYBE** [ ] Configure group permissions, slow mode, anti-spam, and new-member approval.
- **MAYBE** [ ] Change a group/channel description, avatar, visibility, address, or type.
- **MAYBE** [ ] Manage owners, administrators, and the action log.
- **MAYBE** [ ] Post as a channel, add an admin signature, and schedule posts.
- **MAYBE** [ ] View group/channel statistics, reactions, and views.

### Topics and discussions

- [x] View forum topics and switch between them.
- **PARTIAL** [~] Display topic messages and notification indicators; topic management is limited.
- **MAYBE** [ ] Create, rename, close, delete, or pin topics in the UI.
- **MAYBE** [ ] Configure notifications for each topic.
- **MAYBE** [ ] View channel-post discussions and comments, and navigate between a channel and its discussion group.

## 6. Notifications and presence

- [x] Show desktop notifications for new messages when the `notifications` feature is enabled.
- [x] Respect server-side chat notification settings, including mute and hidden previews.
- [x] Suppress notifications for the currently active chat. `[notifications]` adds a sound, quiet hours, click-to-open and the unread count in the terminal's title.
- **PARTIAL** [~] Configure sounds, per-user/group/channel settings, and a do-not-disturb schedule. Leader `N` changes Telegram's settings for private chats, groups, channels and the open chat (notifications, previews, mentions, pinned messages); `quiet_hours` is a daily do-not-disturb span and `sound` a freedesktop sound for every notification. Telegram's own notification sounds, per-chat sounds, timed mutes from the popup and story notifications are not supported.
- [x] Control previews, badge/count, and notification actions. Previews per scope or chat (`N`) and for every notification (`show_preview`); `unread_in_title` puts the unread count in the terminal's title; clicking a notification opens its chat (`open_on_click`). Reply or mark-as-read buttons in notifications are not supported.
- [x] Show a contact's online/last-seen status when Telegram provides it.
- **PARTIAL** [~] Set your status, last-seen visibility, privacy exceptions, and read/typing indicators. Leader `P` sets who sees your last seen time, phone number, photo, bio, birthday and forward link, and who can call, invite or find you, with the open private chat's person as an exception (exceptions from other apps are kept), and hides your read time; others' typing, recording and uploads show in the chat title, and tgt sends typing (`send_typing`) and can stay offline (`appear_online`). Emoji statuses, group exceptions and picking people outside the open chat are not supported.

## 7. Calls and live streams

- **NOT PLANNED** [ ] Make private voice and video calls.
- **NOT PLANNED** [ ] Make group calls, share a screen, manage participants, or use in-call reactions.
- **NOT PLANNED** [ ] Join voice/video chats in groups.
- **NOT PLANNED** [ ] Stream live in channels, connect OBS/RTMP, or manage a stream.
- **NOT PLANNED** [ ] Record calls/streams or access recordings.

## 8. Stories, bots, Mini Apps, and commerce

### Stories and live content

- **NOT PLANNED** [ ] View, publish, and manage Stories.
- **NOT PLANNED** [ ] Manage Story reactions, replies, audience, privacy, expiration, and archive.
- **NOT PLANNED** [ ] Use group/channel Stories and live broadcasts in Stories.

### Bots and Mini Apps

- **POLISH** [~] Read and send bot messages like regular messages; bot buttons and keyboards are not presented as a fully interactive interface.
- **MAYBE** [ ] Use bot commands, callback buttons, bot menus, and inline mode.
- **NOT PLANNED** [ ] Launch Mini Apps/Web Apps and pass user context to them.
- **NOT PLANNED** [ ] Use in-app payments, confirm purchases, and manage subscriptions.

### Premium, Stars, gifts, and business

- **NOT PLANNED** [ ] Subscribe to Premium, manage a subscription, and use Premium features.
- **NOT PLANNED** [ ] Use increased limits, large-file uploads, transcription, exclusive reactions/stickers, and a Premium profile.
- **NOT PLANNED** [ ] Use Telegram Stars for balances, gifts, paid media/messages, and purchases.
- **NOT PLANNED** [ ] Use gifts, collectible gifts, auctions, and achievement/rating displays.
- **NOT PLANNED** [ ] Use a business profile with hours, address, quick replies, greeting/away messages, and connected bots.
- **NOT PLANNED** [ ] Use Suggested Posts, paid/advertising posts, and channel monetization tools.

## 9. Client settings and platform features

- [x] Configure keys, theme, directories, notifications, and keychain through TOML; configuration changes are made manually.
- [x] Customize key bindings, view command help, use the leader menu, and resize panes.
- [x] Use the terminal theme and custom highlight groups.
- [x] Encrypt the local TDLib database with the system keychain when the `keychain` feature is enabled.
- **MAYBE** [~] Use proxies through TDLib/client parameters; no interactive proxy management in the UI was found.
- **MAYBE** [ ] Use a settings screen, change the interface language, and manage localization.
- **MAYBE** [ ] Change the theme in the UI or configure fonts, scaling, colors, animation, and layout.
- **MAYBE** [ ] Lock the client locally with a password/biometrics and configure auto-lock.
- **TODO / PARTIAL** [ ] Mouse-driven UI and adaptive GUI layout. Keyboard shortcuts, command search, and custom menus are available for some navigation; a mouse-based GUI is outside the TUI scope.
- **NOT PLANNED** [ ] Sync contacts with the system address book.
- **NOT PLANNED** [ ] Run in the background as a system tray app, launch at startup, or integrate with mobile OS notifications.
- **NOT PLANNED** [ ] Use Android/iOS/Desktop/macOS features that rely on a camera, microphone, system share sheet, widgets, or mobile permissions.

## Summary

`tgt` already covers the core cloud-messenger workflow in a terminal: sign-in, chat list, reading and sending text/files, history, replies, editing/deleting, search, reactions, pins, forum topics, archive, stickers, downloads, and desktop notifications. Profiles can be viewed but not edited. Major gaps include contacts and profile editing, account/community settings, richer message composition and forwarding, polls, calls, Stories, bot buttons/Mini Apps, and Telegram commerce. Some items describe Telegram-wide features that do not naturally fit a terminal client and would need a product decision before implementation.

## Sources and implementation review

Official Telegram references:

- [Telegram FAQ](https://telegram.org/faq) — chats, groups, channels, security, calls, bots, and accounts.
- [Telegram Premium FAQ](https://telegram.org/faq_premium/) — subscription features.
- [Telegram Business API](https://core.telegram.org/api/business) — business profiles and automation.
- [Telegram update history](https://telegram.org/evolution) — Stories and newer features.
- [Telegram Bot Platform](https://core.telegram.org/bots) — bots and Mini Apps.

The `tgt` implementation was reviewed against `README.md`, `docs/USAGE.md`, `CHANGELOG.md`, `docs/ARCHITECTURE.md`, public operations in `libtgt/src/client/mod.rs`, UI modules in `tgt/src/ui/`, and TDLib update handling. `[ ]` means no user-facing workflow was found in the current repository, even if a corresponding method may exist in TDLib.
