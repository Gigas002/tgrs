//! Application orchestration: runs the chosen command and the TUI event loop.

pub mod action;
pub mod auth;
pub mod context;
pub mod error;
pub mod event;
pub mod keychain;
pub mod media;

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
    model::{ReportStep, SignInAlert, TdMessageReplyToMessage, profile_change_failure},
    store::Store,
    tdlib_rs::enums::ChatList,
};
use ratatui::layout::Rect;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::{
    app::{
        action::{Action, ChatCommand, ForwardRequest, PickerKind, ReactionChoice},
        auth::StdinAuthenticator,
        context::AppContext,
        error::AppError,
        event::Event,
    },
    notifier,
    settings::{ActionBinding, Command, Settings, is_plain_char},
    ui::component_name::ComponentName,
    ui::{Tui, chat_actions, find_chat, terminal::TuiBackend},
};

/// Messages per history request.
const HISTORY_BATCH: i32 = 50;
/// Chats fetched at startup so the list reaches its latest state quickly.
const INITIAL_CHATS: i32 = 200;
/// Redraw tick (~60 FPS).
const REFRESH_MS: u64 = 16;

/// Runs the command selected on the command line.
pub fn run(settings: &Settings) -> Result<(), AppError> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run_telegram(settings))
}

/// Logs in, then runs the command that needs Telegram.
async fn run_telegram(settings: &Settings) -> Result<(), AppError> {
    let database_dir = settings.telegram.database_dir.clone();
    let mut keychain = keychain::open(settings.credentials)?;
    let mut params = settings.telegram.clone();
    params.database_encryption_key =
        keychain::load_key(&mut keychain, settings.credentials, &database_dir)?;

    let store = Arc::new(Store::default());
    if let Err(e) = libtgt::client::configure_logging(&settings.tdlib_log) {
        tracing::error!("failed to set up the TDLib log: {e}");
    }
    let mut client = Client::new(Arc::clone(&store), params);
    let (notify_tx, notify_rx) = tokio::sync::mpsc::unbounded_channel();
    client.start(notify_tx);
    client.set_logging(&settings.tdlib_log).await;
    client.authorize(&mut StdinAuthenticator).await;

    if client.is_authorized() {
        if let Some(keychain) = &keychain
            && !client.is_database_encrypted()
            && !matches!(settings.command, Command::Logout)
        {
            keychain::encrypt_database(&mut client, keychain, settings.credentials, &database_dir)
                .await?;
        }
        client.use_quick_ack().await;
        client.get_me().await;
        let result = match &settings.command {
            Command::Logout => {
                client.log_out().await;
                // TDLib destroys the database when logging out
                keychain::forget_key(settings.credentials, &database_dir);
                Ok(())
            }
            _ => {
                client.load_chats(ChatList::Main, INITIAL_CHATS).await;
                // For the archive row's preview and unread count
                client.load_chats(ChatList::Archive, INITIAL_CHATS).await;
                if settings.notifications.enabled {
                    client.enable_notifications().await;
                }
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

    // One blocking wait (tick | notification), then drain every queue.
    let result = loop {
        let mut notification = None;
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(REFRESH_MS)) => {}
            _ = wake_rx.recv() => {}
            n = notify_rx.recv() => notification = n,
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

/// The reaction picker for a message: our reactions first (picking one
/// removes it), then the ones the chat allows.
fn reaction_picker(app_context: &AppContext, message_id: i64, available: Vec<String>) -> Action {
    let mut reactions: Vec<ReactionChoice> = app_context
        .store()
        .get_message(message_id)
        .map(|message| {
            message
                .reactions()
                .iter()
                .filter(|reaction| reaction.chosen && reaction.is_emoji())
                .map(|reaction| ReactionChoice {
                    emoji: reaction.emoji.clone(),
                    chosen: true,
                })
                .collect()
        })
        .unwrap_or_default();
    for emoji in available {
        if !reactions.iter().any(|reaction| reaction.emoji == emoji) {
            reactions.push(ReactionChoice {
                emoji,
                chosen: false,
            });
        }
    }
    Action::ShowPicker(PickerKind::Reactions {
        message_id,
        reactions,
    })
}

/// What to tell after forwarding `count` messages as `request` asked.
fn forward_status(
    request: &ForwardRequest,
    count: usize,
    result: Result<usize, libtgt::tdlib_rs::types::Error>,
) -> String {
    let to = &request.to_chat_name;
    match result {
        Ok(0) if count == 1 => format!("Forwarded to {to}"),
        Ok(0) => format!("Forwarded {count} messages to {to}"),
        Ok(failed) if failed == count => format!("Can't forward to {to}"),
        Ok(failed) => format!("Forwarded {} of {count} messages to {to}", count - failed),
        Err(e) => format!("Can't forward to {to}: {}", e.message),
    }
}

/// The status bar warning about a new sign-in.
fn sign_in_message(alert: &SignInAlert) -> String {
    format!("{}. Not you? Review sessions with space d.", alert.text())
}

/// Turns a store change into the actions the UI reacts to.
fn handle_notification(
    app_context: &AppContext,
    notification: Notification,
) -> Result<(), AppError> {
    let action_tx = app_context.action_tx();
    match notification {
        Notification::ChatListChanged => action_tx.send(Action::ChatHistoryAppended)?,
        Notification::OpenChatMessageAdded(_) => {
            // ChatWindow follows new messages when the reader is already at the
            // latest message. Setting a jump target here would pull anyone
            // reading older history away from their current position.
            action_tx.send(Action::ChatHistoryAppended)?;
            action_tx.send(Action::ViewAllMessages)?;
        }
        Notification::OpenChatMessageChanged(_) => app_context.mark_dirty(),
        Notification::OpenChatPinsChanged => action_tx.send(Action::LoadPinnedMessages)?,
        Notification::OpenChatHistoryInvalidated => action_tx.send(Action::GetChatHistory)?,
        Notification::OpenChatTopicsChanged => app_context.mark_dirty(),
        // The open chat's messages are on screen already
        Notification::NewMessage(alert)
            if alert.chat_id != app_context.store().open_chat_id_i64() =>
        {
            notifier::show(alert, app_context.notifications().show_preview);
        }
        Notification::NewMessage(_) => {}
        Notification::NewSignIn(alert) => {
            action_tx.send(Action::StatusMessage(sign_in_message(&alert)))?
        }
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

            if focused == Some(ComponentName::LeaderMenu) {
                return handle_leader_key(&app_context, &key_event);
            }
            // The chat search takes typed characters as text
            let typing = focused == Some(ComponentName::ChatList) && app_context.is_typing();
            if typing && is_plain_char(&key_event) {
                app_context
                    .action_tx()
                    .send(Action::Key(key, modifiers.into()))?;
                return Ok(());
            }

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

/// A key pressed in the leader menu: its `[leader_menu]` binding runs after
/// the menu closes (and gives the focus back); any other key just closes it.
fn handle_leader_key(app_context: &AppContext, key: &Event) -> Result<(), AppError> {
    let action_tx = app_context.action_tx();
    action_tx.send(Action::HideLeaderMenu)?;
    if let Some(ActionBinding::Single { action, .. }) = app_context
        .keymap_config()
        .section(ComponentName::LeaderMenu)
        .and_then(|section| section.get(key))
    {
        action_tx.send(action.clone())?;
    }
    Ok(())
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
            | Action::ShowMessageActionsFor(_)
            | Action::ShowChatActionsFor(..)
            | Action::HideChatActions
            | Action::ChatActionsNext
            | Action::ChatActionsPrevious
            | Action::ChatActionsConfirm
            | Action::ShowFindChat
            | Action::MovePinnedChat(_)
            | Action::ShowReport(..)
            | Action::HideReport
            | Action::ReportNext
            | Action::ReportPrevious
            | Action::ReportConfirm
            | Action::ReportStepped(_)
            | Action::ReportFailed(_)
            | Action::ShowBlockedList
            | Action::HideBlockedList
            | Action::BlockedListLoaded(_)
            | Action::BlockedListNext
            | Action::BlockedListPrevious
            | Action::BlockedListUnblock
            | Action::ShowForward(..)
            | Action::HideForward
            | Action::ForwardNext
            | Action::ForwardPrevious
            | Action::ForwardToggleCopy
            | Action::ForwardConfirm
            | Action::ShowSessions
            | Action::HideSessions
            | Action::SessionsLoaded(_)
            | Action::SessionsNext
            | Action::SessionsPrevious
            | Action::SessionsTerminate
            | Action::SessionsTerminateOthers
            | Action::SessionsConfirm
            | Action::HideFindChat
            | Action::FindChatConfirm
            | Action::FindChatFailed(_)
            | Action::OpenChatById(_)
            | Action::ShowProfileFor(_)
            | Action::ProfileLoaded(_)
            | Action::HideProfile
            | Action::ProfileScrollDown
            | Action::ProfileScrollUp
            | Action::PromptPasteImage
            | Action::ShowEditProfile
            | Action::HideEditProfile
            | Action::EditProfileNextField
            | Action::EditProfilePreviousField
            | Action::EditProfileConfirm
            | Action::EditProfileLoaded
            | Action::EditProfileFailed(..)
            | Action::ProfileSaved
            | Action::HideMessageActions
            | Action::MessageActionsPrevious
            | Action::MessageActionsNext
            | Action::ShowLeaderMenu
            | Action::HideLeaderMenu
            | Action::FocusNextPane
            | Action::FocusPreviousPane
            | Action::ResizePaneWider
            | Action::ResizePaneNarrower
            | Action::ResizePaneTaller
            | Action::ResizePaneShorter
            | Action::UnfocusComponent
            | Action::ShowPicker(_)
            | Action::HidePicker
            | Action::PickerPrevious
            | Action::PickerNext
            | Action::ShowStickerPicker
            | Action::HideStickerPicker
            | Action::StickerSetsLoaded(_)
            | Action::StickerSetLoaded(_)
            | Action::StickerPickerLeft
            | Action::StickerPickerRight
            | Action::StickerPickerUp
            | Action::StickerPickerDown
            | Action::StickerPickerNextSet
            | Action::StickerPickerPreviousSet
            | Action::RunMessageCommand(..)
            | Action::StatusMessage(_)
            | Action::ShowFileUploadExplorer
            | Action::HideFileUploadExplorer
            | Action::LoadPinnedMessages
            | Action::ShowPinnedMessagesPopup
            | Action::PinnedPopupConfirmJump
            | Action::HidePinnedMessagesPopup
            | Action::PinnedPopupPrevious
            | Action::PinnedPopupNext
            | Action::Refresh
            | Action::ToggleChatList
            | Action::ToggleArchiveList
            | Action::CloseArchiveList
            | Action::ArchiveListNext
            | Action::ArchiveListPrevious
            | Action::ArchiveListOpen
            | Action::ToggleTopicList
            | Action::LoadForumTopics
            | Action::TopicListNext
            | Action::TopicListPrevious
            | Action::TopicListOpen
            | Action::IncreaseChatListSize
            | Action::DecreaseChatListSize
            | Action::IncreasePromptSize
            | Action::DecreasePromptSize
            | Action::HideChatWindowReply
            | Action::EditMessage(_, _)
            | Action::ReplyMessage(_, _)
            | Action::ShowCommandGuide
            | Action::HideCommandGuide
            | Action::UpdateArea(_)
            | Action::GetChatHistoryNewer
            | Action::ChatHistoryAppended
    ) || preview_changes_ui(action)
}

/// Runs a chat action and says how it went in the status bar. A chat that is
/// left or deleted while open is closed.
async fn run_chat_command(
    app_context: &AppContext,
    client: &Client,
    command: ChatCommand,
    chat_id: i64,
) {
    let Some(state) = app_context.store().chat_state(chat_id) else {
        return;
    };
    let label = chat_actions::command_label(command, &state);
    let result = match command {
        ChatCommand::ToggleMute => client.set_chat_muted(chat_id, !state.is_muted).await,
        ChatCommand::ToggleArchive => client.set_chat_archived(chat_id, !state.is_archived).await,
        ChatCommand::TogglePin => client.set_chat_pinned(chat_id, !state.is_pinned).await,
        ChatCommand::ToggleRead => {
            client
                .set_chat_read(chat_id, state.is_unread, state.last_message_id)
                .await
        }
        ChatCommand::ToggleBlock => client.set_chat_blocked(chat_id, !state.is_blocked).await,
        // Reported step by step in its own popup
        ChatCommand::Report => {
            let _ = app_context
                .action_tx()
                .send(Action::ShowReport(chat_id, Vec::new()));
            return;
        }
        ChatCommand::Remove if state.is_left() => client.leave_chat(chat_id).await,
        ChatCommand::Remove => client.delete_chat(chat_id).await,
    };
    let message = match result {
        Ok(()) => {
            if command == ChatCommand::Remove {
                close_chat(app_context, chat_id);
            }
            format!(
                "{}: {}",
                state.title,
                chat_actions::done_label(command, &state)
            )
        }
        Err(e) => format!("Can't {label}: {}", e.message),
    };
    media::status(app_context, message);
}

/// Closes `chat_id` if it's the open chat.
fn close_chat(app_context: &AppContext, chat_id: i64) {
    let store = app_context.store();
    if store.open_chat_id().as_i64() != chat_id {
        return;
    }
    store.set_open_chat_id_i64(0);
    store.set_open_chat_user(None);
    store.set_open_chat_is_forum(false);
    store.set_open_topic_id(None);
    store.set_open_chat_topics(Vec::new());
    store.clear_open_chat_messages();
    store.set_open_chat_pinned(Vec::new());
    store.clear_reply_message();
}

#[cfg(feature = "media-preview")]
fn preview_changes_ui(action: &Action) -> bool {
    matches!(action, Action::PreviewDecoded(_))
}

#[cfg(not(feature = "media-preview"))]
fn preview_changes_ui(_: &Action) -> bool {
    false
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
                        app_context.store().open_send_topic(),
                        reply_to.clone(),
                    )
                    .await;
                app_context.action_tx().send(Action::HideChatWindowReply)?;
            }
            Action::SendReport(request) => {
                let answer = match client
                    .report_chat(
                        request.chat_id,
                        &request.option_id,
                        request.message_ids.clone(),
                        &request.text,
                    )
                    .await
                {
                    Ok(ReportStep::Done) => {
                        media::status(&app_context, "Reported. Thank you.".to_owned());
                        Action::HideReport
                    }
                    Ok(step) => Action::ReportStepped(step),
                    Err(e) => Action::ReportFailed(format!("Can't report: {}", e.message)),
                };
                app_context.action_tx().send(answer)?;
            }
            Action::ShowBlockedList => match client.blocked_senders().await {
                Ok(blocked) => app_context
                    .action_tx()
                    .send(Action::BlockedListLoaded(blocked))?,
                Err(e) => {
                    media::status(&app_context, format!("No blocked list: {}", e.message));
                    app_context.action_tx().send(Action::HideBlockedList)?;
                }
            },
            Action::UnblockSender(sender) => {
                let name = app_context
                    .store()
                    .sender_name(*sender)
                    .unwrap_or_else(|| "Someone".to_owned());
                match client.set_sender_blocked(*sender, false).await {
                    Ok(()) => media::status(&app_context, format!("{name}: unblocked")),
                    Err(e) => {
                        media::status(&app_context, format!("Can't unblock {name}: {}", e.message));
                        // Show them again
                        app_context.action_tx().send(Action::ShowBlockedList)?;
                    }
                }
            }
            Action::ForwardMessages(request) => {
                let count = request.message_ids.len();
                let result = client
                    .forward_messages(
                        request.to_chat_id,
                        request.from_chat_id,
                        request.message_ids.clone(),
                        request.send_copy,
                    )
                    .await;
                media::status(&app_context, forward_status(request, count, result));
            }
            Action::ShowSessions => match client.active_sessions().await {
                Ok(sessions) => app_context
                    .action_tx()
                    .send(Action::SessionsLoaded(sessions))?,
                Err(e) => {
                    media::status(&app_context, format!("No sessions: {}", e.message));
                    app_context.action_tx().send(Action::HideSessions)?;
                }
            },
            Action::TerminateSession(session_id, name) => {
                match client.terminate_session(*session_id).await {
                    Ok(()) => media::status(&app_context, format!("{name}: logged out")),
                    Err(e) => {
                        media::status(&app_context, format!("Can't log out {name}: {}", e.message));
                        // Show what's still logged in
                        app_context.action_tx().send(Action::ShowSessions)?;
                    }
                }
            }
            Action::TerminateOtherSessions => match client.terminate_other_sessions().await {
                Ok(()) => media::status(&app_context, "Every other session logged out".to_owned()),
                Err(e) => {
                    media::status(
                        &app_context,
                        format!("Can't log out other sessions: {}", e.message),
                    );
                    app_context.action_tx().send(Action::ShowSessions)?;
                }
            },
            Action::ConfirmSession(session_id, name) => {
                match client.confirm_session(*session_id).await {
                    Ok(()) => media::status(&app_context, format!("{name}: confirmed")),
                    Err(e) => {
                        media::status(&app_context, format!("Can't confirm {name}: {}", e.message));
                        app_context.action_tx().send(Action::ShowSessions)?;
                    }
                }
            }
            Action::FindChat(username) => {
                let found = match client.search_public_chat(username).await {
                    Ok(chat_id) => Action::OpenChatById(chat_id),
                    Err(e) => {
                        Action::FindChatFailed(find_chat::failure_message(username, &e.message))
                    }
                };
                app_context.action_tx().send(found)?;
            }
            Action::ShowProfileFor(chat_id) => match client.load_profile(*chat_id).await {
                Ok(()) => app_context
                    .action_tx()
                    .send(Action::ProfileLoaded(*chat_id))?,
                Err(e) => media::status(&app_context, format!("No profile: {}", e.message)),
            },
            Action::ShowEditProfile => {
                let me = app_context.store().me();
                if me != 0 {
                    let loaded = match client.load_profile(me).await {
                        Ok(()) => Action::EditProfileLoaded,
                        Err(e) => Action::EditProfileFailed(
                            None,
                            format!("Can't load your bio: {}", e.message),
                        ),
                    };
                    app_context.action_tx().send(loaded)?;
                }
            }
            Action::SaveProfile(changes) => {
                let mut result = Action::ProfileSaved;
                // In order; the rest waits for a failed change to be fixed
                for change in changes {
                    if let Err(e) = client.change_profile(change).await {
                        let (field, message) = profile_change_failure(change, &e.message);
                        result = Action::EditProfileFailed(Some(field), message);
                        break;
                    }
                }
                app_context.action_tx().send(result)?;
            }
            Action::SaveDraft(chat_id, topic_id, text) => {
                if let Err(e) = client.set_draft(*chat_id, *topic_id, text).await {
                    media::status(&app_context, format!("Can't save the draft: {}", e.message));
                }
            }
            Action::SetPinnedChats(archived, order) => {
                if let Err(e) = client.set_pinned_chats(*archived, order.clone()).await {
                    media::status(&app_context, format!("Can't move the chat: {}", e.message));
                }
            }
            Action::RunChatCommand(command, chat_id) => {
                run_chat_command(&app_context, client, *command, *chat_id).await;
            }
            Action::LoadStickerSets => match client.get_sticker_sets().await {
                Ok(sets) => app_context
                    .action_tx()
                    .send(Action::StickerSetsLoaded(sets))?,
                Err(e) => media::status(&app_context, format!("No stickers: {}", e.message)),
            },
            Action::LoadStickerSet(set_id) => match client.get_sticker_set(*set_id).await {
                Ok(set) => app_context
                    .action_tx()
                    .send(Action::StickerSetLoaded(set))?,
                Err(e) => media::status(&app_context, format!("No stickers: {}", e.message)),
            },
            Action::SendSticker(sticker) => {
                let store = app_context.store();
                let chat_id = store.open_chat_id().as_i64();
                if chat_id == 0 {
                    continue;
                }
                // Answers the message being replied to, like a text message would
                let reply_to = Some(store.reply_message_id().as_i64())
                    .filter(|id| *id > 0)
                    .map(|message_id| TdMessageReplyToMessage {
                        chat_id: 0,
                        message_id,
                    });
                let replying = reply_to.is_some();
                let topic = store.open_send_topic();
                if let Err(e) = client.send_sticker(sticker, chat_id, topic, reply_to).await {
                    media::status(&app_context, format!("Sticker not sent: {}", e.message));
                } else if replying {
                    app_context.action_tx().send(Action::HideChatWindowReply)?;
                }
            }
            Action::SendMessageEdited(message_id, message) => {
                if let Err(e) = client.edit_message(*message_id, message.to_string()).await {
                    media::status(&app_context, format!("Not edited: {}", e.message));
                }
            }
            Action::SendFile(file_path, caption, reply_to) => {
                let chat_id = app_context.store().open_chat_id().as_i64();
                if chat_id == 0 {
                    let _ = app_context.action_tx().send(Action::StatusMessage(
                        "Open a chat before uploading a file.".into(),
                    ));
                    continue;
                }
                let topic = app_context.store().open_send_topic();
                let sent = client
                    .send_document(
                        file_path.to_string(),
                        caption.clone(),
                        chat_id,
                        topic,
                        reply_to.clone(),
                    )
                    .await;
                if let Err(e) = sent {
                    media::status(&app_context, format!("File not sent: {}", e.message));
                }
                if reply_to.is_some() {
                    app_context.action_tx().send(Action::HideChatWindowReply)?;
                }
            }
            Action::OpenMedia(message_id) => {
                media::open(&app_context, client.files(), *message_id);
            }
            Action::OpenUrl(url) => media::open_url(&app_context, url),
            Action::LoadReactions(message_id) => {
                let chat_id = app_context.store().open_chat_id().as_i64();
                match client.available_reactions(chat_id, *message_id).await {
                    Ok(available) => {
                        let picker = reaction_picker(&app_context, *message_id, available);
                        app_context.action_tx().send(picker)?;
                    }
                    Err(e) => media::status(&app_context, format!("Can't react: {}", e.message)),
                }
            }
            Action::SetReaction(message_id, emoji, add) => {
                let chat_id = app_context.store().open_chat_id().as_i64();
                if let Err(e) = client
                    .set_reaction(chat_id, *message_id, emoji.clone(), *add)
                    .await
                {
                    media::status(&app_context, format!("Can't react: {}", e.message));
                }
            }
            Action::SetPinned(message_id, pinned) => {
                let chat_id = app_context.store().open_chat_id().as_i64();
                let (verb, done) = if *pinned {
                    ("pin", "pinned")
                } else {
                    ("unpin", "unpinned")
                };
                match client.set_pinned(chat_id, *message_id, *pinned).await {
                    Ok(()) => media::status(&app_context, format!("Message {done}")),
                    Err(e) => media::status(&app_context, format!("Can't {verb}: {}", e.message)),
                }
            }
            Action::CopyMessageLink(message_id) => {
                let chat_id = app_context.store().open_chat_id().as_i64();
                match client.message_link(chat_id, *message_id).await {
                    Ok(link) => {
                        match arboard::Clipboard::new().and_then(|mut c| c.set_text(link)) {
                            Ok(()) => media::status(&app_context, "Message link copied".to_owned()),
                            Err(e) => {
                                media::status(&app_context, format!("Can't copy the link: {e}"))
                            }
                        }
                    }
                    Err(e) => media::status(&app_context, format!("No link: {}", e.message)),
                }
            }
            Action::DownloadMedia(message_id) => {
                media::download(&app_context, client.files(), *message_id);
            }
            #[cfg(feature = "media-preview")]
            Action::LoadPreview(request) => {
                media::load_preview(&app_context, client.files(), request.clone());
            }
            Action::LoadForumTopics => {
                let chat_id = app_context.store().open_chat_id().as_i64();
                match client.get_forum_topics(chat_id).await {
                    Ok(topics) if app_context.store().open_chat_id().as_i64() == chat_id => {
                        app_context.store().set_open_chat_topics(topics);
                    }
                    Ok(_) => {}
                    Err(e) => media::status(&app_context, format!("No topics: {}", e.message)),
                }
            }
            Action::GetChatHistory if !app_context.store().is_history_loading() => {
                app_context.store().set_history_loading(true);
                let chat_id = app_context.store().open_chat_id().as_i64();
                let topic_id = app_context.store().open_topic_id();
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
                    if app_context.store().open_chat_id().as_i64() != chat_id
                        || app_context.store().open_topic_id() != topic_id
                    {
                        break;
                    }

                    // Always use the oldest message in cache (or 0 if empty) to ensure continuity
                    let from_message_id = app_context.store().oldest_message_id().unwrap_or(0);
                    let (entries, _has_more) = client
                        .get_chat_history_batch(
                            chat_id,
                            topic_id,
                            from_message_id,
                            0,
                            HISTORY_BATCH,
                        )
                        .await;

                    if entries.is_empty() {
                        break;
                    }

                    if app_context.store().open_chat_id().as_i64() != chat_id
                        || app_context.store().open_topic_id() != topic_id
                    {
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
                let topic_id = app_context.store().open_topic_id();
                let topic = app_context.store().open_message_topic();
                let pins = client.fetch_all_pinned_messages(chat_id, topic).await;
                if app_context.store().open_chat_id().as_i64() == chat_id
                    && app_context.store().open_topic_id() == topic_id
                {
                    app_context.store().set_open_chat_pinned(pins);
                }
            }
            Action::GetChatHistoryNewer if !app_context.store().is_history_loading() => {
                let chat_id = app_context.store().open_chat_id().as_i64();
                let topic_id = app_context.store().open_topic_id();
                let newest = app_context.store().open_chat_messages().newest_message_id();
                if let Some(from_id) = newest {
                    app_context.store().set_history_loading(true);
                    // TDLib: offset -N = from_message_id + N newer messages; limit >= -offset
                    // Use offset -49 to get from_id (which we already have) + 49 newer, then filter out from_id
                    let (entries, _) = client
                        .get_chat_history_batch(
                            chat_id,
                            topic_id,
                            from_id,
                            -(HISTORY_BATCH - 1),
                            HISTORY_BATCH,
                        )
                        .await;
                    if app_context.store().open_chat_id().as_i64() == chat_id
                        && app_context.store().open_topic_id() == topic_id
                        && !entries.is_empty()
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
                let topic = app_context.store().open_message_topic();
                match client
                    .search_chat_messages(chat_id, topic, query.clone(), HISTORY_BATCH)
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
                let topic_id = app_context.store().open_topic_id();
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
                    .get_chat_history_batch(chat_id, topic_id, *message_id, 0, OLDER_LIMIT)
                    .await;

                // Load newer messages in multiple batches (TDLib offset limit is -100)
                let mut current_from_id = *message_id;
                for batch_num in 0..NEWER_BATCHES {
                    let (entries_newer, _) = client
                        .get_chat_history_batch(
                            chat_id,
                            topic_id,
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
