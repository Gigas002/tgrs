//! Application orchestration: runs the chosen command and the TUI event loop.

pub mod action;
pub mod auth;
pub mod commands;
pub mod context;
pub mod error;
pub mod event;
pub mod themes;

#[cfg(test)]
mod tests;

use std::{
    collections::HashMap,
    io,
    sync::Arc,
    time::{Duration, Instant},
};

use libtgt::{
    client::{Client, Notification},
    store::Store,
    tdlib_rs::enums::ChatList,
};
use ratatui::layout::Rect;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::{
    app::{
        action::Action, auth::StdinAuthenticator, context::AppContext, error::AppError,
        event::Event,
    },
    settings::{ActionBinding, Command, Settings},
    ui::{Tui, terminal::TuiBackend},
};

/// Messages per history request.
const HISTORY_BATCH: i32 = 50;
/// Chats fetched at startup so the list reaches its latest state quickly.
const INITIAL_CHATS: i32 = 200;
/// Redraw tick (~60 FPS).
const REFRESH_MS: u64 = 16;
/// How long `tgt send` waits for Telegram to acknowledge the message.
const SEND_ACK_TIMEOUT: Duration = Duration::from_secs(10);

/// Runs the command selected on the command line.
pub fn run(settings: &Settings) -> Result<(), AppError> {
    match &settings.command {
        Command::InitConfig { force } => commands::init_config(&settings.dirs, *force),
        Command::Clear {
            config,
            data,
            state,
            yes,
        } => commands::clear(&settings.dirs, *config, *data, *state, *yes),
        Command::Tui | Command::Logout | Command::Send { .. } => {
            tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?
                .block_on(run_telegram(settings))
        }
    }
}

/// Logs in, then runs the command that needs Telegram.
async fn run_telegram(settings: &Settings) -> Result<(), AppError> {
    let store = Arc::new(Store::default());
    let mut client = Client::new(Arc::clone(&store), settings.telegram.clone());
    let (notify_tx, notify_rx) = tokio::sync::mpsc::unbounded_channel();
    client.start(notify_tx);
    client.set_logging(&settings.tdlib_log).await;
    client.authorize(&mut StdinAuthenticator).await;

    if client.is_authorized() {
        client.use_quick_ack().await;
        client.get_me().await;
        let result = match &settings.command {
            Command::Logout => {
                client.log_out().await;
                Ok(())
            }
            Command::Send { chat, message } => send(&mut client, chat, message).await,
            _ => {
                client.load_chats(ChatList::Main, INITIAL_CHATS).await;
                let app_context = Arc::new(AppContext::new(settings, store));
                run_tui(app_context, &mut client, notify_rx).await
            }
        };
        if client.is_authorized() {
            client.close().await;
        }
        // Drain the authorization updates until TDLib reports `Closed`.
        client.authorize(&mut StdinAuthenticator).await;
        client.join().await;
        result
    } else {
        Err(AppError::Command("not authorized".into()))
    }
}

/// `tgt send`: sends one message to the only chat matching `chat_name`.
async fn send(client: &mut Client, chat_name: &str, text: &str) -> Result<(), AppError> {
    client.load_all_chats().await;
    let chats = client
        .search_chats(chat_name)
        .await
        .map_err(|e| AppError::Command(format!("failed to search chats: {}", e.message)))?;
    let chat_id = match chats.chat_ids.as_slice() {
        [] => return Err(AppError::Command(format!("no chat named {chat_name:?}"))),
        [chat_id] => *chat_id,
        _ => {
            return Err(AppError::Command(format!(
                "{} chats match {chat_name:?}",
                chats.chat_ids.len()
            )));
        }
    };

    let message = client
        .send_message(text.to_owned(), chat_id, None)
        .await
        .map_err(|e| AppError::Command(format!("failed to send message: {}", e.message)))?;

    // The acknowledgement may be delayed or never arrive.
    let deadline = Instant::now() + SEND_ACK_TIMEOUT;
    while client.store().last_acknowledged_message_id() != message.id {
        if Instant::now() >= deadline {
            tracing::warn!(message.id, "timed out waiting for MessageSendAcknowledged");
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    tracing::info!("sent a message to {chat_name} ({chat_id})");
    Ok(())
}

/// The TUI event loop.
async fn run_tui(
    app_context: Arc<AppContext>,
    client: &mut Client,
    mut notify_rx: UnboundedReceiver<Notification>,
) -> Result<(), AppError> {
    tracing::info!("starting the TUI");
    let mut tui = Tui::new(Arc::clone(&app_context));
    let mut tui_backend = TuiBackend::new(Arc::clone(&app_context))?;

    client.online().await;
    client.disable_animated_emoji(true).await;

    tui_backend.enter()?;
    tui.register_action_handler(app_context.action_tx().clone())?;
    app_context.mark_dirty();

    // The playback thread signals position updates so the status bar redraws immediately.
    let (voice_wake_tx, mut voice_wake_rx) = tokio::sync::mpsc::unbounded_channel::<()>();
    #[cfg(feature = "audio")]
    if let Some(tx) =
        crate::audio::spawn_playback_thread(app_context.action_tx().clone(), voice_wake_tx)
    {
        app_context.set_voice_playback_tx(tx);
    }
    #[cfg(not(feature = "audio"))]
    drop(voice_wake_tx);

    // The chat list only rebuilds on LoadChats/ChatHistoryAppended/Resize.
    app_context
        .action_tx()
        .send(Action::LoadChats(ChatList::Main.into(), INITIAL_CHATS))?;

    let (wake_tx, mut wake_rx) = tokio::sync::mpsc::unbounded_channel::<()>();
    let refresh_tx = app_context.action_tx().clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(REFRESH_MS));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if refresh_tx.send(Action::Refresh).is_err() {
                break;
            }
            let _ = wake_tx.send(());
        }
    });

    // One blocking wait (tick | notification | voice), then drain every queue.
    let result = loop {
        let mut notification = None;
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(REFRESH_MS)) => {}
            _ = wake_rx.recv() => {}
            n = notify_rx.recv() => notification = n,
            _ = voice_wake_rx.recv() => {}
        }
        let step = async {
            while let Some(ev) = tui_backend.try_next() {
                handle_tui_backend_one_event(
                    Arc::clone(&app_context),
                    &mut tui,
                    &mut tui_backend,
                    ev,
                )
                .await?;
            }
            for n in notification
                .into_iter()
                .chain(std::iter::from_fn(|| notify_rx.try_recv().ok()))
            {
                handle_notification(&app_context, n)?;
            }
            handle_app_actions(Arc::clone(&app_context), &mut tui, &mut tui_backend, client).await
        };
        if let Err(err) = step.await {
            break Err(err);
        }
        if app_context.quit_acquire() || !client.is_authorized() {
            break Ok(());
        }
    };

    client.offline().await;
    tui_backend.exit();
    tracing::info!("quitting");
    // Clear the terminal and move the cursor to the top left corner.
    let _ = io::Write::write_all(&mut io::stdout().lock(), b"\x1b[2J\x1b[1;1H");
    result
}

/// Turns a store change into the actions the UI reacts to.
fn handle_notification(
    app_context: &AppContext,
    notification: Notification,
) -> Result<(), AppError> {
    let action_tx = app_context.action_tx();
    match notification {
        Notification::ChatListChanged => action_tx.send(Action::ChatHistoryAppended)?,
        Notification::OpenChatMessageAdded(message_id) => {
            app_context
                .store()
                .set_jump_target_message_id_i64(message_id);
            action_tx.send(Action::ChatHistoryAppended)?;
            action_tx.send(Action::ViewAllMessages)?;
        }
        Notification::OpenChatPinsChanged => action_tx.send(Action::LoadPinnedMessages)?,
        Notification::OpenChatHistoryInvalidated => action_tx.send(Action::GetChatHistory)?,
    }
    Ok(())
}

#[allow(clippy::await_holding_lock)]
/// Handle a single TUI backend event.
async fn handle_tui_backend_one_event(
    app_context: Arc<AppContext>,
    tui: &mut Tui,
    tui_backend: &mut TuiBackend,
    event: Event,
) -> Result<(), AppError> {
    match event {
        Event::Render => app_context.mark_dirty(),
        Event::Resize(width, height) => {
            app_context.mark_dirty();
            app_context
                .action_tx()
                .send(Action::Resize(width, height))?;
        }
        Event::Key(key, modifiers) => {
            let focused = app_context.focused_component();
            let keymap_config = app_context.keymap_config();
            let key_event = Event::Key(key, modifiers);

            // Merged map (core + component): component map includes core_window + mode-specific keys.
            let should_check_keymap =
                focused.is_none() || keymap_config.get_map_of(focused).contains_key(&key_event);

            if should_check_keymap {
                let keymap = keymap_config.get_map_of(focused);
                if let Some(action_binding) = keymap.get(&key_event) {
                    match action_binding {
                        ActionBinding::Single { action, .. } => {
                            app_context.action_tx().send(action.clone())?;
                            return Ok(());
                        }
                        ActionBinding::Multiple(map_event_action) => {
                            consume_until_single_action(
                                app_context.action_tx(),
                                tui_backend,
                                map_event_action.clone(),
                            )
                            .await;
                            return Ok(());
                        }
                    }
                }
            }
            app_context
                .action_tx()
                .send(Action::from_key_event(key, modifiers))?;
        }
        Event::FocusLost => app_context.action_tx().send(Action::FocusLost)?,
        Event::FocusGained => app_context.action_tx().send(Action::FocusGained)?,
        Event::Paste(ref text) => app_context.action_tx().send(Action::Paste(text.clone()))?,
        _ => {}
    }

    if let Some(action) = tui.handle_events(Some(event.clone()))? {
        app_context.action_tx().send(action)?
    }
    Ok(())
}

#[allow(clippy::await_holding_lock)]
/// Consume events until a single action is produced.
/// This function is used to consume events until a single action is produced
/// from a map of events to actions.
/// This is useful for handling multiple key bindings that produce the same
/// action, or simply to consume events until a single action is produced.
/// The time limit for consuming events is 1 second.
///
/// # Arguments
/// * `action_tx` - An unbounded sender that can send actions.
/// * `tui_backend` - A mutable reference to the `TuiBackend` struct.
/// * `map_event_action` - A map of events to actions.
async fn consume_until_single_action(
    action_tx: &UnboundedSender<Action>,
    tui_backend: &mut TuiBackend,
    map_event_action: HashMap<Event, ActionBinding>,
) {
    let start = Instant::now();
    loop {
        if let Some(event) = tui_backend.next().await
            && let Some(ActionBinding::Single { action, .. }) = map_event_action.get(&event)
        {
            action_tx.send(action.clone()).unwrap();
            break;
        }
        if start.elapsed().as_secs() > 1 {
            break;
        }
    }
}

/// TDLib can emit long bursts of chat-list-related updates. The main loop would otherwise drain
/// thousands of [`Action::ChatHistoryAppended`] / [`Action::Refresh`] in one turn, each forcing
/// chat list work. Compress any run consisting only of those two variants to at most one refresh
/// tick plus one list sync (order preserved relative to other actions).
fn fold_chat_list_refresh_actions(actions: Vec<Action>) -> Vec<Action> {
    if actions.is_empty() {
        return actions;
    }
    let mut out = Vec::with_capacity(actions.len());
    let mut i = 0;
    while i < actions.len() {
        if matches!(actions[i], Action::ChatHistoryAppended | Action::Refresh) {
            let mut saw_refresh = false;
            let mut saw_cha = false;
            while i < actions.len()
                && matches!(actions[i], Action::ChatHistoryAppended | Action::Refresh)
            {
                match &actions[i] {
                    Action::Refresh => saw_refresh = true,
                    Action::ChatHistoryAppended => saw_cha = true,
                    _ => {}
                }
                i += 1;
            }
            if saw_refresh {
                out.push(Action::Refresh);
            }
            if saw_cha {
                out.push(Action::ChatHistoryAppended);
            }
        } else {
            out.push(actions[i].clone());
            i += 1;
        }
    }
    out
}

/// Returns true for actions that change UI-visible state and should trigger a render.
fn action_changes_ui(action: &Action) -> bool {
    matches!(
        action,
        Action::FocusComponent(_)
            | Action::ChatListNext
            | Action::ChatListPrevious
            | Action::ChatListSearch
            | Action::ChatListOpen
            | Action::ChatListSortWithString(_)
            | Action::ChatListRestoreSort
            | Action::ChatWindowNext
            | Action::ChatWindowPrevious
            | Action::ChatWindowSearch
            | Action::ChatWindowSortWithString(_)
            | Action::ChatWindowRestoreSort
            | Action::SearchChatMessages(_)
            | Action::SearchResults(_)
            | Action::JumpToMessage(_)
            | Action::JumpCompleted(_)
            | Action::CloseSearchOverlay
            | Action::ShowSearchOverlay
            | Action::SearchOverlaySubmit
            | Action::SearchOverlayConfirm
            | Action::SwitchTheme
            | Action::SwitchThemeTo(_)
            | Action::ShowThemeSelector
            | Action::HideThemeSelector
            | Action::ShowPhotoViewer
            | Action::HidePhotoViewer
            | Action::ShowFileUploadExplorer
            | Action::HideFileUploadExplorer
            | Action::StartFileDownload
            | Action::ShowFileDownloadExplorer(_)
            | Action::HideFileDownloadExplorer
            | Action::SaveChatFileAs { .. }
            | Action::ViewPhotoMessage(_)
            | Action::PhotoDownloaded(_)
            | Action::PhotoDecoded(_)
            | Action::ToggleVoicePlayback
            | Action::VoicePlaybackStarted(_)
            | Action::VoicePlaybackPosition(_, _, _)
            | Action::VoicePlaybackEnded(_)
            | Action::LoadPinnedMessages
            | Action::ShowPinnedMessagesPopup
            | Action::PinnedPopupConfirmJump
            | Action::PinnedPopupViewPhoto
            | Action::PinnedPopupPlayVoice
            | Action::HidePinnedMessagesPopup
            | Action::PinnedPopupPrevious
            | Action::PinnedPopupNext
            | Action::Refresh
            | Action::ToggleChatList
            | Action::IncreaseChatListSize
            | Action::DecreaseChatListSize
            | Action::IncreasePromptSize
            | Action::DecreasePromptSize
            | Action::ShowChatWindowReply
            | Action::HideChatWindowReply
            | Action::EditMessage(_, _)
            | Action::ReplyMessage(_, _)
            | Action::ShowCommandGuide
            | Action::HideCommandGuide
            | Action::UpdateArea(_)
            | Action::GetChatHistoryNewer
            | Action::ChatHistoryAppended
    )
}

/// Handle incoming actions from the application.
///
/// Bursts of [`Action::ChatHistoryAppended`] / [`Action::Refresh`] are folded before handling
/// ([`fold_chat_list_refresh_actions`]) so one TDLib flurry does not run thousands of chat-list
/// rebuilds in a single turn. [`libtgt::store::Store`] mutexes are not held across
/// `.await` in the [`Action::GetChatHistory`] path (loads use discrete lock regions per batch).
#[allow(clippy::await_holding_lock)]
///
/// # Arguments
/// * `app_context` - An Arc wrapped AppContext struct.
/// * `tui` - A mutable reference to the Tui struct.
/// * `tui_backend` - A mutable reference to the TuiBackend struct.
/// * `client` - The Telegram client.
///
/// # Returns
/// * `Result<(), AppError>` - An Ok result or an error.
async fn handle_app_actions(
    app_context: Arc<AppContext>,
    tui: &mut Tui,
    tui_backend: &mut TuiBackend,
    client: &mut Client,
) -> Result<(), AppError> {
    let mut batch = Vec::new();
    while let Ok(a) = app_context.action_rx().try_recv() {
        batch.push(a);
    }
    if batch.is_empty() {
        return Ok(());
    }

    let raw_len = batch.len();
    let folded = fold_chat_list_refresh_actions(batch);
    let folded_len = folded.len();
    if raw_len > 64 {
        tracing::debug!(
            target: "tgt::actions",
            raw = raw_len,
            folded = folded_len,
            "folded action batch (TDLib/UI burst)"
        );
    }

    for action in folded {
        match &action {
            Action::Render => {
                // Actual draw happens at end of loop when should_render()
            }
            Action::Refresh => {
                app_context.mark_dirty();
            }
            Action::Resize(width, height) => {
                tui_backend
                    .terminal
                    .resize(Rect::new(0, 0, *width, *height))?;
                app_context.mark_dirty();
            }
            Action::FocusLost => {
                tui_backend.suspend()?;
                app_context.mark_dirty();
            }
            Action::FocusGained => {
                tui_backend.resume()?;
                app_context.mark_dirty();
            }
            Action::Quit => {
                app_context.quit_store(true);
            }
            Action::LoadChats(chat_list, limit) => {
                app_context.mark_dirty();
                client.load_chats((*chat_list).into(), *limit).await;
            }
            Action::SendMessage(message, reply_to) => {
                let _ = client
                    .send_message(
                        message.to_string(),
                        app_context.store().open_chat_id().as_i64(),
                        reply_to.clone(),
                    )
                    .await;
                app_context.action_tx().send(Action::HideChatWindowReply)?;
            }
            Action::SendMessageEdited(message_id, message) => {
                client.edit_message(*message_id, message.to_string()).await;
            }
            Action::UploadFile(file_path) => {
                let chat_id = app_context.store().open_chat_id().as_i64();
                if chat_id == 0 {
                    let _ = app_context.action_tx().send(Action::StatusMessage(
                        "Open a chat before uploading a file.".into(),
                    ));
                    continue;
                }
                let _ = client.send_document(file_path.to_string(), chat_id).await;
            }
            Action::SaveChatFileAs {
                message_id,
                dest_path,
            } => {
                let Some(message) = app_context.store().get_message(*message_id) else {
                    let _ = app_context
                        .action_tx()
                        .send(Action::StatusMessage("Message not found.".into()));
                    continue;
                };
                let Some((file_id, _)) = message.save_as_candidate() else {
                    let _ = app_context.action_tx().send(Action::StatusMessage(
                        "This message has no file to save.".into(),
                    ));
                    continue;
                };

                let src_path = if let Some(p) = message.local_telegram_file_path() {
                    p
                } else {
                    match client.download_file(file_id, 32).await {
                        Ok(p) => p,
                        Err(e) => {
                            tracing::error!("Save as: download failed: {:?}", e);
                            let _ = app_context.action_tx().send(Action::StatusMessage(
                                "Failed to download file from Telegram.".into(),
                            ));
                            continue;
                        }
                    }
                };

                match std::fs::copy(&src_path, dest_path) {
                    Ok(n) => {
                        let _ = app_context.action_tx().send(Action::StatusMessage(format!(
                            "Saved {n} bytes to {dest_path}"
                        )));
                    }
                    Err(e) => {
                        tracing::error!("Save as: copy failed: {:?}", e);
                        let _ = app_context
                            .action_tx()
                            .send(Action::StatusMessage(format!("Save failed: {e}")));
                    }
                }
            }
            Action::GetChatHistory if !app_context.store().is_history_loading() => {
                app_context.store().set_history_loading(true);
                let chat_id = app_context.store().open_chat_id().as_i64();
                let cache_len_before = app_context.store().open_chat_messages().len();

                // An empty cache (initial load) gets two batches to fill the window,
                // incremental scrolling one.
                let target_load = if cache_len_before == 0 {
                    2 * HISTORY_BATCH as usize
                } else {
                    HISTORY_BATCH as usize
                };
                let mut loaded_count = 0;

                while loaded_count < target_load {
                    if app_context.store().open_chat_id().as_i64() != chat_id {
                        break;
                    }

                    // Always use the oldest message in cache (or 0 if empty) to ensure continuity
                    let from_message_id = app_context.store().oldest_message_id().unwrap_or(0);
                    let (entries, _has_more) = client
                        .get_chat_history_batch(chat_id, from_message_id, 0, HISTORY_BATCH)
                        .await;

                    if entries.is_empty() {
                        break;
                    }

                    if app_context.store().open_chat_id().as_i64() != chat_id {
                        break;
                    }

                    loaded_count += entries.len();
                    app_context
                        .store()
                        .open_chat_messages()
                        .insert_messages(entries);
                }

                app_context.store().set_history_loading(false);
                let _ = app_context.action_tx().send(Action::ChatHistoryAppended);
            }
            Action::LoadPinnedMessages => {
                let chat_id = app_context.store().open_chat_id().as_i64();
                if chat_id == 0 {
                    continue;
                }
                let pins = client.fetch_all_pinned_messages(chat_id).await;
                if app_context.store().open_chat_id().as_i64() == chat_id {
                    app_context.store().set_open_chat_pinned(pins);
                }
            }
            Action::GetChatHistoryNewer if !app_context.store().is_history_loading() => {
                let chat_id = app_context.store().open_chat_id().as_i64();
                let newest = app_context.store().open_chat_messages().newest_message_id();
                if let Some(from_id) = newest {
                    app_context.store().set_history_loading(true);
                    // TDLib: offset -N = from_message_id + N newer messages; limit >= -offset
                    // Use offset -49 to get from_id (which we already have) + 49 newer, then filter out from_id
                    let (entries, _) = client
                        .get_chat_history_batch(
                            chat_id,
                            from_id,
                            -(HISTORY_BATCH - 1),
                            HISTORY_BATCH,
                        )
                        .await;
                    if app_context.store().open_chat_id().as_i64() == chat_id && !entries.is_empty()
                    {
                        // Skip the first message if it's the boundary message we already have
                        let new_messages = if entries.first().map(|e| e.id()) == Some(from_id) {
                            &entries[1..]
                        } else {
                            &entries[..]
                        };
                        if !new_messages.is_empty() {
                            app_context
                                .store()
                                .open_chat_messages()
                                .insert_messages(new_messages.iter().cloned());
                        }
                    }
                    app_context.store().set_history_loading(false);
                    let _ = app_context.action_tx().send(Action::ChatHistoryAppended);
                }
            }
            Action::SearchChatMessages(query) => {
                let chat_id = app_context.store().open_chat_id().as_i64();
                if chat_id == 0 {
                    let _ = app_context.action_tx().send(Action::StatusMessage(
                        "Open a chat before searching messages.".into(),
                    ));
                    continue;
                }
                match client
                    .search_chat_messages(chat_id, query.clone(), HISTORY_BATCH)
                    .await
                {
                    Ok(entries) => {
                        let _ = app_context.action_tx().send(Action::StatusMessage(format!(
                            "Search results: {}",
                            entries.len()
                        )));
                        let _ = app_context.action_tx().send(Action::SearchResults(entries));
                    }
                    Err(e) => {
                        let _ = app_context.action_tx().send(Action::StatusMessage(format!(
                            "Search failed: {}",
                            e.message
                        )));
                        let _ = app_context.action_tx().send(Action::SearchResults(vec![]));
                    }
                }
            }
            Action::JumpToMessage(message_id) => {
                let chat_id = app_context.store().open_chat_id().as_i64();
                if chat_id == 0 {
                    continue;
                }
                // If message already loaded, just select it; don't clear (avoids wiping chat).
                if app_context
                    .store()
                    .open_chat_messages()
                    .get_message(*message_id)
                    .is_some()
                {
                    app_context
                        .store()
                        .set_jump_target_message_id_i64(*message_id);
                    let _ = app_context
                        .action_tx()
                        .send(Action::JumpCompleted(*message_id));
                    let _ = app_context.action_tx().send(Action::ChatHistoryAppended);
                    continue;
                }

                // Clear chat history BEFORE loading new messages to ensure clean slate
                app_context.store().clear_open_chat_messages();
                app_context.store().set_history_loading(true);

                // Load symmetric window around target message
                // TDLib limitation: offset must be > -100, so we can't load 250 newer in one call
                // Strategy: Load target + older messages, then load newer in multiple batches
                const OLDER_LIMIT: i32 = 100; // target + 99 older messages
                const NEWER_BATCH_SIZE: i32 = 99; // max we can use with offset -98
                const NEWER_BATCHES: usize = 3; // 3 batches of ~99 = ~297 newer messages

                // Get target + 99 older messages (offset 0 = include from_message_id)
                let (mut all_entries, _) = client
                    .get_chat_history_batch(chat_id, *message_id, 0, OLDER_LIMIT)
                    .await;

                // Load newer messages in multiple batches (TDLib offset limit is -100)
                let mut current_from_id = *message_id;
                for batch_num in 0..NEWER_BATCHES {
                    let (entries_newer, _) = client
                        .get_chat_history_batch(
                            chat_id,
                            current_from_id,
                            -(NEWER_BATCH_SIZE - 1),
                            NEWER_BATCH_SIZE,
                        )
                        .await;

                    if entries_newer.is_empty() {
                        break;
                    }

                    // Filter out the boundary message (already have it from previous batch or older)
                    let new_msgs: Vec<_> = entries_newer
                        .into_iter()
                        .filter(|e| e.id() != current_from_id)
                        .collect();

                    if new_msgs.is_empty() {
                        break;
                    }

                    // Update boundary for next batch
                    if let Some(last) = new_msgs.last() {
                        current_from_id = last.id();
                    }

                    all_entries.extend(new_msgs);

                    tracing::debug!(
                        batch_num,
                        loaded = all_entries.len(),
                        "JumpToMessage: loaded newer batch"
                    );
                }

                tracing::debug!(
                    target_id = message_id,
                    total_count = all_entries.len(),
                    first = all_entries.first().map(|e| e.id()),
                    last = all_entries.last().map(|e| e.id()),
                    "JumpToMessage: loaded all batches"
                );

                if !all_entries.is_empty() {
                    // Insert messages into the fresh (already cleared) cache
                    app_context
                        .store()
                        .open_chat_messages()
                        .insert_messages(all_entries);

                    let final_oldest = app_context.store().oldest_message_id();
                    let final_newest = app_context.store().newest_message_id();
                    let has_target = app_context.store().get_message(*message_id).is_some();
                    tracing::debug!(
                        target_id = message_id,
                        has_target,
                        final_oldest,
                        final_newest,
                        total_cached = app_context.store().open_chat_messages().len(),
                        "JumpToMessage: cache after insert"
                    );
                    // Only set jump target AFTER messages are inserted, so the message is in cache
                    app_context
                        .store()
                        .set_jump_target_message_id_i64(*message_id);
                } else {
                    tracing::warn!(
                        target_id = message_id,
                        "JumpToMessage: failed to load any messages, cache is empty"
                    );
                }
                app_context.store().set_history_loading(false);
                let _ = app_context
                    .action_tx()
                    .send(Action::JumpCompleted(*message_id));
                let _ = app_context.action_tx().send(Action::ChatHistoryAppended);
            }
            Action::DeleteMessages(message_ids, revoke) => {
                client
                    .delete_messages(
                        app_context.store().open_chat_id().as_i64(),
                        message_ids.to_vec(),
                        *revoke,
                    )
                    .await;
            }
            Action::ReplyMessage(message_id, message) => {
                app_context
                    .store()
                    .set_reply_message_i64(*message_id, message.to_string());
            }
            Action::ViewAllMessages => {
                client.view_all_messages().await;
            }
            Action::ViewPhotoMessage(message_id) => {
                // Get the message and check if it's a photo that needs downloading
                if let Some(message) = app_context.store().get_message(*message_id)
                    && let libtgt::model::MessageContentType::Photo { file_id, file_path } =
                        message.content_type()
                {
                    // Check if file needs to be downloaded
                    if file_path.is_empty() || !std::path::Path::new(file_path).exists() {
                        // Download the file
                        match client.download_file(*file_id, 32).await {
                            Ok(downloaded_path) => {
                                // Notify PhotoViewer that the photo is ready (viewer will send LoadPhotoFromPath)
                                app_context
                                    .action_tx()
                                    .send(Action::PhotoDownloaded(downloaded_path))?;
                            }
                            Err(e) => {
                                tracing::error!("Failed to download photo: {:?}", e);
                                let _ = app_context.action_tx().send(Action::StatusMessage(
                                    "Failed to download photo.".into(),
                                ));
                            }
                        }
                    }
                }
            }
            #[cfg(not(feature = "audio"))]
            Action::PlayVoiceMessage(_) => {
                let _ = app_context.action_tx().send(Action::StatusMessage(
                    "Audio playback is disabled. Build with default features.".into(),
                ));
            }
            #[cfg(feature = "audio")]
            Action::PlayVoiceMessage(message_id) => {
                let message_opt = app_context.store().get_message(*message_id);
                let skip_voice_disabled = cfg!(not(feature = "voice-message"))
                    && message_opt.as_ref().is_none_or(|m| m.is_voice_note());
                if skip_voice_disabled {
                    let _ = app_context.action_tx().send(Action::StatusMessage(
                        "Voice messages are disabled. Build with the voice-message feature (requires libopus).".into(),
                    ));
                    app_context.mark_dirty();
                }
                if !skip_voice_disabled
                    && let Some(message) = message_opt
                    && let Some((file_id, file_path, duration_secs)) =
                        message.voice_audio_file_info()
                {
                    let path = if file_path.is_empty() || !std::path::Path::new(&file_path).exists()
                    {
                        match client.download_file(file_id, 32).await {
                            Ok(downloaded) => {
                                if let Some(entry) = app_context
                                    .store()
                                    .open_chat_messages()
                                    .get_message_mut(*message_id)
                                {
                                    entry.set_audio_file_path(downloaded.clone());
                                }
                                downloaded
                            }
                            Err(e) => {
                                tracing::error!("Failed to download voice/audio file: {:?}", e);
                                continue;
                            }
                        }
                    } else {
                        file_path
                    };
                    let state = app_context.voice_playback_state();
                    let is_playing_this = state.message_id == Some(*message_id) && state.is_playing;
                    drop(state);
                    if is_playing_this {
                        app_context.voice_playback_send(crate::audio::VoicePlaybackCommand::Stop);
                        let mut s = app_context.voice_playback_state();
                        s.is_playing = false;
                        s.message_id = None;
                    } else {
                        app_context.voice_playback_send(crate::audio::VoicePlaybackCommand::Stop);
                        let sent = app_context.voice_playback_send(
                            crate::audio::VoicePlaybackCommand::Play {
                                path: path.clone(),
                                duration_secs: duration_secs.max(0) as u64,
                                message_id: *message_id,
                            },
                        );
                        if !sent {
                            let _ = app_context.action_tx().send(Action::StatusMessage(
                                "Voice: playback unavailable".to_string(),
                            ));
                        } else {
                            let mut s = app_context.voice_playback_state();
                            s.message_id = Some(*message_id);
                            s.position_secs = 0;
                            s.duration_secs = duration_secs.max(0) as u64;
                            s.is_playing = false;
                        }
                    }
                }
            }
            #[cfg(feature = "audio")]
            Action::VoicePlaybackStarted(msg_id) => {
                let mut state = app_context.voice_playback_state();
                if state.message_id == Some(*msg_id) {
                    state.is_playing = true;
                }
            }
            #[cfg(feature = "audio")]
            Action::VoicePlaybackPosition(msg_id, pos, dur) => {
                let mut state = app_context.voice_playback_state();
                state.message_id = Some(*msg_id);
                state.position_secs = *pos;
                state.duration_secs = *dur;
                state.is_playing = true;
            }
            #[cfg(feature = "audio")]
            Action::VoicePlaybackEnded(msg_id) => {
                let mut state = app_context.voice_playback_state();
                if state.message_id == Some(*msg_id) {
                    state.is_playing = false;
                }
            }
            Action::LoadPhotoFromPath(path, message_id) => {
                // Decode image on a blocking thread to avoid stalling the main loop
                let photo_max_dimension = app_context.app_config().photo_max_dimension;
                let action_tx = app_context.action_tx().clone();
                let path = path.clone();
                let msg_id = *message_id;
                tokio::spawn(async move {
                    let result = tokio::task::spawn_blocking(move || {
                        image::open(&path).map_err(|e| e.to_string())
                    })
                    .await
                    .unwrap_or_else(|e| Err(e.to_string()));
                    // Downscale when photo_max_dimension > 0 (0 = no downscaling)
                    let result = result.map(|img| {
                        if photo_max_dimension == 0 {
                            img
                        } else {
                            let (w, h) = (img.width(), img.height());
                            if w.max(h) > photo_max_dimension {
                                if w >= h {
                                    img.thumbnail(
                                        photo_max_dimension,
                                        (h * photo_max_dimension / w).max(1),
                                    )
                                } else {
                                    img.thumbnail(
                                        (w * photo_max_dimension / h).max(1),
                                        photo_max_dimension,
                                    )
                                }
                            } else {
                                img
                            }
                        }
                    });
                    let payload = crate::app::action::PhotoDecodedPayload(msg_id, result);
                    let _ = action_tx.send(Action::PhotoDecoded(payload));
                });
            }
            _ => {}
        }

        if action_changes_ui(&action) {
            app_context.mark_dirty();
        }
        tui.update(action.clone())
    }

    if raw_len >= 32 {
        tokio::task::yield_now().await;
    }

    if app_context.should_render() {
        tui_backend.terminal.draw(|f| {
            tui.draw(f, f.area()).unwrap();
        })?;
        app_context.clear_render_flag();
    }
    Ok(())
}
