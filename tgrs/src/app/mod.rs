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

use crossterm::event::KeyCode;
use libtgrs::{
    client::{Client, Notification, Requests},
    model::{
        ReportStep, SignInAlert, TdMessageReplyToMessage, TransferState, format_size,
        profile_change_failure,
    },
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
    ui::{
        Tui, chat_actions, find_chat,
        leader_menu::LeaderMenu,
        terminal::{self, TuiBackend},
    },
};

/// Messages per history request.
const HISTORY_BATCH: i32 = 50;
/// Chats fetched at startup so the list reaches its latest state quickly.
const INITIAL_CHATS: i32 = 200;
/// How often the screen is redrawn with nothing else happening, for what
/// expires with time (status messages, typing notices).
const CLOCK_TICK: Duration = Duration::from_secs(1);

/// A background job: a Telegram request and what follows from its answer.
type Job = futures::future::BoxFuture<'static, ()>;

/// Runs jobs one at a time, in the order they came, off the UI loop: a slow
/// request then never holds up typing or drawing.
struct Worker(UnboundedSender<Job>);

impl Worker {
    fn start() -> Self {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Job>();
        tokio::spawn(async move {
            while let Some(job) = rx.recv().await {
                // Its own task, so a job that panics doesn't stop the ones after it
                if let Err(e) = tokio::spawn(job).await {
                    tracing::error!("a background request failed: {e}");
                }
            }
        });
        Self(tx)
    }

    fn run(&self, job: impl Future<Output = ()> + Send + 'static) {
        let _ = self.0.send(Box::pin(job));
    }
}

/// Background workers: what we send (kept in order: messages, edits,
/// reactions, typing, read marks, drafts) and what we load (history, pins,
/// topics, search), so a long history load doesn't hold up sending.
struct Workers {
    send: Worker,
    load: Worker,
    /// A read mark is waiting to be sent: it covers every message up to
    /// then, so a busy chat doesn't queue one per message ahead of ours.
    view_pending: Arc<std::sync::atomic::AtomicBool>,
}

impl Workers {
    fn start() -> Self {
        Self {
            send: Worker::start(),
            load: Worker::start(),
            view_pending: Arc::default(),
        }
    }
}

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
    if let Err(e) = libtgrs::client::configure_logging(&settings.tdlib_log) {
        tracing::error!("failed to set up the TDLib log: {e}");
    }
    let mut client = Client::new(Arc::clone(&store), params);
    let (notify_tx, notify_rx) = tokio::sync::mpsc::unbounded_channel();
    client.start(notify_tx);
    client.set_logging(&settings.tdlib_log).await;
    client.authorize(&mut StdinAuthenticator::default()).await;

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
        client.authorize(&mut StdinAuthenticator::default()).await;
        client.join().await;
        result
    } else {
        Err(AppError::Command("not authorized".into()))
    }
}

/// The TUI event loop.
// The action receiver's lock is held while waiting: the loop is its only
// reader, and it's let go before the actions are handled.
#[allow(clippy::await_holding_lock)]
async fn run_tui(
    app_context: Arc<AppContext>,
    client: &mut Client,
    mut notify_rx: UnboundedReceiver<Notification>,
) -> Result<(), AppError> {
    tracing::info!("starting the TUI");
    let mut tui = Tui::new(Arc::clone(&app_context));
    let mut tui_backend = TuiBackend::new(Arc::clone(&app_context))?;

    if app_context.privacy().appear_online {
        client.online().await;
    } else {
        client.offline().await;
    }
    client.disable_animated_emoji(true).await;

    tui_backend.enter()?;
    if app_context.notifications().unread_in_title {
        terminal::save_window_title();
        terminal::set_window_title(&notifier::window_title(0));
    }
    tui.register_action_handler(app_context.action_tx().clone())?;
    app_context.mark_dirty();

    // The chat list only rebuilds on LoadChats/ChatHistoryAppended/Resize.
    app_context
        .action_tx()
        .send(Action::LoadChats(ChatList::Main.into(), INITIAL_CHATS))?;
    if app_context.media().trims_cache() {
        app_context.action_tx().send(Action::TrimCache)?;
    }

    let workers = Workers::start();
    // Only for what changes with time alone: status messages and typing
    // notices expiring. Everything else redraws when it changes.
    let mut clock = tokio::time::interval(CLOCK_TICK);
    clock.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // Sleep until something happens, handle everything queued, then draw
    // once if anything changed.
    let result = loop {
        let mut event = None;
        let mut notification = None;
        let mut action = None;
        {
            let mut action_rx = app_context.action_rx();
            tokio::select! {
                e = tui_backend.next() => event = e,
                n = notify_rx.recv() => notification = n,
                a = action_rx.recv() => action = a,
                _ = clock.tick() => app_context.mark_dirty(),
            }
        }
        let step = async {
            let mut event = event;
            while let Some(ev) = event.take().or_else(|| tui_backend.try_next()) {
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
            handle_app_actions(
                Arc::clone(&app_context),
                &mut tui,
                &mut tui_backend,
                client,
                &workers,
                action,
            )
            .await
        };
        if let Err(err) = step.await {
            break Err(err);
        }
        if app_context.should_render() {
            if let Err(err) = tui_backend.terminal.draw(|f| {
                tui.draw(f, f.area()).unwrap();
            }) {
                break Err(err.into());
            }
            app_context.clear_render_flag();
        }
        if app_context.quit_acquire() || !client.is_authorized() {
            break Ok(());
        }
    };

    client.offline().await;
    if app_context.notifications().unread_in_title {
        terminal::restore_window_title();
    }
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
    result: Result<usize, libtgrs::tdlib_rs::types::Error>,
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

/// Tells a privacy change failed and shows what Telegram has; quiet when saved.
async fn privacy_saved(
    app_context: &Arc<AppContext>,
    client: &Client,
    what: &str,
    result: Result<(), libtgrs::tdlib_rs::types::Error>,
) -> Result<(), AppError> {
    if let Err(e) = result {
        media::status(app_context, format!("Can't change {what}: {}", e.message));
        if let Ok(settings) = client.privacy_settings().await {
            app_context
                .action_tx()
                .send(Action::PrivacyLoaded(settings))?;
        }
    }
    Ok(())
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
        Notification::OpenChatTopicsChanged | Notification::OpenChatActivityChanged => {
            app_context.mark_dirty()
        }
        // The open chat's messages are on screen already
        Notification::NewMessage(alert)
            if alert.chat_id != app_context.store().open_chat_id_i64() =>
        {
            let settings = app_context.notifications();
            if notifier::is_quiet(settings, chrono::Local::now().time()) {
                tracing::debug!(chat_id = alert.chat_id, "quiet hours: no notification");
            } else {
                let open_tx = action_tx.clone();
                notifier::show(alert, settings, move |chat_id| {
                    let _ = open_tx.send(Action::OpenChatById(chat_id));
                });
            }
        }
        Notification::NewMessage(_) => {}
        Notification::TransferChanged(_) => app_context.mark_dirty(),
        Notification::UnreadCountChanged(unread) => {
            if app_context.notifications().unread_in_title {
                terminal::set_window_title(&notifier::window_title(unread));
            }
        }
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
    // Input is answered on screen right away
    app_context.mark_dirty();
    match event {
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
            // The chat search takes typed characters as text, and Esc ends it
            let typing = focused == Some(ComponentName::ChatList) && app_context.is_typing();
            if typing && (is_plain_char(&key_event) || key == KeyCode::Esc) {
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
/// The keys that move through the entries keep it open.
fn handle_leader_key(app_context: &AppContext, key: &Event) -> Result<(), AppError> {
    let action_tx = app_context.action_tx();
    let action = match app_context
        .keymap_config()
        .section(ComponentName::LeaderMenu)
        .and_then(|section| section.get(key))
    {
        Some(ActionBinding::Single { action, .. }) => Some(action.clone()),
        _ => None,
    };
    match action {
        Some(action) if LeaderMenu::navigates(&action) => action_tx.send(action)?,
        Some(Action::HideLeaderMenu) | None => action_tx.send(Action::HideLeaderMenu)?,
        Some(action) => {
            action_tx.send(Action::HideLeaderMenu)?;
            action_tx.send(action)?;
        }
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
        ChatCommand::ClearHistory => client.clear_chat_history(chat_id, false).await,
        ChatCommand::ClearHistoryForEveryone => client.clear_chat_history(chat_id, true).await,
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

/// Handle incoming actions from the application.
///
/// Bursts of [`Action::ChatHistoryAppended`] / [`Action::Refresh`] are folded before handling
/// ([`fold_chat_list_refresh_actions`]) so one TDLib flurry does not run thousands of chat-list
/// rebuilds in a single turn. [`libtgrs::store::Store`] mutexes are not held across
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
    workers: &Workers,
    first: Option<Action>,
) -> Result<(), AppError> {
    let mut batch: Vec<Action> = first.into_iter().collect();
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
            target: "tgrs::actions",
            raw = raw_len,
            folded = folded_len,
            "folded action batch (TDLib/UI burst)"
        );
    }

    // Whatever the actions change is drawn: one frame for the whole batch
    app_context.mark_dirty();
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
            Action::Typing => {
                let store = app_context.store();
                let chat_id = store.open_chat_id_i64();
                if app_context.privacy().send_typing
                    && chat_id != 0
                    && app_context.should_send_typing(
                        chat_id,
                        store.open_topic_id(),
                        std::time::Instant::now(),
                    )
                {
                    let requests = client.requests();
                    let topic = store.open_send_topic();
                    workers
                        .send
                        .run(async move { requests.send_typing(chat_id, topic).await });
                }
            }
            Action::SendMessage(message, reply_to) => {
                app_context.reset_typing();
                let requests = client.requests();
                let (message, reply_to) = (message.to_string(), reply_to.clone());
                let chat_id = app_context.store().open_chat_id().as_i64();
                let topic = app_context.store().open_send_topic();
                let ctx = Arc::clone(&app_context);
                workers.send.run(async move {
                    if let Err(e) = requests
                        .send_message(message, chat_id, topic, reply_to)
                        .await
                    {
                        media::status(&ctx, format!("Message not sent: {}", e.message));
                    }
                });
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
            Action::ShowNotificationSettings => {
                let open_chat = app_context.store().open_chat_id_i64();
                match client.notification_settings(Some(open_chat)).await {
                    Ok(settings) => app_context
                        .action_tx()
                        .send(Action::NotificationSettingsLoaded(settings))?,
                    Err(e) => {
                        media::status(
                            &app_context,
                            format!("No notification settings: {}", e.message),
                        );
                        app_context
                            .action_tx()
                            .send(Action::HideNotificationSettings)?;
                    }
                }
            }
            Action::SetNotificationOption(target, option, value) => {
                if let Err(e) = client
                    .set_notification_option(*target, *option, *value)
                    .await
                {
                    media::status(
                        &app_context,
                        format!(
                            "Can't change {}: {}",
                            option.label().to_lowercase(),
                            e.message
                        ),
                    );
                    // Show what Telegram has
                    let open_chat = app_context.store().open_chat_id_i64();
                    if let Ok(settings) = client.notification_settings(Some(open_chat)).await {
                        app_context
                            .action_tx()
                            .send(Action::NotificationSettingsLoaded(settings))?;
                    }
                }
            }
            Action::ShowPrivacy => match client.privacy_settings().await {
                Ok(settings) => app_context
                    .action_tx()
                    .send(Action::PrivacyLoaded(settings))?,
                Err(e) => {
                    media::status(&app_context, format!("No privacy settings: {}", e.message));
                    app_context.action_tx().send(Action::HidePrivacy)?;
                }
            },
            Action::SetPrivacyRules(setting, rules) => {
                let result = client.set_privacy_rules(*setting, rules).await;
                privacy_saved(&app_context, client, setting.label(), result).await?;
            }
            Action::SetShowReadDate(show) => {
                let result = client.set_show_read_date(*show).await;
                privacy_saved(&app_context, client, "Read time", result).await?;
            }
            Action::ShowStorage => match client.storage_usage().await {
                Ok(usage) => app_context.action_tx().send(Action::StorageLoaded(usage))?,
                Err(e) => {
                    media::status(&app_context, format!("No storage usage: {}", e.message));
                    app_context.action_tx().send(Action::HideStorage)?;
                }
            },
            Action::ClearCache(chat_id) => {
                match client.clear_cache(*chat_id).await {
                    Ok(freed) => {
                        media::status(&app_context, format!("Freed {}", format_size(freed)))
                    }
                    Err(e) => media::status(
                        &app_context,
                        format!("Can't clear the cache: {}", e.message),
                    ),
                }
                // Show what's left
                if let Ok(usage) = client.storage_usage().await {
                    app_context.action_tx().send(Action::StorageLoaded(usage))?;
                }
            }
            Action::TrimCache => {
                let limits = app_context.media();
                match client
                    .trim_cache(limits.cache_max_size, limits.cache_max_age)
                    .await
                {
                    Ok(freed) if freed > 0 => {
                        tracing::info!("trimmed the file cache by {freed} bytes");
                        media::status(
                            &app_context,
                            format!("Cache trimmed: freed {}", format_size(freed)),
                        );
                    }
                    Ok(_) => {}
                    Err(e) => {
                        media::status(&app_context, format!("Can't trim the cache: {}", e.message))
                    }
                }
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
                let requests = client.requests();
                let (chat_id, topic_id, text) = (*chat_id, *topic_id, text.clone());
                let ctx = Arc::clone(&app_context);
                workers.send.run(async move {
                    if let Err(e) = requests.set_draft(chat_id, topic_id, &text).await {
                        media::status(&ctx, format!("Can't save the draft: {}", e.message));
                    }
                });
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
                let requests = client.requests();
                let sticker = sticker.clone();
                let ctx = Arc::clone(&app_context);
                workers.send.run(async move {
                    if let Err(e) = requests
                        .send_sticker(&sticker, chat_id, topic, reply_to)
                        .await
                    {
                        media::status(&ctx, format!("Sticker not sent: {}", e.message));
                    } else if replying {
                        let _ = ctx.action_tx().send(Action::HideChatWindowReply);
                    }
                });
            }
            Action::SendMessageEdited(message_id, message) => {
                let requests = client.requests();
                let (message_id, message) = (*message_id, message.to_string());
                let ctx = Arc::clone(&app_context);
                workers.send.run(async move {
                    if let Err(e) = requests.edit_message(message_id, message).await {
                        media::status(&ctx, format!("Not edited: {}", e.message));
                    }
                });
            }
            Action::SendFile(file_path, caption, reply_to, send_as) => {
                let chat_id = app_context.store().open_chat_id().as_i64();
                if chat_id == 0 {
                    let _ = app_context.action_tx().send(Action::StatusMessage(
                        "Open a chat before uploading a file.".into(),
                    ));
                    continue;
                }
                let topic = app_context.store().open_send_topic();
                let sent = client
                    .send_file(
                        file_path.to_string(),
                        caption.clone(),
                        chat_id,
                        topic,
                        reply_to.clone(),
                        *send_as,
                    )
                    .await;
                if let Err(e) = sent {
                    media::status(&app_context, format!("File not sent: {}", e.message));
                }
                if reply_to.is_some() {
                    app_context.action_tx().send(Action::HideChatWindowReply)?;
                }
            }
            Action::SendAlbum(file_paths, caption, reply_to, send_as) => {
                let chat_id = app_context.store().open_chat_id().as_i64();
                if chat_id == 0 {
                    continue;
                }
                let topic = app_context.store().open_send_topic();
                let sent = client
                    .send_album(
                        file_paths.clone(),
                        caption.clone(),
                        chat_id,
                        topic,
                        reply_to.clone(),
                        *send_as,
                    )
                    .await;
                if let Err(e) = sent {
                    media::status(&app_context, format!("Files not sent: {}", e.message));
                }
                if reply_to.is_some() {
                    app_context.action_tx().send(Action::HideChatWindowReply)?;
                }
            }
            Action::PauseDownload(file_id) => {
                let store = app_context.store();
                store.set_transfer_state(*file_id, TransferState::Paused);
                if let Err(e) = client.files().pause(*file_id).await {
                    store.set_transfer_state(*file_id, TransferState::Downloading);
                    media::status(&app_context, format!("Can't pause: {}", e.message));
                }
                app_context.mark_dirty();
            }
            Action::CancelDownload(file_id) => {
                let store = app_context.store();
                let before = store.transfer(*file_id).map(|t| t.state);
                store.set_transfer_state(*file_id, TransferState::Cancelled);
                if let Err(e) = client.files().cancel(*file_id).await {
                    if let Some(state) = before {
                        store.set_transfer_state(*file_id, state);
                    }
                    media::status(&app_context, format!("Can't cancel: {}", e.message));
                }
                app_context.mark_dirty();
            }
            Action::FetchMedia(file_id, purpose) => {
                if let Some(transfer) = app_context.store().transfer(*file_id) {
                    media::fetch(&app_context, client.files(), transfer.media, *purpose);
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
                let requests = client.requests();
                let (message_id, emoji, add) = (*message_id, emoji.clone(), *add);
                let ctx = Arc::clone(&app_context);
                workers.send.run(async move {
                    if let Err(e) = requests.set_reaction(chat_id, message_id, emoji, add).await {
                        media::status(&ctx, format!("Can't react: {}", e.message));
                    }
                });
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
                let (ctx, requests) = (Arc::clone(&app_context), client.requests());
                workers.load.run(load_forum_topics(ctx, requests));
            }
            Action::GetChatHistory if !app_context.store().is_history_loading() => {
                // Set now: the request that follows waits for this one
                app_context.store().set_history_loading(true);
                let (ctx, requests) = (Arc::clone(&app_context), client.requests());
                workers.load.run(load_older_history(ctx, requests));
            }
            Action::LoadPinnedMessages => {
                let (ctx, requests) = (Arc::clone(&app_context), client.requests());
                workers.load.run(load_pinned_messages(ctx, requests));
            }
            Action::GetChatHistoryNewer if !app_context.store().is_history_loading() => {
                if app_context
                    .store()
                    .open_chat_messages()
                    .newest_message_id()
                    .is_some()
                {
                    app_context.store().set_history_loading(true);
                    let (ctx, requests) = (Arc::clone(&app_context), client.requests());
                    workers.load.run(load_newer_history(ctx, requests));
                }
            }
            Action::SearchChatMessages(query) => {
                let (ctx, requests) = (Arc::clone(&app_context), client.requests());
                workers
                    .load
                    .run(search_chat_messages(ctx, requests, query.clone()));
            }
            Action::JumpToMessage(message_id) => {
                let (ctx, requests) = (Arc::clone(&app_context), client.requests());
                workers
                    .load
                    .run(jump_to_message(ctx, requests, *message_id));
            }
            Action::DeleteMessages(message_ids, revoke) => {
                let requests = client.requests();
                let chat_id = app_context.store().open_chat_id().as_i64();
                let (message_ids, revoke) = (message_ids.to_vec(), *revoke);
                workers.send.run(async move {
                    requests.delete_messages(chat_id, message_ids, revoke).await;
                });
            }
            Action::ReplyMessage(message_id, message) => {
                app_context
                    .store()
                    .set_reply_message_i64(*message_id, message.to_string());
            }
            Action::ViewAllMessages => {
                use std::sync::atomic::Ordering;
                if !workers.view_pending.swap(true, Ordering::AcqRel) {
                    let requests = client.requests();
                    let pending = Arc::clone(&workers.view_pending);
                    workers.send.run(async move {
                        // Reads what's unread when it's sent
                        pending.store(false, Ordering::Release);
                        requests.view_all_messages().await;
                    });
                }
            }
            _ => {}
        }

        tui.update(action)
    }

    if raw_len >= 32 {
        tokio::task::yield_now().await;
    }
    Ok(())
}

/// Loads the open forum's topics.
async fn load_forum_topics(app_context: Arc<AppContext>, requests: Requests) {
    let chat_id = app_context.store().open_chat_id().as_i64();
    match requests.get_forum_topics(chat_id).await {
        Ok(topics) if app_context.store().open_chat_id().as_i64() == chat_id => {
            app_context.store().set_open_chat_topics(topics);
            app_context.mark_dirty();
        }
        Ok(_) => {}
        Err(e) => media::status(&app_context, format!("No topics: {}", e.message)),
    }
}

/// Loads older messages of the open chat; the history must be marked loading.
async fn load_older_history(app_context: Arc<AppContext>, requests: Requests) {
    let store = app_context.store();
    let chat_id = store.open_chat_id().as_i64();
    let topic_id = store.open_topic_id();
    // An empty cache (initial load) gets two batches to fill the window,
    // incremental scrolling one.
    let target_load = if store.open_chat_messages().is_empty() {
        2 * HISTORY_BATCH as usize
    } else {
        HISTORY_BATCH as usize
    };
    let mut loaded_count = 0;
    while loaded_count < target_load {
        if store.open_chat_id().as_i64() != chat_id || store.open_topic_id() != topic_id {
            break;
        }
        // Always from the oldest message in cache (or 0 if empty) for continuity
        let from_message_id = store.oldest_message_id().unwrap_or(0);
        let (entries, _has_more) = requests
            .get_chat_history_batch(chat_id, topic_id, from_message_id, 0, HISTORY_BATCH)
            .await;
        if entries.is_empty()
            || store.open_chat_id().as_i64() != chat_id
            || store.open_topic_id() != topic_id
        {
            break;
        }
        loaded_count += entries.len();
        store.open_chat_messages().insert_messages(entries);
        // Nothing older came: the start of the chat
        if store.oldest_message_id().unwrap_or(0) == from_message_id {
            break;
        }
        // Show the first batch while the next one loads
        let _ = app_context.action_tx().send(Action::ChatHistoryAppended);
    }
    store.set_history_loading(false);
    let _ = app_context.action_tx().send(Action::ChatHistoryAppended);
}

/// Loads messages newer than the newest loaded one; the history must be
/// marked loading.
async fn load_newer_history(app_context: Arc<AppContext>, requests: Requests) {
    let store = app_context.store();
    let chat_id = store.open_chat_id().as_i64();
    let topic_id = store.open_topic_id();
    let newest = store.open_chat_messages().newest_message_id();
    if let Some(from_id) = newest {
        // TDLib: offset -N = from_message_id + N newer messages; limit >= -offset.
        // from_id (which we have) + 49 newer, then from_id is left out.
        let (entries, _) = requests
            .get_chat_history_batch(
                chat_id,
                topic_id,
                from_id,
                -(HISTORY_BATCH - 1),
                HISTORY_BATCH,
            )
            .await;
        if store.open_chat_id().as_i64() == chat_id && store.open_topic_id() == topic_id {
            let new_messages = entries.into_iter().filter(|entry| entry.id() != from_id);
            store.open_chat_messages().insert_messages(new_messages);
        }
    }
    store.set_history_loading(false);
    let _ = app_context.action_tx().send(Action::ChatHistoryAppended);
}

/// Loads the pinned messages of the open chat (or topic).
async fn load_pinned_messages(app_context: Arc<AppContext>, requests: Requests) {
    let store = app_context.store();
    let chat_id = store.open_chat_id().as_i64();
    if chat_id == 0 {
        return;
    }
    let topic_id = store.open_topic_id();
    let topic = store.open_message_topic();
    let pins = requests.fetch_all_pinned_messages(chat_id, topic).await;
    if store.open_chat_id().as_i64() == chat_id && store.open_topic_id() == topic_id {
        store.set_open_chat_pinned(pins);
        app_context.mark_dirty();
    }
}

/// Searches the open chat (or topic) on the server.
async fn search_chat_messages(app_context: Arc<AppContext>, requests: Requests, query: String) {
    let action_tx = app_context.action_tx();
    let chat_id = app_context.store().open_chat_id().as_i64();
    if chat_id == 0 {
        let _ = action_tx.send(Action::StatusMessage(
            "Open a chat before searching messages.".into(),
        ));
        return;
    }
    let topic = app_context.store().open_message_topic();
    match requests
        .search_chat_messages(chat_id, topic, query, HISTORY_BATCH)
        .await
    {
        Ok(entries) => {
            let _ = action_tx.send(Action::StatusMessage(format!(
                "Search results: {}",
                entries.len()
            )));
            let _ = action_tx.send(Action::SearchResults(entries));
        }
        Err(e) => {
            let _ = action_tx.send(Action::StatusMessage(format!(
                "Search failed: {}",
                e.message
            )));
            let _ = action_tx.send(Action::SearchResults(vec![]));
        }
    }
}

/// Shows `message_id` in the open chat: selects it when it's loaded, or
/// loads the history around it.
async fn jump_to_message(app_context: Arc<AppContext>, requests: Requests, message_id: i64) {
    let store = app_context.store();
    let action_tx = app_context.action_tx();
    let chat_id = store.open_chat_id().as_i64();
    let topic_id = store.open_topic_id();
    if chat_id == 0 {
        return;
    }
    // Already loaded: just select it, without clearing the chat
    if store.open_chat_messages().get_message(message_id).is_some() {
        store.set_jump_target_message_id_i64(message_id);
        let _ = action_tx.send(Action::JumpCompleted(message_id));
        let _ = action_tx.send(Action::ChatHistoryAppended);
        return;
    }

    store.clear_open_chat_messages();
    store.set_history_loading(true);

    // A window around the target. TDLib's offset must be > -100, so the newer
    // messages come in several batches.
    const OLDER_LIMIT: i32 = 100; // target + 99 older messages
    const NEWER_BATCH_SIZE: i32 = 99; // the most offset -98 allows
    const NEWER_BATCHES: usize = 3; // ~297 newer messages

    let (mut all_entries, _) = requests
        .get_chat_history_batch(chat_id, topic_id, message_id, 0, OLDER_LIMIT)
        .await;
    let mut current_from_id = message_id;
    for batch_num in 0..NEWER_BATCHES {
        let (entries_newer, _) = requests
            .get_chat_history_batch(
                chat_id,
                topic_id,
                current_from_id,
                -(NEWER_BATCH_SIZE - 1),
                NEWER_BATCH_SIZE,
            )
            .await;
        // The boundary message is loaded already
        let new_msgs: Vec<_> = entries_newer
            .into_iter()
            .filter(|e| e.id() != current_from_id)
            .collect();
        let Some(last) = new_msgs.last() else {
            break;
        };
        current_from_id = last.id();
        all_entries.extend(new_msgs);
        tracing::debug!(
            batch_num,
            loaded = all_entries.len(),
            "JumpToMessage: loaded newer batch"
        );
    }

    let same_chat = store.open_chat_id().as_i64() == chat_id && store.open_topic_id() == topic_id;
    if !same_chat {
        tracing::debug!(message_id, "JumpToMessage: the chat changed while loading");
    } else if all_entries.is_empty() {
        tracing::warn!(
            target_id = message_id,
            "JumpToMessage: failed to load any messages, cache is empty"
        );
    } else {
        store.open_chat_messages().insert_messages(all_entries);
        tracing::debug!(
            target_id = message_id,
            total_cached = store.open_chat_messages().len(),
            "JumpToMessage: cache after insert"
        );
        // Only once the message is in the cache
        store.set_jump_target_message_id_i64(message_id);
    }
    store.set_history_loading(false);
    let _ = action_tx.send(Action::JumpCompleted(message_id));
    let _ = action_tx.send(Action::ChatHistoryAppended);
}
