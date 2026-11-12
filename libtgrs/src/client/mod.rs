//! TDLib client: requests, the login flow and the update loop.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tdlib_rs::enums::{
    self, AuthorizationState, BlockList, ChatList, ForumTopics, FoundChatMessages,
    InputMessageContent, InputMessageReplyTo, LogStream, MessageTopic, Messages, OptionValue,
    ReactionType, SearchMessagesFilter, StickerSets, StickerType, User,
};
use tdlib_rs::functions;
use tdlib_rs::types::{
    FormattedText, InputFileId, InputFileLocal, InputMessageDocument, InputMessagePhoto,
    InputMessageSticker, InputMessageText, InputSticker, LogStreamFile, MessageTopicForum,
    OptionValueBoolean, OptionValueInteger, ReactionTypeEmoji,
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;

use crate::model::notification_settings::MUTE_FOREVER;
use crate::model::{
    BlockedSender, ChatSettings, MessageEntry, NotificationOption, NotificationScope,
    NotificationSettings, NotificationTarget, PrivacyRules, PrivacySetting, PrivacySettings,
    ProfileChange, ProfileSource, ReportStep, ScopeSettings, SessionEntry, StickerEntry,
    StickerSetEntry, StorageUsage, TdChatList, TdMessageReplyToMessage, TdMessageSender,
    TopicEntry, apply_chat_option, apply_scope_option, session_entries, text_draft,
};
use crate::store::Store;

/// The chats with the most files that the storage popup lists.
const STORAGE_CHAT_LIMIT: i32 = 10;

mod auth;
mod execute;
mod updates;

#[cfg(test)]
mod tests;

pub use auth::{Authenticator, Login, Proxy, ProxyKind, TdLogging, TdParams};
pub use execute::configure_logging;
pub use updates::Notification;

/// Errors returned by TDLib requests.
pub type TdError = tdlib_rs::types::Error;

/// Messages per `getChatHistory` request.
const HISTORY_BATCH: i32 = 50;
/// Topics per `getForumTopics` request (TDLib's maximum).
const TOPICS_BATCH: i32 = 100;
/// Blocked users and chats per `getBlockedMessageSenders` request (TDLib's maximum).
const BLOCKED_BATCH: i32 = 100;
/// Topics loaded for a forum at most.
const MAX_TOPICS: usize = 1000;
/// Chats with notifications shown at once; any positive value enables them.
const NOTIFICATION_GROUP_COUNT_MAX: i64 = 25;

/// The requests that only need the client id and the store. Cheap to
/// clone into background tasks, so slow requests don't hold up the caller.
#[derive(Clone)]
pub struct Requests {
    client_id: i32,
    store: Arc<Store>,
}

pub struct Client {
    requests: Requests,
    params: TdParams,
    auth_rx: UnboundedReceiver<AuthorizationState>,
    auth_tx: UnboundedSender<AuthorizationState>,
    have_authorization: bool,
    /// Whether the database was opened with [`TdParams::database_encryption_key`].
    database_encrypted: bool,
    can_quit: Arc<AtomicBool>,
    /// Chat lists TDLib has loaded every chat of.
    full_chat_lists: HashSet<TdChatList>,
    updates: Option<JoinHandle<()>>,
}

impl Client {
    /// Creates a TDLib client. Call [`Client::start`] before [`Client::authorize`].
    pub fn new(store: Arc<Store>, params: TdParams) -> Self {
        let (auth_tx, auth_rx) = tokio::sync::mpsc::unbounded_channel();
        let client_id = tdlib_rs::create_client();
        tracing::info!("Created TDLib client with client_id: {}", client_id);

        Self {
            requests: Requests { client_id, store },
            params,
            auth_rx,
            auth_tx,
            have_authorization: false,
            database_encrypted: false,
            can_quit: Arc::new(AtomicBool::new(false)),
            full_chat_lists: HashSet::new(),
            updates: None,
        }
    }

    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    /// The requests that can run in a background task.
    pub fn requests(&self) -> Requests {
        self.requests.clone()
    }

    pub fn client_id(&self) -> i32 {
        self.client_id
    }

    pub fn is_authorized(&self) -> bool {
        self.have_authorization
    }

    /// Whether the TDLib database is encrypted. False when no key was given,
    /// or when the given key didn't match and the database was opened unencrypted.
    pub fn is_database_encrypted(&self) -> bool {
        self.database_encrypted
    }

    /// Starts the TDLib update loop; changes are announced on `notify`.
    pub fn start(&mut self, notify: UnboundedSender<Notification>) {
        self.updates = Some(tokio::spawn(updates::run(
            Arc::clone(&self.store),
            self.auth_tx.clone(),
            Arc::clone(&self.can_quit),
            notify,
        )));
    }

    /// Waits for the update loop to finish after the client was closed.
    pub async fn join(&mut self) {
        if let Some(handle) = self.updates.take()
            && let Err(e) = handle.await
        {
            tracing::error!("TDLib update loop failed: {e}");
        }
    }

    /// Applies `logging` to this client. Also the first request, which tdlib_rs
    /// needs to start receiving updates; call [`configure_logging`] before
    /// creating the client so TDLib's start-up isn't logged to the terminal.
    pub async fn set_logging(&self, logging: &TdLogging) {
        // tdlib_rs needs one request with the client to start receiving updates for it.
        if let Err(e) =
            functions::set_log_verbosity_level(logging.verbosity_level, self.client_id).await
        {
            tracing::error!("Failed to set TDLib log verbosity: {e:?}");
        }

        let log_stream_file = LogStreamFile {
            path: logging.path.to_string_lossy().into_owned(),
            max_file_size: 1 << 27,
            redirect_stderr: logging.redirect_stderr,
        };
        if let Err(e) =
            functions::set_log_stream(LogStream::File(log_stream_file), self.client_id).await
        {
            tracing::error!("Failed to set TDLib log stream: {e:?}");
        }
    }

    /// Drives the authorization state machine until the client is ready or closed.
    pub async fn authorize(&mut self, auth: &mut impl Authenticator) {
        tracing::info!("Handling authorization state");
        while let Some(state) = self.auth_rx.recv().await {
            match state {
                AuthorizationState::WaitTdlibParameters => {
                    let key = self.params.database_encryption_key.clone();
                    let mut response = self.set_tdlib_parameters(key.clone()).await;
                    // The key doesn't match an existing unencrypted database
                    // (e.g. a previous re-encryption failed): open it without
                    // one, so the caller can encrypt it again.
                    if let Err(e) = &response
                        && !key.is_empty()
                    {
                        tracing::warn!("Opening the TDLib database with its key failed: {e:?}");
                        response = self.set_tdlib_parameters(String::new()).await;
                        self.database_encrypted = false;
                    } else {
                        self.database_encrypted = !key.is_empty();
                    }
                    if let Err(e) = response {
                        auth.report_error(&e.message);
                        // Nothing more can happen without parameters
                        self.close().await;
                        continue;
                    }
                    // Before TDLib establishes connections
                    if let Some(proxy) = &self.params.proxy {
                        self.add_proxy(proxy).await;
                    }
                }
                AuthorizationState::WaitPhoneNumber => loop {
                    let response = match auth.login() {
                        Login::PhoneNumber(phone_number) => {
                            functions::set_authentication_phone_number(
                                phone_number,
                                None,
                                self.client_id,
                            )
                            .await
                        }
                        Login::QrCode => {
                            functions::request_qr_code_authentication(Vec::new(), self.client_id)
                                .await
                        }
                    };
                    match response {
                        Ok(_) => break,
                        Err(e) => auth.report_error(&e.message),
                    }
                },
                AuthorizationState::WaitOtherDeviceConfirmation(x) => auth.confirm_link(&x.link),
                AuthorizationState::WaitEmailAddress(_) => loop {
                    let email_address = auth.email_address();
                    match functions::set_authentication_email_address(email_address, self.client_id)
                        .await
                    {
                        Ok(_) => break,
                        Err(e) => auth.report_error(&e.message),
                    }
                },
                AuthorizationState::WaitEmailCode(_) => loop {
                    let code = auth.email_code();
                    match functions::check_authentication_email_code(
                        enums::EmailAddressAuthentication::Code(
                            tdlib_rs::types::EmailAddressAuthenticationCode { code },
                        ),
                        self.client_id,
                    )
                    .await
                    {
                        Ok(_) => break,
                        Err(e) => auth.report_error(&e.message),
                    }
                },
                AuthorizationState::WaitCode(_) => loop {
                    let code = auth.code();
                    match functions::check_authentication_code(code, self.client_id).await {
                        Ok(_) => break,
                        Err(e) => auth.report_error(&e.message),
                    }
                },
                AuthorizationState::WaitRegistration(_) => loop {
                    let (first_name, last_name) = auth.registration();
                    match functions::register_user(first_name, last_name, false, self.client_id)
                        .await
                    {
                        Ok(_) => break,
                        Err(e) => auth.report_error(&e.message),
                    }
                },
                AuthorizationState::WaitPassword(_) => loop {
                    let password = auth.password();
                    match functions::check_authentication_password(password, self.client_id).await {
                        Ok(_) => break,
                        Err(e) => auth.report_error(&e.message),
                    }
                },
                AuthorizationState::Ready => {
                    self.have_authorization = true;
                    break;
                }
                AuthorizationState::LoggingOut => {
                    self.have_authorization = false;
                    tracing::info!("Logging out");
                }
                AuthorizationState::Closing => {
                    self.have_authorization = false;
                    tracing::info!("Closing");
                }
                AuthorizationState::Closed => {
                    tracing::info!("Closed");
                    self.can_quit.store(true, Ordering::Release);
                    break;
                }
                AuthorizationState::WaitPremiumPurchase(_) => {
                    tracing::info!("Waiting for premium purchase confirmation");
                }
            }
        }
    }

    async fn set_tdlib_parameters(&self, database_encryption_key: String) -> Result<(), TdError> {
        let params = &self.params;
        functions::set_tdlib_parameters(
            params.use_test_dc,
            params.database_dir.to_string_lossy().into_owned(),
            String::new(),
            database_encryption_key,
            params.use_file_database,
            params.use_chat_info_database,
            params.use_message_database,
            false,
            params.api_id,
            params.api_hash.clone(),
            params.system_language_code.clone(),
            params.device_model.clone(),
            String::new(),
            params.application_version.clone(),
            self.client_id,
        )
        .await
    }

    /// Re-encrypts the TDLib database with `key` (base64; empty decrypts it).
    pub async fn set_database_encryption_key(&mut self, key: String) -> Result<(), TdError> {
        let encrypted = !key.is_empty();
        functions::set_database_encryption_key(key, self.client_id).await?;
        self.database_encrypted = encrypted;
        Ok(())
    }

    async fn add_proxy(&self, proxy: &Proxy) {
        tracing::info!(
            "Setting up {:?} proxy: {}:{}",
            proxy.kind,
            proxy.server,
            proxy.port
        );
        match functions::add_proxy(proxy.into(), true, String::new(), self.client_id).await {
            Ok(enums::AddedProxy::AddedProxy(added)) => {
                tracing::info!("Proxy added and enabled: id={}", added.id);
            }
            Err(e) => {
                tracing::error!("Failed to add proxy: {} (code: {})", e.message, e.code);
            }
        }
    }

    /// Closes the TDLib instance; [`Client::authorize`] then observes `Closed`.
    pub async fn close(&mut self) {
        self.have_authorization = false;
        match functions::close(self.client_id).await {
            Ok(()) => tracing::info!("TDLib client closed"),
            Err(e) => tracing::error!("Error closing TDLib client: {e:?}"),
        }
    }

    pub async fn log_out(&mut self) {
        match functions::log_out(self.client_id).await {
            Ok(()) => tracing::info!("Logged out"),
            Err(e) => tracing::error!("Error logging out: {e:?}"),
        }
    }

    pub async fn get_me(&self) {
        match functions::get_me(self.client_id).await {
            Ok(User::User(me)) => self.store.set_me(me.id),
            Err(e) => tracing::error!("Failed to get me: {e:?}"),
        }
    }

    pub async fn load_chats(&mut self, chat_list: ChatList, limit: i32) {
        let list = TdChatList::from(chat_list.clone());
        if self.full_chat_lists.contains(&list) {
            return;
        }
        // TDLib answers with a 404 once every chat is loaded
        if let Err(e) = functions::load_chats(Some(chat_list), limit, self.client_id).await {
            tracing::info!("Stopped loading chats of {list:?}: {e:?}");
            self.full_chat_lists.insert(list);
        }
    }

    pub async fn load_all_chats(&mut self) {
        while !self.full_chat_lists.contains(&TdChatList::Main) {
            self.load_chats(ChatList::Main, HISTORY_BATCH).await;
        }
    }
}

impl std::ops::Deref for Client {
    type Target = Requests;

    fn deref(&self) -> &Requests {
        &self.requests
    }
}

impl Requests {
    /// A handle for downloading files that can be moved into other tasks.
    pub fn files(&self) -> Files {
        Files {
            client_id: self.client_id,
        }
    }

    pub async fn search_chats(&self, query: &str) -> Result<tdlib_rs::types::Chats, TdError> {
        match functions::search_chats(query.to_owned(), None, 50, self.client_id).await {
            Ok(enums::Chats::Chats(chats)) => Ok(chats),
            Err(e) => {
                tracing::error!("Failed to search chats for {query:?}: {e:?}");
                Err(e)
            }
        }
    }

    /// Marks the open chat's unread messages as read.
    pub async fn view_all_messages(&self) {
        if let Err(e) = functions::view_messages(
            self.store.open_chat_id().as_i64(),
            self.store.unread_messages(),
            None,
            true,
            self.client_id,
        )
        .await
        {
            tracing::error!("Failed to view all messages: {e:?}");
        }
    }

    /// Turns a chat's notifications off for good, or back on. Its other
    /// notification settings are kept.
    pub async fn set_chat_muted(&self, chat_id: i64, muted: bool) -> Result<(), TdError> {
        let Some(mut settings) = self
            .store
            .chats()
            .get(&chat_id)
            .map(|chat| chat.notification_settings.clone())
        else {
            return Ok(());
        };
        settings.use_default_mute_for = false;
        settings.mute_for = if muted { MUTE_FOREVER } else { 0 };
        functions::set_chat_notification_settings(chat_id, settings, self.client_id)
            .await
            .inspect_err(|e| tracing::error!("Failed to mute chat {chat_id}: {e:?}"))
    }

    /// The defaults of private chats, groups and channels, and `chat_id`'s
    /// own settings when given.
    pub async fn notification_settings(
        &self,
        chat_id: Option<i64>,
    ) -> Result<NotificationSettings, TdError> {
        let mut scopes = Vec::with_capacity(NotificationScope::ALL.len());
        for scope in NotificationScope::ALL {
            let enums::ScopeNotificationSettings::ScopeNotificationSettings(td) =
                functions::get_scope_notification_settings(scope.into(), self.client_id)
                    .await
                    .inspect_err(|e| {
                        tracing::error!("Failed to get notification settings of {scope:?}: {e:?}")
                    })?;
            scopes.push(ScopeSettings::new(scope, &td));
        }
        let chat = chat_id.filter(|id| *id != 0).and_then(|chat_id| {
            let state = self.store.chat_state(chat_id)?;
            let td = self
                .store
                .chats()
                .get(&chat_id)?
                .notification_settings
                .clone();
            Some(ChatSettings::new(chat_id, state.title, state.kind, &td))
        });
        Ok(NotificationSettings { scopes, chat })
    }

    /// Turns a notification option on or off for a scope, or for a chat,
    /// where None goes back to the scope's default. The other settings are kept.
    pub async fn set_notification_option(
        &self,
        target: NotificationTarget,
        option: NotificationOption,
        value: Option<bool>,
    ) -> Result<(), TdError> {
        match target {
            NotificationTarget::Scope(scope) => {
                let enums::ScopeNotificationSettings::ScopeNotificationSettings(mut td) =
                    functions::get_scope_notification_settings(scope.into(), self.client_id)
                        .await?;
                apply_scope_option(&mut td, option, value.unwrap_or(true));
                functions::set_scope_notification_settings(scope.into(), td, self.client_id)
                    .await
                    .inspect_err(|e| {
                        tracing::error!("Failed to set notification settings of {scope:?}: {e:?}")
                    })
            }
            NotificationTarget::Chat(chat_id) => {
                let Some(mut td) = self
                    .store
                    .chats()
                    .get(&chat_id)
                    .map(|chat| chat.notification_settings.clone())
                else {
                    return Ok(());
                };
                apply_chat_option(&mut td, option, value);
                functions::set_chat_notification_settings(chat_id, td, self.client_id)
                    .await
                    .inspect_err(|e| {
                        tracing::error!("Failed to set notification settings of {chat_id}: {e:?}")
                    })
            }
        }
    }

    /// Who sees what of yours, with the read time setting.
    pub async fn privacy_settings(&self) -> Result<PrivacySettings, TdError> {
        let mut rules = Vec::with_capacity(PrivacySetting::ALL.len());
        for setting in PrivacySetting::ALL {
            let enums::UserPrivacySettingRules::UserPrivacySettingRules(td) =
                functions::get_user_privacy_setting_rules(setting.into(), self.client_id)
                    .await
                    .inspect_err(|e| {
                        tracing::error!("Failed to get privacy of {setting:?}: {e:?}")
                    })?;
            rules.push((setting, PrivacyRules::new(&td)));
        }
        let enums::ReadDatePrivacySettings::ReadDatePrivacySettings(read_date) =
            functions::get_read_date_privacy_settings(self.client_id)
                .await
                .inspect_err(|e| tracing::error!("Failed to get read date privacy: {e:?}"))?;
        Ok(PrivacySettings {
            rules,
            show_read_date: read_date.show_read_date,
        })
    }

    /// Saves who sees `setting`.
    pub async fn set_privacy_rules(
        &self,
        setting: PrivacySetting,
        rules: &PrivacyRules,
    ) -> Result<(), TdError> {
        functions::set_user_privacy_setting_rules(setting.into(), rules.to_td(), self.client_id)
            .await
            .inspect_err(|e| tracing::error!("Failed to set privacy of {setting:?}: {e:?}"))
    }

    /// Shows others when you read their messages, or hides it.
    pub async fn set_show_read_date(&self, show: bool) -> Result<(), TdError> {
        functions::set_read_date_privacy_settings(
            tdlib_rs::types::ReadDatePrivacySettings {
                show_read_date: show,
            },
            self.client_id,
        )
        .await
        .inspect_err(|e| tracing::error!("Failed to set read date privacy: {e:?}"))
    }

    /// Tells the chat, or one of its forum topics, that you're typing;
    /// Telegram shows it for about 5 seconds.
    pub async fn send_typing(&self, chat_id: i64, topic: Option<MessageTopic>) {
        if let Err(e) = functions::send_chat_action(
            chat_id,
            topic,
            Some(enums::ChatAction::Typing),
            self.client_id,
        )
        .await
        {
            tracing::debug!("Failed to send typing to {chat_id}: {e:?}");
        }
    }

    /// Moves a chat to the archive, or back to the main chat list.
    pub async fn set_chat_archived(&self, chat_id: i64, archived: bool) -> Result<(), TdError> {
        let list = if archived {
            ChatList::Archive
        } else {
            ChatList::Main
        };
        functions::add_chat_to_list(chat_id, list, self.client_id)
            .await
            .inspect_err(|e| tracing::error!("Failed to move chat {chat_id}: {e:?}"))
    }

    /// Pins a chat at the top of its chat list (the archive for an archived
    /// chat), or unpins it.
    pub async fn set_chat_pinned(&self, chat_id: i64, pinned: bool) -> Result<(), TdError> {
        let list = if self.store.is_archived(chat_id) {
            ChatList::Archive
        } else {
            ChatList::Main
        };
        functions::toggle_chat_is_pinned(list, chat_id, pinned, self.client_id)
            .await
            .inspect_err(|e| tracing::error!("Failed to pin chat {chat_id}: {e:?}"))
    }

    /// Keeps `text` as the unsent message of a chat, or of one of its forum
    /// topics (none for the General topic); blank text clears it.
    pub async fn set_draft(
        &self,
        chat_id: i64,
        topic_id: Option<i32>,
        text: &str,
    ) -> Result<(), TdError> {
        let topic = topic_id
            .map(|forum_topic_id| MessageTopic::Forum(MessageTopicForum { forum_topic_id }));
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i32);
        functions::set_chat_draft_message(chat_id, topic, text_draft(text, now), self.client_id)
            .await
            .inspect_err(|e| tracing::error!("Failed to save the draft of {chat_id}: {e:?}"))
    }

    /// Sets the order of the pinned chats of the main chat list, or of the
    /// archive, top first.
    pub async fn set_pinned_chats(
        &self,
        archived: bool,
        chat_ids: Vec<i64>,
    ) -> Result<(), TdError> {
        let list = if archived {
            ChatList::Archive
        } else {
            ChatList::Main
        };
        functions::set_pinned_chats(list, chat_ids, self.client_id)
            .await
            .inspect_err(|e| tracing::error!("Failed to reorder pinned chats: {e:?}"))
    }

    /// Reads a chat up to `last_message_id` (0: nothing to read) with its
    /// mentions, or marks it as unread.
    pub async fn set_chat_read(
        &self,
        chat_id: i64,
        read: bool,
        last_message_id: i64,
    ) -> Result<(), TdError> {
        if !read {
            return functions::toggle_chat_is_marked_as_unread(chat_id, true, self.client_id)
                .await
                .inspect_err(|e| tracing::error!("Failed to mark chat {chat_id} unread: {e:?}"));
        }
        if last_message_id != 0 {
            functions::view_messages(chat_id, vec![last_message_id], None, true, self.client_id)
                .await
                .inspect_err(|e| tracing::error!("Failed to read chat {chat_id}: {e:?}"))?;
        }
        functions::read_all_chat_mentions(chat_id, self.client_id)
            .await
            .inspect_err(|e| tracing::error!("Failed to read mentions of {chat_id}: {e:?}"))?;
        functions::toggle_chat_is_marked_as_unread(chat_id, false, self.client_id)
            .await
            .inspect_err(|e| tracing::error!("Failed to mark chat {chat_id} read: {e:?}"))
    }

    /// Leaves a group or channel.
    pub async fn leave_chat(&self, chat_id: i64) -> Result<(), TdError> {
        functions::leave_chat(chat_id, self.client_id)
            .await
            .inspect_err(|e| tracing::error!("Failed to leave chat {chat_id}: {e:?}"))
    }

    /// Deletes a chat's history for you and removes it from the chat list.
    pub async fn delete_chat(&self, chat_id: i64) -> Result<(), TdError> {
        functions::delete_chat_history(chat_id, true, false, self.client_id)
            .await
            .inspect_err(|e| tracing::error!("Failed to delete chat {chat_id}: {e:?}"))
    }

    /// Deletes every message of a chat, keeping it in the chat list: for
    /// you, or for everyone in it.
    pub async fn clear_chat_history(
        &self,
        chat_id: i64,
        for_everyone: bool,
    ) -> Result<(), TdError> {
        functions::delete_chat_history(chat_id, false, for_everyone, self.client_id)
            .await
            .inspect_err(|e| tracing::error!("Failed to clear the history of {chat_id}: {e:?}"))
    }

    /// The chat of a public username (without `@`): a person's, bot's,
    /// group's or channel's. It's added to the store if TDLib hadn't sent it.
    pub async fn search_public_chat(&self, username: &str) -> Result<i64, TdError> {
        let enums::Chat::Chat(chat) =
            functions::search_public_chat(username.to_owned(), self.client_id)
                .await
                .inspect_err(|e| tracing::warn!("No public chat @{username}: {e:?}"))?;
        let chat_id = chat.id;
        self.store.chats().entry(chat_id).or_insert(chat);
        Ok(chat_id)
    }

    /// Puts someone on your block list, or takes them off it.
    pub async fn set_sender_blocked(
        &self,
        sender: TdMessageSender,
        blocked: bool,
    ) -> Result<(), TdError> {
        let block_list = blocked.then_some(BlockList::Main);
        functions::set_message_sender_block_list(sender.into(), block_list, self.client_id)
            .await
            .inspect_err(|e| tracing::error!("Failed to block {sender:?}: {e:?}"))
    }

    /// Blocks or unblocks the person or bot of a private or secret chat.
    pub async fn set_chat_blocked(&self, chat_id: i64, blocked: bool) -> Result<(), TdError> {
        match self.store.profile_source(chat_id) {
            Some(ProfileSource::User(user_id)) => {
                self.set_sender_blocked(TdMessageSender::User(user_id), blocked)
                    .await
            }
            _ => Ok(()),
        }
    }

    /// Everyone on your block list, newest first, with their names.
    pub async fn blocked_senders(&self) -> Result<Vec<BlockedSender>, TdError> {
        let mut senders = Vec::new();
        loop {
            let enums::MessageSenders::MessageSenders(page) =
                functions::get_blocked_message_senders(
                    BlockList::Main,
                    senders.len() as i32,
                    BLOCKED_BATCH,
                    self.client_id,
                )
                .await
                .inspect_err(|e| tracing::error!("Failed to list blocked senders: {e:?}"))?;
            let done = page.senders.is_empty();
            senders.extend(page.senders.iter().map(TdMessageSender::from));
            if done || senders.len() >= page.total_count as usize {
                break;
            }
        }
        let mut blocked = Vec::with_capacity(senders.len());
        for sender in senders {
            blocked.push(BlockedSender {
                sender,
                name: self.sender_name(sender).await,
            });
        }
        Ok(blocked)
    }

    /// A user's or chat's name, asking TDLib for ones not known yet.
    async fn sender_name(&self, sender: TdMessageSender) -> String {
        match sender {
            TdMessageSender::User(user_id) if !self.store.users().contains_key(&user_id) => {
                if let Ok(User::User(user)) = functions::get_user(user_id, self.client_id).await {
                    self.store.users().insert(user_id, user);
                }
            }
            TdMessageSender::Chat(chat_id) if !self.store.chats().contains_key(&chat_id) => {
                if let Ok(enums::Chat::Chat(chat)) =
                    functions::get_chat(chat_id, self.client_id).await
                {
                    self.store.chats().entry(chat_id).or_insert(chat);
                }
            }
            _ => {}
        }
        self.store
            .sender_name(sender)
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| match sender {
                TdMessageSender::User(id) | TdMessageSender::Chat(id) => format!("#{id}"),
            })
    }

    /// Takes a step reporting a chat, or `message_ids` in it: an empty
    /// `option_id` starts, then each answers the step before.
    pub async fn report_chat(
        &self,
        chat_id: i64,
        option_id: &str,
        message_ids: Vec<i64>,
        text: &str,
    ) -> Result<ReportStep, TdError> {
        functions::report_chat(
            chat_id,
            option_id.to_owned(),
            message_ids,
            text.to_owned(),
            self.client_id,
        )
        .await
        .map(ReportStep::from)
        .inspect_err(|e| tracing::error!("Failed to report chat {chat_id}: {e:?}"))
    }

    /// Forwards messages of `from_chat_id` to `chat_id` (to a forum's
    /// General topic). With `send_copy`, they're sent as new messages
    /// without the original author. Returns how many TDLib couldn't forward.
    pub async fn forward_messages(
        &self,
        chat_id: i64,
        from_chat_id: i64,
        mut message_ids: Vec<i64>,
        send_copy: bool,
    ) -> Result<usize, TdError> {
        // TDLib wants them in increasing order
        message_ids.sort_unstable();
        message_ids.dedup();
        let enums::Messages::Messages(sent) = functions::forward_messages(
            chat_id,
            None,
            from_chat_id,
            message_ids,
            None,
            send_copy,
            false,
            self.client_id,
        )
        .await
        .inspect_err(|e| tracing::error!("Failed to forward to {chat_id}: {e:?}"))?;
        Ok(sent.messages.iter().filter(|m| m.is_none()).count())
    }

    /// Your active sessions, in the order to show them (this app's first).
    pub async fn active_sessions(&self) -> Result<Vec<SessionEntry>, TdError> {
        let enums::Sessions::Sessions(sessions) = functions::get_active_sessions(self.client_id)
            .await
            .inspect_err(|e| tracing::error!("Failed to get active sessions: {e:?}"))?;
        Ok(session_entries(&sessions.sessions))
    }

    /// Logs another device or app out of your account.
    pub async fn terminate_session(&self, session_id: i64) -> Result<(), TdError> {
        functions::terminate_session(session_id, self.client_id)
            .await
            .inspect_err(|e| tracing::error!("Failed to terminate session {session_id}: {e:?}"))
    }

    /// Logs every other device and app out of your account.
    pub async fn terminate_other_sessions(&self) -> Result<(), TdError> {
        functions::terminate_all_other_sessions(self.client_id)
            .await
            .inspect_err(|e| tracing::error!("Failed to terminate other sessions: {e:?}"))
    }

    /// The disk space Telegram's files and database take, with the chats
    /// that have the most files.
    pub async fn storage_usage(&self) -> Result<StorageUsage, TdError> {
        let enums::StorageStatisticsFast::StorageStatisticsFast(fast) =
            functions::get_storage_statistics_fast(self.client_id)
                .await
                .inspect_err(|e| tracing::error!("Failed to get storage statistics: {e:?}"))?;
        let enums::StorageStatistics::StorageStatistics(stats) =
            functions::get_storage_statistics(STORAGE_CHAT_LIMIT, self.client_id)
                .await
                .inspect_err(|e| tracing::error!("Failed to get storage statistics: {e:?}"))?;
        Ok(StorageUsage::new(&fast, &stats, |chat_id| {
            self.store.name_from_chats(chat_id)
        }))
    }

    /// Deletes cached files: those of `chat_id` only when given, else all of
    /// them. Returns the bytes freed.
    pub async fn clear_cache(&self, chat_id: Option<i64>) -> Result<i64, TdError> {
        let enums::StorageStatistics::StorageStatistics(deleted) = functions::optimize_storage(
            0,
            0,
            0,
            0,
            Vec::new(),
            chat_id.into_iter().collect(),
            Vec::new(),
            true,
            0,
            self.client_id,
        )
        .await
        .inspect_err(|e| tracing::error!("Failed to clear the cache: {e:?}"))?;
        Ok(deleted.size)
    }

    /// Trims the file cache to `max_size` bytes and deletes files unused for
    /// `max_age` seconds, as TDLib does on its own; 0 leaves either alone.
    pub async fn trim_cache(&self, max_size: i64, max_age: i32) -> Result<i64, TdError> {
        // TDLib's defaults (-1) would trim by the other limits too
        let size = if max_size > 0 { max_size } else { i64::MAX };
        let ttl = if max_age > 0 { max_age } else { i32::MAX };
        let enums::StorageStatistics::StorageStatistics(deleted) = functions::optimize_storage(
            size,
            ttl,
            i32::MAX,
            -1,
            Vec::new(),
            Vec::new(),
            Vec::new(),
            true,
            0,
            self.client_id,
        )
        .await
        .inspect_err(|e| tracing::error!("Failed to trim the cache: {e:?}"))?;
        Ok(deleted.size)
    }

    /// Confirms a new login as yours.
    pub async fn confirm_session(&self, session_id: i64) -> Result<(), TdError> {
        functions::confirm_session(session_id, self.client_id)
            .await
            .inspect_err(|e| tracing::error!("Failed to confirm session {session_id}: {e:?}"))
    }

    /// Saves one change to your own profile. TDLib then sends the updated
    /// user or full info, which the store picks up.
    pub async fn change_profile(&self, change: &ProfileChange) -> Result<(), TdError> {
        let id = self.client_id;
        match change {
            ProfileChange::Name {
                first_name,
                last_name,
            } => functions::set_name(first_name.clone(), last_name.clone(), id).await,
            ProfileChange::Bio(bio) => functions::set_bio(bio.clone(), id).await,
            ProfileChange::Username(username) => {
                functions::set_username(username.clone(), id).await
            }
        }
        .inspect_err(|e| tracing::warn!("Failed to change the profile ({change:?}): {e:?}"))
    }

    /// Loads the full info a chat's profile shows (bio, description) into the
    /// store, with the user if it isn't known yet.
    pub async fn load_profile(&self, chat_id: i64) -> Result<(), TdError> {
        let Some(source) = self.store.profile_source(chat_id) else {
            return Ok(());
        };
        let log = |e: &TdError| tracing::error!("Failed to load profile of {chat_id}: {e:?}");
        match source {
            ProfileSource::User(user_id) => {
                if !self.store.users().contains_key(&user_id) {
                    let User::User(user) = functions::get_user(user_id, self.client_id)
                        .await
                        .inspect_err(log)?;
                    self.store.users().insert(user_id, user);
                }
                let enums::UserFullInfo::UserFullInfo(info) =
                    functions::get_user_full_info(user_id, self.client_id)
                        .await
                        .inspect_err(log)?;
                self.store.users_full_info().insert(user_id, info);
            }
            ProfileSource::BasicGroup(group_id) => {
                let enums::BasicGroupFullInfo::BasicGroupFullInfo(info) =
                    functions::get_basic_group_full_info(group_id, self.client_id)
                        .await
                        .inspect_err(log)?;
                self.store.basic_groups_full_info().insert(group_id, info);
            }
            ProfileSource::Supergroup(group_id) => {
                let enums::SupergroupFullInfo::SupergroupFullInfo(info) =
                    functions::get_supergroup_full_info(group_id, self.client_id)
                        .await
                        .inspect_err(log)?;
                self.store.supergroups_full_info().insert(group_id, info);
            }
        }
        Ok(())
    }

    /// The topics of a forum chat, in topic list order.
    pub async fn get_forum_topics(&self, chat_id: i64) -> Result<Vec<TopicEntry>, TdError> {
        let mut topics: Vec<TopicEntry> = Vec::new();
        let (mut offset_date, mut offset_message_id, mut offset_topic_id) = (0, 0, 0);
        while topics.len() < MAX_TOPICS {
            let ForumTopics::ForumTopics(found) = functions::get_forum_topics(
                chat_id,
                String::new(),
                offset_date,
                offset_message_id,
                offset_topic_id,
                TOPICS_BATCH,
                self.client_id,
            )
            .await
            .inspect_err(|e| tracing::error!("Failed to get forum topics: {e:?}"))?;
            let next = (
                found.next_offset_date,
                found.next_offset_message_id,
                found.next_offset_forum_topic_id,
            );
            let done = found.topics.is_empty()
                || next == (offset_date, offset_message_id, offset_topic_id)
                || topics.len() + found.topics.len() >= found.total_count as usize;
            topics.extend(found.topics.iter().map(TopicEntry::from));
            if done {
                break;
            }
            (offset_date, offset_message_id, offset_topic_id) = next;
        }
        Ok(topics)
    }

    /// Fetches one batch of chat history, or of a forum topic's history.
    /// `offset` 0 means `from_message_id` and older; a negative offset includes
    /// newer messages (TDLib requires `limit >= -offset`).
    /// Returns the entries and whether more may follow.
    pub async fn get_chat_history_batch(
        &self,
        chat_id: i64,
        topic_id: Option<i32>,
        from_message_id: i64,
        offset: i32,
        limit: i32,
    ) -> (Vec<MessageEntry>, bool) {
        let history = match topic_id {
            Some(topic_id) => {
                functions::get_forum_topic_history(
                    chat_id,
                    topic_id,
                    from_message_id,
                    offset,
                    limit,
                    self.client_id,
                )
                .await
            }
            None => {
                functions::get_chat_history(
                    chat_id,
                    from_message_id,
                    offset,
                    limit,
                    false,
                    self.client_id,
                )
                .await
            }
        };
        match history {
            Ok(Messages::Messages(messages)) => {
                let entries: Vec<MessageEntry> = messages
                    .messages
                    .into_iter()
                    .flatten()
                    .map(|m| MessageEntry::from(&m))
                    .collect();
                let has_more = entries.len() >= limit as usize;
                (entries, has_more)
            }
            Err(e) => {
                tracing::error!("Failed to get chat history: {e:?}");
                (Vec::new(), false)
            }
        }
    }

    /// Loads up to 100 older messages of the open chat into the store.
    pub async fn get_chat_history(&self, chat_id: i64) {
        const WINDOW: usize = 100;
        let start_len = self.store.open_chat_messages().len();

        while self.store.open_chat_messages().len() < start_len + WINDOW {
            let from_message_id = self.store.from_message_id();
            let (entries, has_more) = self
                .get_chat_history_batch(
                    chat_id,
                    self.store.open_topic_id(),
                    from_message_id,
                    0,
                    HISTORY_BATCH,
                )
                .await;
            let Some(last) = entries.last() else {
                tracing::info!("No more messages to get");
                break;
            };
            self.store.set_from_message_id(last.id());
            self.store.open_chat_messages().insert_messages(entries);
            if !has_more {
                break;
            }
        }
    }

    /// Server-side search in a chat, or in one of its topics.
    pub async fn search_chat_messages(
        &self,
        chat_id: i64,
        topic: Option<MessageTopic>,
        query: String,
        limit: i32,
    ) -> Result<Vec<MessageEntry>, TdError> {
        match functions::search_chat_messages(
            chat_id,
            topic, // None: all topics
            query,
            None, // any sender
            0,
            0,
            limit,
            Some(SearchMessagesFilter::Empty),
            self.client_id,
        )
        .await
        {
            Ok(FoundChatMessages::FoundChatMessages(found)) => {
                Ok(found.messages.iter().map(MessageEntry::from).collect())
            }
            Err(e) => {
                tracing::error!("search_chat_messages failed: {e:?}");
                Err(e)
            }
        }
    }

    /// All pinned messages in a chat or one of its topics (paginated server
    /// search), newest first.
    pub async fn fetch_all_pinned_messages(
        &self,
        chat_id: i64,
        topic: Option<MessageTopic>,
    ) -> Vec<MessageEntry> {
        let in_topic = topic.is_some();
        let mut by_id: HashMap<i64, MessageEntry> = HashMap::new();
        let mut from_message_id: i64 = 0;
        loop {
            match functions::search_chat_messages(
                chat_id,
                topic.clone(),
                String::new(),
                None,
                from_message_id,
                0,
                100,
                Some(SearchMessagesFilter::Pinned),
                self.client_id,
            )
            .await
            {
                Ok(FoundChatMessages::FoundChatMessages(found)) => {
                    by_id.extend(found.messages.iter().map(|m| (m.id, MessageEntry::from(m))));
                    let next = found.next_from_message_id;
                    if next == 0 || next == from_message_id {
                        break;
                    }
                    from_message_id = next;
                }
                Err(e) => {
                    tracing::error!("fetch_all_pinned_messages failed: {e:?}");
                    break;
                }
            }
        }
        // The chat's pinned message may be in another topic
        if by_id.is_empty() && !in_topic {
            match functions::get_chat_pinned_message(chat_id, self.client_id).await {
                Ok(enums::Message::Message(m)) => {
                    by_id.insert(m.id, MessageEntry::from(&m));
                }
                Err(e) => {
                    tracing::debug!(chat_id, error = ?e, "no pinned message");
                }
            }
        }
        let mut pins: Vec<MessageEntry> = by_id.into_values().collect();
        pins.sort_by_key(|m| std::cmp::Reverse(m.id()));
        pins
    }

    /// Sends a text message, to a forum topic when `topic` is given, and
    /// clears the pending reply.
    pub async fn send_message(
        &self,
        message: String,
        chat_id: i64,
        topic: Option<MessageTopic>,
        reply_to: Option<TdMessageReplyToMessage>,
    ) -> Result<tdlib_rs::types::Message, TdError> {
        self.store.clear_reply_message();
        let reply_to = reply_to.map(|r| InputMessageReplyTo::Message((&r).into()));
        let options = self.send_options();
        match functions::send_message(
            chat_id,
            topic,
            reply_to,
            options,
            text_content(message),
            self.client_id,
        )
        .await
        {
            Ok(enums::Message::Message(message)) => Ok(message),
            Err(e) => {
                tracing::error!("Failed to send message: {e:?}");
                Err(e)
            }
        }
    }

    /// The options of a message being sent: without sound when asked for
    /// (once).
    fn send_options(&self) -> Option<tdlib_rs::types::MessageSendOptions> {
        self.store
            .take_send_silently()
            .then(|| tdlib_rs::types::MessageSendOptions {
                disable_notification: true,
                ..Default::default()
            })
    }

    /// Sends a local file with `caption` (none if empty): an image as a
    /// photo with [`SendAs::Photos`], anything else as a document. To a forum
    /// topic when `topic` is given and as a reply when `reply_to` is; clears
    /// the pending reply.
    pub async fn send_file(
        &self,
        file_path: String,
        caption: String,
        chat_id: i64,
        topic: Option<MessageTopic>,
        reply_to: Option<TdMessageReplyToMessage>,
        send_as: SendAs,
    ) -> Result<tdlib_rs::types::Message, TdError> {
        self.store.clear_reply_message();
        let reply_to = reply_to.map(|r| InputMessageReplyTo::Message((&r).into()));
        let content = file_content(file_path, &caption, send_as);
        let options = self.send_options();
        match functions::send_message(chat_id, topic, reply_to, options, content, self.client_id)
            .await
        {
            Ok(enums::Message::Message(message)) => Ok(message),
            Err(e) => {
                tracing::error!("Failed to send a file: {e:?}");
                Err(e)
            }
        }
    }

    /// Sends local files as albums of at most [`ALBUM_SIZE`] (see
    /// [`send_groups`]): the first file has `caption` (none if empty) and the
    /// first album answers `reply_to`. A last single file goes on its own.
    /// Clears the pending reply. Every group is tried; the first error is
    /// returned.
    pub async fn send_album(
        &self,
        file_paths: Vec<String>,
        caption: String,
        chat_id: i64,
        topic: Option<MessageTopic>,
        reply_to: Option<TdMessageReplyToMessage>,
        send_as: SendAs,
    ) -> Result<(), TdError> {
        let mut caption = Some(caption);
        let mut reply_to = reply_to;
        let mut first_error = None;
        // Every album of the files goes without sound, or none
        let silent = self.store.take_send_silently();
        for group in send_groups(&file_paths, send_as) {
            let caption = caption.take().unwrap_or_default();
            let reply = reply_to.take();
            self.store.set_send_silently(silent);
            let options = self.send_options();
            let sent = if let [path] = group.as_slice() {
                self.store.set_send_silently(silent);
                self.send_file(
                    path.clone(),
                    caption,
                    chat_id,
                    topic.clone(),
                    reply,
                    send_as,
                )
                .await
                .map(|_| ())
            } else {
                self.store.clear_reply_message();
                let reply = reply.map(|r| InputMessageReplyTo::Message((&r).into()));
                let contents = group
                    .iter()
                    .enumerate()
                    .map(|(index, path)| {
                        file_content(
                            path.clone(),
                            if index == 0 { &caption } else { "" },
                            send_as,
                        )
                    })
                    .collect();
                functions::send_message_album(
                    chat_id,
                    topic.clone(),
                    reply,
                    options.clone(),
                    contents,
                    self.client_id,
                )
                .await
                .map(|_| ())
                .inspect_err(|e| tracing::error!("Failed to send an album: {e:?}"))
            };
            if let Err(e) = sent {
                first_error.get_or_insert(e);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    /// Sends a sticker, to a forum topic when `topic` is given, and clears
    /// the pending reply.
    pub async fn send_sticker(
        &self,
        sticker: &StickerEntry,
        chat_id: i64,
        topic: Option<MessageTopic>,
        reply_to: Option<TdMessageReplyToMessage>,
    ) -> Result<tdlib_rs::types::Message, TdError> {
        self.store.clear_reply_message();
        let reply_to = reply_to.map(|r| InputMessageReplyTo::Message((&r).into()));
        let content = InputMessageContent::InputMessageSticker(InputMessageSticker {
            sticker: InputSticker {
                sticker: enums::InputFile::Id(InputFileId {
                    id: sticker.file_id,
                }),
                thumbnail: None,
                width: sticker.width,
                height: sticker.height,
            },
            emoji: sticker.emoji.clone(),
        });
        let options = self.send_options();
        match functions::send_message(chat_id, topic, reply_to, options, content, self.client_id)
            .await
        {
            Ok(enums::Message::Message(message)) => Ok(message),
            Err(e) => {
                tracing::error!("Failed to send sticker: {e:?}");
                Err(e)
            }
        }
    }

    /// The installed sticker sets, without their stickers, and before them
    /// the recently sent stickers when there are any.
    pub async fn get_sticker_sets(&self) -> Result<Vec<StickerSetEntry>, TdError> {
        let StickerSets::StickerSets(installed) =
            functions::get_installed_sticker_sets(StickerType::Regular, self.client_id)
                .await
                .inspect_err(|e| tracing::error!("Failed to get sticker sets: {e:?}"))?;
        let recent = match functions::get_recent_stickers(false, self.client_id).await {
            Ok(enums::Stickers::Stickers(recent)) => recent.stickers,
            Err(e) => {
                tracing::error!("Failed to get recent stickers: {e:?}");
                Vec::new()
            }
        };
        let recent = (!recent.is_empty())
            .then(|| StickerSetEntry::recent(recent.iter().map(StickerEntry::from).collect()));
        Ok(recent
            .into_iter()
            .chain(installed.sets.iter().map(StickerSetEntry::from))
            .collect())
    }

    /// A sticker set with its stickers.
    pub async fn get_sticker_set(&self, set_id: i64) -> Result<StickerSetEntry, TdError> {
        match functions::get_sticker_set(set_id, self.client_id).await {
            Ok(enums::StickerSet::StickerSet(set)) => Ok(StickerSetEntry::from(&set)),
            Err(e) => {
                tracing::error!("Failed to get sticker set {set_id}: {e:?}");
                Err(e)
            }
        }
    }

    /// Replaces the text of a message in the open chat, or the caption of a
    /// media message (an empty one removes it).
    pub async fn edit_message(&self, message_id: i64, message: String) -> Result<(), TdError> {
        let chat_id = self.store.open_chat_id().as_i64();
        let is_media = self
            .store
            .get_message(message_id)
            .is_some_and(|entry| entry.media().is_some());
        let edited = if is_media {
            let caption = (!message.trim().is_empty()).then(|| FormattedText {
                text: message,
                entities: vec![],
            });
            functions::edit_message_caption(chat_id, message_id, caption, false, self.client_id)
                .await
        } else {
            functions::edit_message_text(chat_id, message_id, text_content(message), self.client_id)
                .await
        };
        match edited {
            Ok(_) => {
                tracing::info!("Message edited");
                Ok(())
            }
            Err(e) => {
                tracing::error!("Failed to edit message: {e:?}");
                Err(e)
            }
        }
    }

    pub async fn delete_messages(&self, chat_id: i64, message_ids: Vec<i64>, revoke: bool) {
        match functions::delete_messages(chat_id, message_ids, revoke, self.client_id).await {
            Ok(()) => tracing::info!("Messages deleted"),
            Err(e) => tracing::error!("Failed to delete messages: {e:?}"),
        }
    }

    /// Emoji reactions that can be added to a message, the most used first.
    /// Custom emoji and paid reactions are left out.
    pub async fn available_reactions(
        &self,
        chat_id: i64,
        message_id: i64,
    ) -> Result<Vec<String>, TdError> {
        let enums::AvailableReactions::AvailableReactions(available) =
            functions::get_message_available_reactions(chat_id, message_id, 8, self.client_id)
                .await?;
        if available.unavailability_reason.is_some() {
            return Err(TdError {
                code: 400,
                message: "reactions aren't available to you here".to_owned(),
            });
        }
        let mut emoji: Vec<String> = Vec::new();
        for reaction in available
            .top_reactions
            .iter()
            .chain(&available.recent_reactions)
            .chain(&available.popular_reactions)
        {
            if let ReactionType::Emoji(reaction) = &reaction.r#type
                && !emoji.contains(&reaction.emoji)
            {
                emoji.push(reaction.emoji.clone());
            }
        }
        Ok(emoji)
    }

    /// Adds or removes our emoji reaction on a message.
    pub async fn set_reaction(
        &self,
        chat_id: i64,
        message_id: i64,
        emoji: String,
        add: bool,
    ) -> Result<(), TdError> {
        let reaction = ReactionType::Emoji(ReactionTypeEmoji { emoji });
        if add {
            functions::add_message_reaction(
                chat_id,
                message_id,
                reaction,
                false,
                true,
                self.client_id,
            )
            .await
        } else {
            functions::remove_message_reaction(chat_id, message_id, reaction, self.client_id).await
        }
    }

    /// Pins or unpins a message, for everyone in the chat.
    pub async fn set_pinned(
        &self,
        chat_id: i64,
        message_id: i64,
        pinned: bool,
    ) -> Result<(), TdError> {
        if pinned {
            functions::pin_chat_message(chat_id, message_id, false, false, self.client_id).await
        } else {
            functions::unpin_chat_message(chat_id, message_id, self.client_id).await
        }
    }

    /// An HTTPS link to a message; only supergroups and channels have them.
    pub async fn message_link(&self, chat_id: i64, message_id: i64) -> Result<String, TdError> {
        let enums::MessageLink::MessageLink(link) = functions::get_message_link(
            chat_id,
            message_id,
            0,
            0,
            String::new(),
            false,
            false,
            self.client_id,
        )
        .await?;
        Ok(link.link)
    }

    pub async fn use_quick_ack(&self) {
        self.set_bool_option("use_quick_ack", true).await;
    }

    pub async fn online(&self) {
        self.set_bool_option("online", true).await;
    }

    pub async fn offline(&self) {
        self.set_bool_option("online", false).await;
    }

    pub async fn disable_animated_emoji(&self, disable: bool) {
        self.set_bool_option("disable_animated_emoji", disable)
            .await;
    }

    /// Asks TDLib to announce new messages worth a notification as
    /// [`Notification::NewMessage`], honoring the chats' notification settings.
    pub async fn enable_notifications(&self) {
        match functions::set_option(
            "notification_group_count_max".to_owned(),
            Some(OptionValue::Integer(OptionValueInteger {
                value: NOTIFICATION_GROUP_COUNT_MAX,
            })),
            self.client_id,
        )
        .await
        {
            Ok(()) => tracing::info!("Enabled notifications"),
            Err(e) => tracing::error!("Failed to enable notifications: {e:?}"),
        }
    }

    async fn set_bool_option(&self, name: &str, value: bool) {
        match functions::set_option(
            name.to_owned(),
            Some(OptionValue::Boolean(OptionValueBoolean { value })),
            self.client_id,
        )
        .await
        {
            Ok(()) => tracing::info!("Set option {name} = {value}"),
            Err(e) => tracing::error!("Failed to set option {name}: {e:?}"),
        }
    }
}

/// Downloads files; cheap to copy into background tasks.
#[derive(Debug, Clone, Copy)]
pub struct Files {
    client_id: i32,
}

impl Files {
    /// Stops downloading a file, keeping what was downloaded to resume.
    pub async fn pause(&self, file_id: i32) -> Result<(), TdError> {
        functions::cancel_download_file(file_id, false, self.client_id)
            .await
            .inspect_err(|e| tracing::error!("Failed to pause download {file_id}: {e:?}"))
    }

    /// Stops downloading a file and deletes what was downloaded of it.
    pub async fn cancel(&self, file_id: i32) -> Result<(), TdError> {
        self.pause(file_id).await?;
        functions::delete_file(file_id, self.client_id)
            .await
            .inspect_err(|e| tracing::error!("Failed to delete file {file_id}: {e:?}"))
    }

    /// Downloads a file completely and returns its local path.
    pub async fn download(&self, file_id: i32) -> Result<String, TdError> {
        match functions::download_file(file_id, 32, 0, 0, true, self.client_id).await {
            Ok(enums::File::File(file)) => {
                tracing::info!("File downloaded: {:?}", file.local.path);
                Ok(file.local.path)
            }
            Err(e) => {
                tracing::error!("Failed to download file {file_id}: {e:?}");
                Err(e)
            }
        }
    }
}

/// Most files in one album.
pub const ALBUM_SIZE: usize = 10;

/// `paths` in albums of at most [`ALBUM_SIZE`], sized evenly so that none
/// is left with a single file when it can be avoided: 11 files are 6 and 5.
pub fn album_groups(paths: &[String]) -> Vec<&[String]> {
    if paths.is_empty() {
        return Vec::new();
    }
    let albums = paths.len().div_ceil(ALBUM_SIZE);
    let size = paths.len().div_ceil(albums);
    paths.chunks(size).collect()
}

/// How files that are images are sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SendAs {
    /// As documents, the images untouched.
    #[default]
    Files,
    /// Images as photos, compressed by Telegram; other files as documents.
    Photos,
}

/// Image formats Telegram takes as photos.
const PHOTO_EXTENSIONS: [&str; 4] = ["jpg", "jpeg", "png", "webp"];

/// Whether the file at `path` is an image Telegram can send as a photo, by
/// its extension.
pub fn is_photo(path: impl AsRef<std::path::Path>) -> bool {
    path.as_ref()
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            PHOTO_EXTENSIONS
                .iter()
                .any(|photo| ext.eq_ignore_ascii_case(photo))
        })
}

/// `paths` in the albums they are sent in, each sized by [`album_groups`].
/// An album can't mix photos with documents: with [`SendAs::Photos`], the
/// images come first in albums of their own, then the other files.
pub fn send_groups(paths: &[String], send_as: SendAs) -> Vec<Vec<String>> {
    let (photos, files): (Vec<String>, Vec<String>) = match send_as {
        SendAs::Files => (Vec::new(), paths.to_vec()),
        SendAs::Photos => paths.iter().cloned().partition(|path| is_photo(path)),
    };
    album_groups(&photos)
        .into_iter()
        .chain(album_groups(&files))
        .map(<[String]>::to_vec)
        .collect()
}

/// A local file with `caption`: as a photo when it's an image to send as
/// one, otherwise as a document.
fn file_content(file_path: String, caption: &str, send_as: SendAs) -> InputMessageContent {
    if send_as == SendAs::Photos && is_photo(&file_path) {
        photo_content(file_path, caption)
    } else {
        document_content(file_path, caption)
    }
}

/// `caption` as text, none if it's blank.
fn caption_text(caption: &str) -> Option<FormattedText> {
    (!caption.trim().is_empty()).then(|| FormattedText {
        text: caption.to_owned(),
        entities: vec![],
    })
}

/// A local image as a photo, with `caption` unless it's blank. Telegram
/// finds its size.
fn photo_content(file_path: String, caption: &str) -> InputMessageContent {
    InputMessageContent::InputMessagePhoto(InputMessagePhoto {
        photo: tdlib_rs::types::InputPhoto {
            photo: enums::InputFile::Local(InputFileLocal { path: file_path }),
            thumbnail: None,
            video: None,
            added_sticker_file_ids: Vec::new(),
            width: 0,
            height: 0,
        },
        caption: caption_text(caption),
        show_caption_above_media: false,
        self_destruct_type: None,
        has_spoiler: false,
    })
}

/// A local file as a document, with `caption` unless it's blank.
fn document_content(file_path: String, caption: &str) -> InputMessageContent {
    let caption = caption_text(caption);
    InputMessageContent::InputMessageDocument(InputMessageDocument {
        document: tdlib_rs::types::InputDocument {
            document: enums::InputFile::Local(InputFileLocal { path: file_path }),
            thumbnail: None,
            disable_content_type_detection: true,
        },
        caption,
    })
}

fn text_content(text: String) -> InputMessageContent {
    InputMessageContent::InputMessageText(InputMessageText {
        text: FormattedText {
            text,
            entities: vec![],
        },
        link_preview_options: None,
        clear_draft: true,
    })
}
