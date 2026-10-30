//! TDLib client: requests, the login flow and the update loop.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tdlib_rs::enums::{
    self, AuthorizationState, ChatList, FoundChatMessages, InputMessageContent,
    InputMessageReplyTo, LogStream, Messages, OptionValue, SearchMessagesFilter, User,
};
use tdlib_rs::functions;
use tdlib_rs::types::{
    FormattedText, InputFileLocal, InputMessageDocument, InputMessageText, LogStreamFile,
    OptionValueBoolean,
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;

use crate::model::{MessageEntry, TdMessageReplyToMessage};
use crate::store::Store;

mod auth;
mod execute;
mod updates;

#[cfg(test)]
mod tests;

pub use auth::{Authenticator, Proxy, ProxyKind, TdLogging, TdParams};
pub use execute::configure_logging;
pub use updates::Notification;

/// Errors returned by TDLib requests.
pub type TdError = tdlib_rs::types::Error;

/// Messages per `getChatHistory` request.
const HISTORY_BATCH: i32 = 50;

pub struct Client {
    client_id: i32,
    store: Arc<Store>,
    params: TdParams,
    auth_rx: UnboundedReceiver<AuthorizationState>,
    auth_tx: UnboundedSender<AuthorizationState>,
    have_authorization: bool,
    /// Whether the database was opened with [`TdParams::database_encryption_key`].
    database_encrypted: bool,
    can_quit: Arc<AtomicBool>,
    full_chats_list: bool,
    updates: Option<JoinHandle<()>>,
}

impl Client {
    /// Creates a TDLib client. Call [`Client::start`] before [`Client::authorize`].
    pub fn new(store: Arc<Store>, params: TdParams) -> Self {
        let (auth_tx, auth_rx) = tokio::sync::mpsc::unbounded_channel();
        let client_id = tdlib_rs::create_client();
        tracing::info!("Created TDLib client with client_id: {}", client_id);

        Self {
            client_id,
            store,
            params,
            auth_rx,
            auth_tx,
            have_authorization: false,
            database_encrypted: false,
            can_quit: Arc::new(AtomicBool::new(false)),
            full_chats_list: false,
            updates: None,
        }
    }

    pub fn store(&self) -> &Arc<Store> {
        &self.store
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

    /// A handle for downloading files that can be moved into other tasks.
    pub fn files(&self) -> Files {
        Files {
            client_id: self.client_id,
        }
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
                    let phone_number = auth.phone_number();
                    match functions::set_authentication_phone_number(
                        phone_number,
                        None,
                        self.client_id,
                    )
                    .await
                    {
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
            false,
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
        if self.full_chats_list {
            return;
        }
        // TDLib answers with a 404 once every chat is loaded
        if let Err(e) = functions::load_chats(Some(chat_list), limit, self.client_id).await {
            tracing::info!("Stopped loading chats: {e:?}");
            self.full_chats_list = true;
        }
    }

    pub async fn load_all_chats(&mut self) {
        while !self.full_chats_list {
            self.load_chats(ChatList::Main, HISTORY_BATCH).await;
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

    /// Fetches one batch of chat history. `offset` 0 means `from_message_id` and older;
    /// a negative offset includes newer messages (TDLib requires `limit >= -offset`).
    /// Returns the entries and whether more may follow.
    pub async fn get_chat_history_batch(
        &self,
        chat_id: i64,
        from_message_id: i64,
        offset: i32,
        limit: i32,
    ) -> (Vec<MessageEntry>, bool) {
        match functions::get_chat_history(
            chat_id,
            from_message_id,
            offset,
            limit,
            false,
            self.client_id,
        )
        .await
        {
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
                .get_chat_history_batch(chat_id, from_message_id, 0, HISTORY_BATCH)
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

    /// Server-side search in a chat.
    pub async fn search_chat_messages(
        &self,
        chat_id: i64,
        query: String,
        limit: i32,
    ) -> Result<Vec<MessageEntry>, TdError> {
        match functions::search_chat_messages(
            chat_id,
            None, // all topics
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

    /// All pinned messages in a chat (paginated server search), newest first.
    pub async fn fetch_all_pinned_messages(&self, chat_id: i64) -> Vec<MessageEntry> {
        let mut by_id: HashMap<i64, MessageEntry> = HashMap::new();
        let mut from_message_id: i64 = 0;
        loop {
            match functions::search_chat_messages(
                chat_id,
                None,
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
        if by_id.is_empty() {
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

    /// Sends a text message and clears the pending reply.
    pub async fn send_message(
        &self,
        message: String,
        chat_id: i64,
        reply_to: Option<TdMessageReplyToMessage>,
    ) -> Result<tdlib_rs::types::Message, TdError> {
        self.store.clear_reply_message();
        let reply_to = reply_to.map(|r| InputMessageReplyTo::Message((&r).into()));
        match functions::send_message(
            chat_id,
            None,
            reply_to,
            None,
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

    /// Sends a local file as a document and clears the pending reply.
    pub async fn send_document(
        &self,
        file_path: String,
        chat_id: i64,
    ) -> Result<tdlib_rs::types::Message, TdError> {
        self.store.clear_reply_message();
        let content = InputMessageContent::InputMessageDocument(InputMessageDocument {
            document: tdlib_rs::types::InputDocument {
                document: enums::InputFile::Local(InputFileLocal { path: file_path }),
                thumbnail: None,
                disable_content_type_detection: true,
            },
            caption: None,
        });
        match functions::send_message(chat_id, None, None, None, content, self.client_id).await {
            Ok(enums::Message::Message(message)) => Ok(message),
            Err(e) => {
                tracing::error!("Failed to send document: {e:?}");
                Err(e)
            }
        }
    }

    /// Replaces the text of a message in the open chat.
    pub async fn edit_message(&self, message_id: i64, message: String) {
        match functions::edit_message_text(
            self.store.open_chat_id().as_i64(),
            message_id,
            text_content(message),
            self.client_id,
        )
        .await
        {
            Ok(_) => tracing::info!("Message edited"),
            Err(e) => tracing::error!("Failed to edit message: {e:?}"),
        }
    }

    pub async fn delete_messages(&self, chat_id: i64, message_ids: Vec<i64>, revoke: bool) {
        match functions::delete_messages(chat_id, message_ids, revoke, self.client_id).await {
            Ok(()) => tracing::info!("Messages deleted"),
            Err(e) => tracing::error!("Failed to delete messages: {e:?}"),
        }
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
