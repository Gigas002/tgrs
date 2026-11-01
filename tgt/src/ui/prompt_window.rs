use crate::{
    app::action::{Action, Modifiers},
    app::context::AppContext,
    app::event::Event,
    settings::HighlightGroup,
    ui::component_name::ComponentName,
    ui::component_traits::{Component, HandleFocus},
    ui::{attachment, pane},
};
use arboard::Clipboard;
use crossterm::event::KeyCode;
use libtgt::model::td_enums::TdMessageReplyToMessage;
use ratatui::{
    Frame,
    layout::{Position, Rect},
    text::{Line, Span},
    widgets::Paragraph,
};
use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::mpsc::UnboundedSender;
use unicode_width::UnicodeWidthChar;

/// Longest text message Telegram accepts, in characters.
const MESSAGE_MAX_CHARS: usize = 4096;

/// `DirSelection` is an enum that represents the direction of the selection.
/// It is used to keep track of the direction of the selection when the user
/// is selecting text.
#[derive(Debug, Clone, Eq, PartialEq)]
enum DirSelection {
    /// The user is not selecting text.
    Empty,
    /// The user is selecting text to the left.
    Left,
    /// The user is selecting text to the right.
    Right,
    /// The user is selecting text up.
    Up,
    /// The user is selecting text down.
    Down,
}
/// `Mode` is an enum that represents the mode of the prompt.
/// It is used to keep track of the mode of the prompt.
#[cfg_attr(test, derive(PartialEq))]
pub(crate) enum Mode {
    /// The normal mode of the prompt.
    Normal,
    /// The edit mode of the prompt.
    /// Usually, when the prompt is editing a message.
    /// The parameter is the message id of the message that is being replied.
    Edit(i64),
    /// The reply mode of the prompt.
    /// Usually, when the prompt is replying to a message.
    /// The parameter is the message id of the message that is being replied.
    Reply(i64),
    /// The search mode of the prompt.
    /// Used to sort the chat list window based on the search string.
    SearchChatList,
    /// The search mode for chat messages.
    /// Used to filter and search messages within the current chat.
    SearchChatMessages,
}
/// `InputCell` is a struct that represents a cell of the input.
/// It is responsible for managing the input cell of the prompt.
#[derive(Debug, Clone, Eq, PartialEq)]
struct InputCell {
    /// The character of the input cell.
    c: char,
    /// A flag indicating whether the input cell is selected or not.
    selected: bool,
}
/// `Input` is a struct that represents the input of a prompt.
/// It is responsible for managing the input of the prompt.
struct Input {
    /// The text of the input.
    text: Vec<Vec<InputCell>>,
    /// The cursor position of the input.
    cursor: (usize, usize),
    /// The area of the component where the input is displayed.
    area_input: Rect,
    /// An unbounded sender that send action for processing.
    /// It is used to send actions to the main event loop for processing.
    /// Basically, it is used to send `IncreasePromptSize` and `DecreasePromptSize`
    /// actions.
    action_tx: Option<UnboundedSender<Action>>,
    /// An enum that represents the direction of the selection.
    /// Implicitly, it is used to keep track whether the user is selecting text
    /// or not.
    dir_selection: DirSelection,
    /// The correct prompt size.
    /// It is used to keep track of the correct prompt size when the prompt
    /// window is focused or unfocused.
    correct_prompt_size: usize,
    /// A flag indicating whether the prompt size is restored or not.
    /// Basically, it is used to keep track of whether the prompt size is
    /// equal to the correct prompt size or not.
    is_restored: bool,
    /// The mode of the input.
    /// It is used to keep track of the mode of the input.
    mode: Mode,
}
/// Implement the `Input` struct.
impl Input {
    /// Set the command sender of the `Input` struct.
    ///
    /// # Arguments
    /// * `command_tx` - An unbounded sender that send action for processing.
    fn set_command_tx(&mut self, command_tx: UnboundedSender<Action>) {
        self.action_tx = Some(command_tx);
    }
    /// Returns the rendered width of a line in terminal columns.
    fn line_display_width(line: &[InputCell]) -> usize {
        line.iter().map(|cell| cell.c.width().unwrap_or(0)).sum()
    }
    /// Get the cursor x position of the `Input` struct.
    fn cursor_x(&self) -> usize {
        Self::line_display_width(&self.text[self.cursor.1][..self.cursor.0])
    }
    /// Get the cursor y position of the `Input` struct.
    fn cursor_y(&self) -> usize {
        self.cursor.1
    }
    /// Get the text of the `Input` struct.
    fn text(&self) -> &Vec<Vec<InputCell>> {
        &self.text
    }
    /// Send search updates if in search mode.
    /// This method sends the current search string to the appropriate component.
    fn send_search_update(&mut self) {
        match self.mode {
            Mode::SearchChatList => {
                if let Some(tx) = self.action_tx.as_ref() {
                    let _ = tx.send(Action::ChatListSortWithString(self.text_to_string()));
                }
            }
            Mode::SearchChatMessages => {
                if let Some(tx) = self.action_tx.as_ref() {
                    let _ = tx.send(Action::ChatWindowSortWithString(self.text_to_string()));
                }
            }
            _ => {}
        }
    }
    /// Insert a character into the `Input` struct.
    /// The character is inserted at the current cursor position.
    /// If the cursor is at the end of the line, a new line is inserted.
    ///
    /// # Arguments
    /// * `c` - The character to insert.
    fn insert(&mut self, c: char) {
        // Subtract the two border columns and wrap by rendered width.
        let available_width = self.area_input.width.saturating_sub(2) as usize;
        let next_cell_width = c.width().unwrap_or(0);
        if available_width > 0 && self.cursor_x() + next_cell_width >= available_width {
            self.insert_newline();
        }
        self.text[self.cursor.1].insert(self.cursor.0, InputCell { c, selected: false });
        self.cursor.0 += 1;
        // Send search update if in search mode
        self.send_search_update();
    }
    /// Insert a newline into the `Input` struct.
    /// A newline is inserted at the current cursor position.
    /// The text after the cursor position is moved to the next line.
    fn insert_newline(&mut self) {
        // Split the current line at the cursor position
        let line = &mut self.text[self.cursor.1];
        let right = line[self.cursor.0..].to_vec();
        line.truncate(self.cursor.0);
        // Insert a new empty line after the current line
        self.text.insert(self.cursor.1 + 1, right);
        // Move cursor to the start of the new line
        self.cursor.0 = 0;
        self.cursor.1 += 1;
        // Increase the prompt size to accommodate the new line
        if let Some(tx) = self.action_tx.as_ref() {
            self.correct_prompt_size += 1;
            tx.send(Action::IncreasePromptSize).unwrap()
        }
    }
    /// Delete the character before the cursor position.
    fn backspace(&mut self) {
        if self.text[self.cursor.1].is_empty()
            && self.cursor.1 > 0
            && let Some(tx) = self.action_tx.as_ref()
        {
            self.correct_prompt_size -= 1;
            tx.send(Action::DecreasePromptSize).unwrap();
        }
        if self.cursor.0 == 0 {
            if self.cursor.1 > 0 {
                let line = self.text.remove(self.cursor.1);
                self.cursor.1 -= 1;
                self.cursor.0 = self.text[self.cursor.1].len();
                self.text[self.cursor.1].extend(line);
            }
        } else {
            self.text[self.cursor.1].remove(self.cursor.0 - 1);
            self.cursor.0 -= 1;
        }
        // Send search update if in search mode
        self.send_search_update();
    }
    /// Delete the character next to the cursor position.
    fn delete(&mut self) {
        if self.cursor.0 == self.text[self.cursor.1].len() {
            if self.cursor.1 < self.text.len() - 1 {
                let line = self.text.remove(self.cursor.1 + 1);
                self.text[self.cursor.1].extend(line);
            }
        } else {
            self.text[self.cursor.1].remove(self.cursor.0);
        }
        // Send search update if in search mode
        self.send_search_update();
    }
    /// Move the cursor to the left.
    fn move_cursor_left(&mut self) {
        if self.cursor.0 > 0 {
            self.cursor.0 -= 1;
        }
    }
    /// Move the cursor to the right.
    fn move_cursor_right(&mut self) {
        if self.cursor.0 < self.text[self.cursor.1].len() {
            self.cursor.0 += 1;
        }
    }
    /// Move the cursor up.
    fn move_cursor_up(&mut self) {
        if self.cursor.1 > 0 {
            // Handle moving cursor to the left if the current line is shorter
            // than the previous line.
            if self.cursor.0 > self.text[self.cursor.1 - 1].len() {
                self.cursor.0 = self.text[self.cursor.1 - 1].len();
            }
            self.cursor.1 -= 1;
        }
    }
    /// Move the cursor down.
    fn move_cursor_down(&mut self) {
        if self.cursor.1 < self.text.len() - 1 {
            // Handle moving cursor to the left if the current line is shorter
            // than the next line.
            if self.cursor.0 > self.text[self.cursor.1 + 1].len() {
                self.cursor.0 = self.text[self.cursor.1 + 1].len();
            }
            self.cursor.1 += 1;
        }
    }
    /// Move the cursor to the previous word.
    fn move_cursor_to_previous_word(&mut self) {
        let line = &self.text[self.cursor.1];
        let mut i = self.cursor.0;
        while i > 0 && line[i - 1].c.is_whitespace() {
            i -= 1;
        }
        while i > 0 && !line[i - 1].c.is_whitespace() {
            i -= 1;
        }
        self.cursor.0 = i;
    }
    /// Move the cursor to the next word.
    fn move_cursor_to_next_word(&mut self) {
        let line = &self.text[self.cursor.1];
        let mut i = self.cursor.0;
        while i < line.len() && line[i].c.is_whitespace() {
            i += 1;
        }
        while i < line.len() && !line[i].c.is_whitespace() {
            i += 1;
        }
        self.cursor.0 = i;
    }
    /// Move the cursor to the start of the line.
    fn move_cursor_to_end(&mut self) {
        self.cursor.0 = self.text[self.cursor.1].len();
    }
    /// Move the cursor to the end of the last line.
    fn move_cursor_to_text_end(&mut self) {
        let last = self.text.len().saturating_sub(1);
        self.cursor = (self.text.get(last).map_or(0, Vec::len), last);
    }
    /// Move the cursor to the end of the line.
    fn move_cursor_to_start(&mut self) {
        self.cursor.0 = 0;
    }
    /// Delete the previous word.
    fn delete_previous_word(&mut self) {
        let line = &mut self.text[self.cursor.1];
        let mut i = self.cursor.0;
        while i > 0 && line[i - 1].c.is_whitespace() {
            i -= 1;
        }
        while i > 0 && !line[i - 1].c.is_whitespace() {
            i -= 1;
        }
        line.drain(i..self.cursor.0);
        self.cursor.0 = i;
        // Send search update if in search mode
        self.send_search_update();
    }
    /// Restore the prompt size of the `Input` struct.
    /// It is used to restore the prompt size to the correct prompt size when
    /// the prompt window is focused.
    /// It sends `IncreasePromptSize` actions to the main event loop to increase
    /// the prompt size.
    fn restore_prompt_size(&mut self) {
        if !self.is_restored {
            if let Some(tx) = self.action_tx.as_ref() {
                for _ in 0..self.correct_prompt_size {
                    tx.send(Action::IncreasePromptSize).unwrap();
                }
            }
            self.is_restored = true;
        }
    }
    /// Set the prompt size to one.
    /// It is used to set the prompt size to one.
    fn set_prompt_size_to_one(&mut self) {
        if let Some(tx) = self.action_tx.as_ref() {
            for _ in 0..self.correct_prompt_size {
                tx.send(Action::DecreasePromptSize).unwrap();
            }
        }
    }
    /// Set the prompt size to one.
    /// It is used to set the prompt size to one when the prompt window is
    /// focused.
    /// Simply, it set the correct prompt size to zero and the cursor to the
    /// start of the line.
    fn set_prompt_size_to_one_focused(&mut self) {
        self.set_prompt_size_to_one();
        self.correct_prompt_size = 0;
        self.cursor = (0, 0);
    }
    /// Clear the input text and reset cursor and prompt size so the state is
    /// consistent (avoids cursor out of bounds when text is cleared).
    fn clear_and_reset_cursor(&mut self) {
        self.text = vec![vec![]];
        self.cursor = (0, 0);
        self.correct_prompt_size = 0;
    }
    /// Set the prompt size to one.
    /// It is used to set the prompt size to one when the prompt window is
    /// unfocused.
    /// It means that when the prompt window will return to focus, the prompt
    /// size will be restored to the correct prompt size.
    fn set_prompt_size_to_one_unfocused(&mut self) {
        self.set_prompt_size_to_one();
        self.is_restored = false;
    }
    /// Move the cursor to the left and toggle the selection.
    fn move_cursor_left_and_toggle_selection(&mut self) {
        if self.cursor.0 > 0 {
            self.cursor.0 -= 1;
            self.text[self.cursor.1][self.cursor.0].selected =
                !self.text[self.cursor.1][self.cursor.0].selected;
        }
        self.dir_selection = DirSelection::Left;
    }
    /// Move the cursor to the right and toggle the selection.
    fn move_cursor_right_and_toggle_selection(&mut self) {
        if self.cursor.0 < self.text[self.cursor.1].len() {
            self.text[self.cursor.1][self.cursor.0].selected =
                !self.text[self.cursor.1][self.cursor.0].selected;
            self.cursor.0 += 1;
        }
        self.dir_selection = DirSelection::Right;
    }
    // Move the cursor to the previous word and toggle the selection.
    fn move_cursor_to_previous_word_and_toggle_selection(&mut self) {
        let line = &self.text[self.cursor.1];
        let mut i = self.cursor.0;
        while i > 0 && line[i - 1].c.is_whitespace() {
            i -= 1;
        }
        while i > 0 && !line[i - 1].c.is_whitespace() {
            i -= 1;
        }
        for cell in &mut self.text[self.cursor.1][i..self.cursor.0] {
            cell.selected = !cell.selected;
        }
        self.cursor.0 = i;
        self.dir_selection = DirSelection::Left;
    }
    /// Move the cursor to the next word and toggle the selection.
    fn move_cursor_to_next_word_and_toggle_selection(&mut self) {
        let line = &self.text[self.cursor.1];
        let mut i = self.cursor.0;
        while i < line.len() && line[i].c.is_whitespace() {
            i += 1;
        }
        while i < line.len() && !line[i].c.is_whitespace() {
            i += 1;
        }
        for cell in &mut self.text[self.cursor.1][self.cursor.0..i] {
            cell.selected = !cell.selected;
        }
        self.cursor.0 = i;
        self.dir_selection = DirSelection::Right;
    }
    /// Move the cursor up and toggle the selection.
    fn move_cursor_up_and_toggle_selection(&mut self) {
        if self.cursor.1 > 0 {
            self.toggle_selection_from_cursor_to_start_of_line();
            self.move_cursor_up();
            self.toggle_selection_from_end_of_line_to_cursor();
            self.dir_selection = DirSelection::Up;
        }
    }
    /// Move the cursor down and toggle the selection.
    fn move_cursor_down_and_toggle_selection(&mut self) {
        if self.cursor.1 < self.text.len() - 1 {
            self.toggle_selection_from_cursor_to_end_of_line();
            self.move_cursor_down();
            self.toggle_selection_from_start_of_line_to_cursor();
            self.dir_selection = DirSelection::Down;
        }
    }
    /// Toggle the selection from the cursor to the start of the line.
    fn toggle_selection_from_cursor_to_start_of_line(&mut self) {
        for cell in &mut self.text[self.cursor.1][..self.cursor.0] {
            cell.selected = !cell.selected;
        }
    }
    /// Toggle the selection from the cursor to the end of the line.
    fn toggle_selection_from_cursor_to_end_of_line(&mut self) {
        for cell in &mut self.text[self.cursor.1][self.cursor.0..] {
            cell.selected = !cell.selected;
        }
    }
    /// Toggle the selection from the start of the line to the cursor.
    fn toggle_selection_from_start_of_line_to_cursor(&mut self) {
        for cell in &mut self.text[self.cursor.1][..self.cursor.0] {
            cell.selected = !cell.selected;
        }
    }
    /// Toggle the selection from the end of the line to the cursor.
    fn toggle_selection_from_end_of_line_to_cursor(&mut self) {
        for cell in &mut self.text[self.cursor.1][self.cursor.0..] {
            cell.selected = !cell.selected;
        }
    }
    /// Unselect all the text.
    fn unselect_all(&mut self) {
        if self.dir_selection != DirSelection::Empty {
            for line in &mut self.text {
                for cell in line {
                    cell.selected = false;
                }
            }
            self.dir_selection = DirSelection::Empty;
        }
    }
    /// Copy the selected text of the `Input` struct.
    /// The selected text is copied to the clipboard.
    fn copy_selected(&self) {
        if let Ok(mut clipboard) = Clipboard::new() {
            let mut text = String::new();
            for (i, line) in self.text.iter().enumerate() {
                for cell in line {
                    if cell.selected {
                        text.push(cell.c);
                    }
                }
                if i < self.text.len() - 1 {
                    text.push('\n');
                }
            }
            clipboard.set_text(text).unwrap();
        }
    }
    /// Paste text into the `Input` struct.
    /// The text is pasted at the current cursor position.
    fn paste(&mut self, text: String) {
        for c in text.chars() {
            if c == '\n' {
                self.insert_newline();
            } else {
                self.insert(c);
            }
        }
    }
    /// Edit a message.
    ///
    /// # Arguments
    /// * `message_id` - The message id of the message to edit.
    /// * `message` - The message to edit.
    fn edit_message(&mut self, message_id: i64, message: String) {
        self.mode = Mode::Edit(message_id);
        self.set_text(&message);
    }
    /// Replaces the text, lines split at `\n`, with the cursor at the start.
    fn set_text(&mut self, message: &str) {
        self.text = message
            .split('\n')
            .map(|line| {
                line.chars()
                    .map(|c| InputCell { c, selected: false })
                    .collect()
            })
            .collect();
        let extra_lines = self.text.len().saturating_sub(1);
        self.correct_prompt_size = extra_lines;
        // Let restore_prompt_size() send IncreasePromptSize on next focused draw,
        // so we only adjust size once and the prompt is correctly sized when shown.
        self.is_restored = false;
        self.cursor = (0, 0);
    }

    /// Send a message.
    /// The message is sent to the main event loop for processing.
    ///
    /// # Arguments
    /// * `app_context` - An Arc wrapped AppContext struct.
    fn send_message(&mut self, app_context: Arc<AppContext>) {
        let action_tx = app_context.action_tx();
        let composing = matches!(self.mode, Mode::Normal | Mode::Edit(_) | Mode::Reply(_));
        // An empty caption is sent to remove it
        let editing_caption = matches!(
            self.mode,
            Mode::Edit(id) if app_context.store().get_message(id).is_some_and(|m| m.media().is_some())
        );
        if composing && !editing_caption && self.text_to_string().trim().is_empty() {
            // Nothing to send: Enter on an empty prompt does nothing
            return;
        }
        {
            match self.mode {
                Mode::Normal => {
                    let _ = action_tx.send(Action::SendMessage(self.text_to_string(), None));
                    self.text = vec![vec![]];
                    self.set_prompt_size_to_one_focused();
                }
                Mode::Edit(message_id) => {
                    let _ = action_tx
                        .send(Action::SendMessageEdited(message_id, self.text_to_string()));
                    self.text = vec![vec![]];
                    self.set_prompt_size_to_one_focused();
                    self.mode = Mode::Normal;
                }
                Mode::Reply(message_id) => {
                    let _ = action_tx.send(Action::SendMessage(
                        self.text_to_string(),
                        Some(TdMessageReplyToMessage {
                            chat_id: 0, // This must be  0 and not `app_context.store().open_chat_id()` because the tdlib (maybe from the version 1.8.29 or before)  is able to know the chat id from the message_id; it will infer the chat id from the message_id.
                            message_id,
                        }),
                    ));
                    self.text = vec![vec![]];
                    self.set_prompt_size_to_one_focused();
                    self.mode = Mode::Normal;
                }
                Mode::SearchChatList => {
                    // NOTE: This should probably change in the future. Currently CoreWindow
                    // only propagates updates to the other components if they are in focus.
                    // FocusComponent is sent first to work around this problem.
                    //
                    // Ideally CoreWindow should propagate updates regardless of focus, that
                    // way components can handle their own state by themselves.
                    self.action_tx
                        .as_ref()
                        .unwrap()
                        .send(Action::FocusComponent(ComponentName::ChatList))
                        .unwrap();
                    self.action_tx
                        .as_ref()
                        .unwrap()
                        .send(Action::ChatListSortWithString(self.text_to_string()))
                        .unwrap();
                    self.text = vec![vec![]];
                    self.mode = Mode::Normal;
                    self.set_prompt_size_to_one_focused();
                }
                Mode::SearchChatMessages => {
                    // Open server search overlay and run search with prompt text
                    let query = self.text_to_string();
                    self.action_tx
                        .as_ref()
                        .unwrap()
                        .send(Action::ChatWindowSearch)
                        .unwrap();
                    if !query.is_empty() {
                        self.action_tx
                            .as_ref()
                            .unwrap()
                            .send(Action::SearchChatMessages(query))
                            .unwrap();
                    }
                    self.text = vec![vec![]];
                    self.mode = Mode::Normal;
                    self.set_prompt_size_to_one_focused();
                }
            }
        }
    }
    /// Convert the text of the `Input` struct to a string.
    /// Lines are joined with newlines so multiline input is sent as a single message.
    fn text_to_string(&self) -> String {
        // TODO: Parse into markdown
        self.text
            .iter()
            .map(|line| line.iter().map(|c| c.c).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }
}
/// Implement the `Default` trait for the `Input` struct.
impl Default for Input {
    fn default() -> Self {
        Self {
            text: vec![vec![]],
            cursor: (0, 0),
            area_input: Rect::default(),
            action_tx: None,
            dir_selection: DirSelection::Empty,
            correct_prompt_size: 0,
            is_restored: true,
            mode: Mode::Normal,
        }
    }
}
/// `PromptWindow` is a struct that represents a window for displaying a prompt.
/// It is responsible for managing the layout and rendering of the prompt
/// window.
pub struct PromptWindow {
    /// The application context.
    app_context: Arc<AppContext>,
    /// The name of the `PromptWindow`.
    name: String,
    /// An unbounded sender that send action for processing.
    action_tx: Option<UnboundedSender<Action>>,
    /// Indicates whether the `PromptWindow` is focused or not.
    focused: bool,
    /// The keys that allows the `PromptWindow` to be focused.
    focused_keys: Vec<Event>,
    /// The current input of the `PromptWindow`.
    input: Input,
    /// The chat (and forum topic) the text is the draft of.
    draft_target: Option<DraftTarget>,
    /// The draft as last loaded or saved, to tell whether it was edited.
    saved_draft: String,
    /// Files dropped or pasted into the prompt, sent with the next message.
    attachments: Vec<PathBuf>,
}

/// A path as text that pastes back as the same file: quoted when it has
/// spaces or quotes.
fn quote_path(path: &Path) -> String {
    let text = path.display().to_string();
    if text.contains(|c: char| c.is_whitespace() || c == '\'' || c == '"' || c == '\\') {
        format!("'{}'", text.replace('\'', r"'\''"))
    } else {
        text
    }
}

/// A chat, and a forum topic of it (none for the General topic), that has
/// its own draft.
type DraftTarget = (i64, Option<i32>);
/// Implement the `PromptWindow` struct.
impl PromptWindow {
    /// Create a new instance of the `PromptWindow` struct.
    ///
    /// # Arguments
    /// * `app_context` - An Arc wrapped AppContext struct.
    ///
    /// # Returns
    /// * `Self` - The new instance of the `PromptWindow` struct.
    pub fn new(app_context: Arc<AppContext>) -> Self {
        let name = "".to_string();
        let action_tx = None;
        let focused = false;
        // From the chat, where the hint is read; short keys like `i` first
        let mut focused_keys = app_context.keymap_config().get_key_of_single_action(
            ComponentName::Chat,
            Action::FocusComponent(ComponentName::Prompt),
        );
        focused_keys.sort_by_key(|key| {
            let name = key.to_string();
            (name.len(), name)
        });

        let input = Input::default();

        PromptWindow {
            app_context,
            name,
            action_tx,
            focused,
            focused_keys,
            input,
            draft_target: None,
            saved_draft: String::new(),
            attachments: Vec::new(),
        }
    }

    /// The chat and topic messages are written to now, if a chat is open.
    fn current_draft_target(&self) -> Option<DraftTarget> {
        let store = self.app_context.store();
        let chat_id = store.open_chat_id().as_i64();
        let topic_id = store
            .open_topic()
            .filter(|topic| !topic.is_general)
            .map(|topic| topic.topic_id);
        (chat_id != 0).then_some((chat_id, topic_id))
    }

    /// Saves the text as the draft of its chat if it was edited since it
    /// was loaded or last saved. A message being edited isn't a draft.
    fn save_draft(&mut self) {
        let Some((chat_id, topic_id)) = self.draft_target else {
            return;
        };
        if !matches!(self.input.mode, Mode::Normal | Mode::Reply(_)) {
            return;
        }
        let text = self.input.text_to_string();
        if text == self.saved_draft {
            return;
        }
        if let Some(tx) = self.action_tx.as_ref() {
            let _ = tx.send(Action::SaveDraft(chat_id, topic_id, text.clone()));
        }
        self.saved_draft = text;
    }

    /// Whether files can be attached now: not while editing or searching.
    fn can_attach(&self) -> bool {
        matches!(self.input.mode, Mode::Normal | Mode::Reply(_))
    }

    /// Sends files, the first with the text as its caption and as a reply
    /// when replying, and clears the text. While editing or searching, the
    /// text isn't a caption and stays.
    fn send_files(&mut self, paths: Vec<PathBuf>) {
        let Some((first, rest)) = paths.split_first() else {
            return;
        };
        let (caption, reply_to) = match self.input.mode {
            Mode::Normal => (self.input.text_to_string(), None),
            Mode::Reply(message_id) => (
                self.input.text_to_string(),
                // TDLib finds the chat from the message, like for text replies
                Some(TdMessageReplyToMessage {
                    chat_id: 0,
                    message_id,
                }),
            ),
            _ => (String::new(), None),
        };
        if matches!(self.input.mode, Mode::Normal | Mode::Reply(_)) {
            self.input.mode = Mode::Normal;
            self.input.set_text("");
            self.input.set_prompt_size_to_one_focused();
        }
        if let Some(tx) = self.action_tx.as_ref() {
            let path = |p: &PathBuf| p.display().to_string();
            let caption = caption.trim().to_owned();
            let _ = tx.send(if rest.is_empty() {
                Action::SendFile(path(first), caption, reply_to)
            } else {
                Action::SendAlbum(paths.iter().map(path).collect(), caption, reply_to)
            });
        }
        // The caption was the draft: it's sent now
        self.save_draft();
    }

    /// Puts the chat's draft back in the prompt, e.g. after editing a message.
    fn restore_draft(&mut self) {
        let draft = self.saved_draft.clone();
        self.input.set_text(&draft);
        self.input.move_cursor_to_text_end();
    }

    /// Keeps the text the draft of the open chat: switching chats (or
    /// topics) saves the old one's and loads the new one's, and a draft
    /// changed on another device is loaded unless the text was edited here.
    fn sync_draft(&mut self) {
        // Searching uses the prompt for the query, not a draft
        if matches!(
            self.input.mode,
            Mode::SearchChatList | Mode::SearchChatMessages
        ) {
            return;
        }
        let target = self.current_draft_target();
        if target != self.draft_target {
            self.save_draft();
            self.draft_target = target;
            self.saved_draft = target
                .map(|(chat_id, topic_id)| self.app_context.store().draft(chat_id, topic_id))
                .unwrap_or_default();
            self.input.mode = Mode::Normal;
            // Files attached for the old chat aren't sent to the new one
            self.attachments.clear();
            let draft = self.saved_draft.clone();
            self.input.set_text(&draft);
            self.input.move_cursor_to_text_end();
            return;
        }
        let Some((chat_id, topic_id)) = target else {
            return;
        };
        let unedited = matches!(self.input.mode, Mode::Normal)
            && self.input.text_to_string() == self.saved_draft;
        let stored = self.app_context.store().draft(chat_id, topic_id);
        if unedited && stored != self.saved_draft {
            self.input.set_text(&stored);
            self.input.move_cursor_to_text_end();
            self.saved_draft = stored;
        }
    }
    /// Set the name of the `PromptWindow`.
    ///
    /// # Arguments
    /// * `name` - The name of the `PromptWindow`
    ///
    /// # Returns
    /// * `Self` - The modified instance of the `PromptWindow`.
    pub fn with_name(mut self, name: impl AsRef<str>) -> Self {
        self.name = name.as_ref().to_string();
        self
    }
    /// Current prompt mode (for tests).
    #[cfg(test)]
    pub(crate) fn current_mode(&self) -> &Mode {
        &self.input.mode
    }

    fn reply_title(preview: &str) -> String {
        if preview.is_empty() {
            return "Replying to message".into();
        }

        const MAX_TITLE_CHARS: usize = 40;
        const PREVIEW_CHARS: usize = MAX_TITLE_CHARS - 1;

        let mut cutoff = None;
        for (char_index, (byte_index, _)) in preview.char_indices().enumerate() {
            if char_index == PREVIEW_CHARS {
                cutoff = Some(byte_index);
            }

            if char_index == MAX_TITLE_CHARS {
                let cutoff = cutoff.unwrap_or(byte_index);
                return format!("Replying to: {}...", &preview[..cutoff]);
            }
        }

        format!("Replying to: {preview}")
    }
    /// Update the input area of the `PromptWindow`.
    /// It is used to update the input area of the `PromptWindow` when a new
    /// line is inserted or deleted.
    ///
    /// # Arguments
    /// * `area_input` - The new input area of the `PromptWindow`.
    pub fn update_input(&mut self, area_input: Rect) {
        if self.input.area_input != area_input {
            self.input.area_input = area_input;
        }
    }
}

/// Implement the `HandleFocus` trait for the `PromptWindow` struct.
/// This trait allows the `PromptWindow` to be focused or unfocused.
impl HandleFocus for PromptWindow {
    /// Set the `focused` flag for the `PromptWindow`.
    fn focus(&mut self) {
        self.focused = true;
    }
    /// Set the `focused` flag for the `PromptWindow`.
    fn unfocus(&mut self) {
        self.focused = false;
        // Leaving the prompt keeps what was written as the chat's draft
        self.save_draft();
        match self.input.mode {
            Mode::SearchChatList => {
                if let Some(tx) = self.input.action_tx.as_ref() {
                    let _ = tx.send(Action::ChatListRestoreSort);
                }
                self.input.mode = Mode::Normal;
                self.input.clear_and_reset_cursor();
            }
            Mode::SearchChatMessages => {
                if let Some(tx) = self.input.action_tx.as_ref() {
                    let _ = tx.send(Action::ChatWindowRestoreSort);
                }
                self.input.mode = Mode::Normal;
                self.input.clear_and_reset_cursor();
            }
            // Leaving an edit cancels it: the draft comes back
            Mode::Edit(_) => {
                self.input.mode = Mode::Normal;
                self.restore_draft();
            }
            _ => {
                // Normal / Reply: preserve the draft; only reset the mode
                self.input.mode = Mode::Normal;
            }
        }
    }
}

/// Implement the `Component` trait for the `ChatListWindow` struct.
impl Component for PromptWindow {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> std::io::Result<()> {
        self.action_tx = Some(tx.clone());
        self.input.set_command_tx(tx);
        Ok(())
    }

    fn update(&mut self, action: Action) {
        match action {
            Action::PromptSendMessage if !self.attachments.is_empty() && self.can_attach() => {
                self.input.unselect_all();
                let files = std::mem::take(&mut self.attachments);
                self.send_files(files);
            }
            Action::PromptSendMessage => {
                self.input.unselect_all();
                let editing = matches!(self.input.mode, Mode::Edit(_));
                self.input.send_message(Arc::clone(&self.app_context));
                // The edit is sent: the draft comes back
                if editing && matches!(self.input.mode, Mode::Normal) {
                    self.restore_draft();
                }
            }
            Action::PromptCopy => {
                self.input.copy_selected();
                self.input.unselect_all();
            }
            Action::Key(key_code, modifiers) => match (key_code, modifiers) {
                // Esc turns attached files back into the text that was pasted
                (KeyCode::Esc, ..) if !self.attachments.is_empty() => {
                    let paths = std::mem::take(&mut self.attachments);
                    let text = paths
                        .iter()
                        .map(|path| quote_path(path))
                        .collect::<Vec<_>>()
                        .join(" ");
                    self.input.paste(text);
                }
                (KeyCode::Esc, ..) => {
                    // ESC cancels reply or edit mode and returns focus to chat:
                    // a reply keeps its text as the draft, an edit brings the
                    // draft back
                    if matches!(self.input.mode, Mode::Reply(_) | Mode::Edit(_)) {
                        if let Some(tx) = self.action_tx.as_ref() {
                            let _ = tx.send(Action::HideChatWindowReply);
                            let _ = tx.send(Action::FocusComponent(ComponentName::Chat));
                        }
                        let editing = matches!(self.input.mode, Mode::Edit(_));
                        self.input.mode = Mode::Normal;
                        if editing {
                            self.restore_draft();
                        }
                    }
                }
                (KeyCode::Home, ..)
                | (
                    KeyCode::Char('b'),
                    Modifiers {
                        shift: true,
                        super_: true,
                        ..
                    },
                )
                | (
                    KeyCode::Char('b'),
                    Modifiers {
                        control: true,
                        alt: true,
                        ..
                    },
                )
                | (KeyCode::Char('a'), Modifiers { control: true, .. }) => {
                    self.input.unselect_all();
                    self.input.move_cursor_to_start();
                }

                (KeyCode::End, ..)
                | (
                    KeyCode::Char('f'),
                    Modifiers {
                        shift: true,
                        super_: true,
                        ..
                    },
                )
                | (
                    KeyCode::Char('f'),
                    Modifiers {
                        control: true,
                        alt: true,
                        ..
                    },
                )
                | (KeyCode::Char('e'), Modifiers { control: true, .. }) => {
                    self.input.unselect_all();
                    self.input.move_cursor_to_end();
                }

                (
                    KeyCode::Left,
                    Modifiers {
                        shift: true,
                        control: true,
                        ..
                    },
                ) => {
                    self.input
                        .move_cursor_to_previous_word_and_toggle_selection();
                }

                (
                    KeyCode::Right,
                    Modifiers {
                        shift: true,
                        control: true,
                        ..
                    },
                ) => {
                    self.input.move_cursor_to_next_word_and_toggle_selection();
                }

                (KeyCode::Left, Modifiers { shift: true, .. }) => {
                    self.input.move_cursor_left_and_toggle_selection();
                }

                (KeyCode::Right, Modifiers { shift: true, .. }) => {
                    self.input.move_cursor_right_and_toggle_selection();
                }

                (KeyCode::Up, Modifiers { shift: true, .. }) => {
                    self.input.move_cursor_up_and_toggle_selection();
                }

                (KeyCode::Down, Modifiers { shift: true, .. }) => {
                    self.input.move_cursor_down_and_toggle_selection();
                }

                (KeyCode::Char('c'), Modifiers { control: true, .. }) => {
                    self.input.copy_selected();
                    self.input.unselect_all();
                }

                (KeyCode::Char('v'), Modifiers { control: true, .. }) => {
                    if let Ok(mut clipboard) = Clipboard::new()
                        && let Ok(text) = clipboard.get_text()
                    {
                        self.input.paste(text);
                    }
                }

                (KeyCode::Left | KeyCode::Char('b'), Modifiers { control: true, .. }) => {
                    self.input.unselect_all();
                    self.input.move_cursor_to_previous_word();
                }

                (KeyCode::Right | KeyCode::Char('f'), Modifiers { control: true, .. }) => {
                    self.input.unselect_all();
                    self.input.move_cursor_to_next_word();
                }

                // Ctrl+J is a newline, like Shift/Ctrl/Alt+Enter (Enter sends)
                (KeyCode::Char('j'), Modifiers { control: true, .. }) => {
                    self.input.unselect_all();
                    self.input.insert_newline();
                }

                (KeyCode::Backspace, Modifiers { control: true, .. })
                | (KeyCode::Char('h'), Modifiers { control: true, .. })
                | (KeyCode::Char('w'), Modifiers { control: true, .. }) => {
                    self.input.unselect_all();
                    self.input.delete_previous_word();
                }

                (
                    KeyCode::Char(c),
                    Modifiers {
                        alt: false,
                        control: false,
                        meta: false,
                        super_: false,
                        hyper: false,
                        ..
                    },
                ) => {
                    self.input.unselect_all();
                    self.input.insert(c);
                    // Not for edits, which Telegram doesn't announce
                    if matches!(self.input.mode, Mode::Normal | Mode::Reply(_))
                        && let Some(tx) = self.action_tx.as_ref()
                    {
                        let _ = tx.send(Action::Typing);
                    }
                }

                (KeyCode::Backspace, ..) => {
                    self.input.unselect_all();
                    self.input.backspace();
                }

                (KeyCode::Delete, ..) => {
                    self.input.unselect_all();
                    self.input.delete();
                }

                (KeyCode::Char('r'), Modifiers { alt: true, .. }) => {
                    // Alt+R from prompt: activate chat message search if in chat context
                    self.input.unselect_all();
                    if let Some(tx) = self.action_tx.as_ref() {
                        tx.send(Action::ChatWindowSearch).unwrap();
                    }
                }
                (KeyCode::Enter, ..) => {
                    self.input.unselect_all();
                    // In search mode, Enter exits search mode instead of inserting newline
                    match self.input.mode {
                        Mode::SearchChatList => {
                            self.action_tx
                                .as_ref()
                                .unwrap()
                                .send(Action::ChatListRestoreSort)
                                .unwrap();
                            self.input.text = vec![vec![]];
                            self.input.mode = Mode::Normal;
                            self.input.set_prompt_size_to_one_focused();
                        }
                        Mode::SearchChatMessages => {
                            self.action_tx
                                .as_ref()
                                .unwrap()
                                .send(Action::ChatWindowRestoreSort)
                                .unwrap();
                            self.input.text = vec![vec![]];
                            self.input.mode = Mode::Normal;
                            self.input.set_prompt_size_to_one_focused();
                        }
                        _ => {
                            self.input.insert_newline();
                        }
                    }
                }

                (KeyCode::Left, ..) => {
                    self.input.unselect_all();
                    self.input.move_cursor_left();
                }

                (KeyCode::Right, ..) => {
                    self.input.unselect_all();
                    self.input.move_cursor_right();
                }

                (KeyCode::Up, ..) => {
                    self.input.unselect_all();
                    self.input.move_cursor_up();
                }

                (KeyCode::Down, ..) => {
                    self.input.unselect_all();
                    self.input.move_cursor_down();
                }
                _ => {}
            },
            Action::Paste(text) => {
                self.input.unselect_all();
                match attachment::dropped_files(&text) {
                    Some(paths) if self.can_attach() => self.attachments.extend(paths),
                    _ => self.input.paste(text),
                }
            }
            Action::PromptToggleSilent => {
                let store = self.app_context.store();
                store.set_send_silently(!store.sends_silently());
            }
            Action::PromptPasteImage if self.can_attach() => {
                match attachment::save_clipboard_image() {
                    Ok(path) => self.attachments.push(path),
                    Err(message) => {
                        if let Some(tx) = self.action_tx.as_ref() {
                            let _ = tx.send(Action::StatusMessage(message));
                        }
                    }
                }
            }
            Action::EditMessage(message_id, message) => {
                self.input.edit_message(message_id, message);
            }
            // A picked file goes with the files attached before it
            Action::UploadFile(path) => {
                let mut files = if self.can_attach() {
                    std::mem::take(&mut self.attachments)
                } else {
                    Vec::new()
                };
                files.push(PathBuf::from(path));
                self.send_files(files);
            }
            Action::AttachFile(path) => {
                let message = if self.can_attach() {
                    self.attachments.push(PathBuf::from(path));
                    format!(
                        "Attached {}: enter in the prompt sends them",
                        attachment::names(&self.attachments)
                    )
                } else {
                    "Finish editing the message before attaching files.".to_owned()
                };
                if let Some(tx) = self.action_tx.as_ref() {
                    let _ = tx.send(Action::StatusMessage(message));
                }
            }
            // A sticker answered the message; the text stays
            Action::HideChatWindowReply if matches!(self.input.mode, Mode::Reply(_)) => {
                self.input.mode = Mode::Normal;
            }
            Action::ReplyMessage(message_id, _) => {
                self.input.mode = Mode::Reply(message_id);
            }
            Action::ChatListSearch => self.input.mode = Mode::SearchChatList,
            Action::ChatWindowSearch => self.input.mode = Mode::SearchChatMessages,
            _ => {}
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect) -> io::Result<()> {
        self.sync_draft();
        self.update_input(area);

        let app_context = Arc::clone(&self.app_context);
        let theme = app_context.theme();
        let character_count: usize = self.input.text().iter().map(Vec::len).sum::<usize>()
            + self.input.text().len().saturating_sub(1);
        let text = self
            .input
            .text()
            .iter()
            .map(|line| {
                Line::from(
                    line.iter()
                        .map(|cell| {
                            if cell.selected {
                                Span::styled(
                                    cell.c.to_string(),
                                    self.app_context.style_prompt_message_text_selected(),
                                )
                            } else {
                                Span::styled(
                                    cell.c.to_string(),
                                    self.app_context.style_prompt_message_text(),
                                )
                            }
                        })
                        .collect::<Vec<Span>>(),
                )
            })
            .collect::<Vec<Line>>();

        let (text, style_text) = if self.focused {
            self.input.restore_prompt_size();
            (text, self.app_context.style_prompt())
        } else {
            self.input.set_prompt_size_to_one_unfocused();
            (
                vec![Line::from(format!(
                    "Press {} to write a message",
                    self.focused_keys
                        .iter()
                        .map(|e| e.to_string())
                        .collect::<Vec<String>>()
                        .join(" or ")
                ))],
                theme.style(HighlightGroup::Placeholder),
            )
        };

        // Set title based on mode (sending, editing, replying, search)
        let title: String = match &self.input.mode {
            Mode::SearchChatList => "Search chats".into(),
            Mode::SearchChatMessages => "Search messages".into(),
            Mode::Edit(_) => "Editing message".into(),
            Mode::Reply(_) => {
                let tg_context = self.app_context.store();
                let preview = tg_context.reply_message_text();
                Self::reply_title(&preview)
            }
            Mode::Normal => {
                let store = self.app_context.store();
                match (store.name_of_open_chat_id(), store.open_topic()) {
                    (Some(name), Some(topic)) => format!("Message {name} › {}", topic.name),
                    (Some(name), None) => format!("Message {name}"),
                    (None, _) => "Message".into(),
                }
            }
        };

        // Attached files come first: they go with the next message
        let title = if self.attachments.is_empty() || !self.can_attach() {
            title
        } else {
            format!(
                "📎 {} · enter sends, esc makes it text · {title}",
                attachment::names(&self.attachments)
            )
        };

        // Edits and searches aren't sent
        let title = if self.app_context.store().sends_silently()
            && matches!(self.input.mode, Mode::Normal | Mode::Reply(_))
        {
            format!("🔕 without sound · {title}")
        } else {
            title
        };

        let counter_style = if character_count > MESSAGE_MAX_CHARS {
            theme.style(HighlightGroup::Error)
        } else {
            theme.style(HighlightGroup::MessageSecondary)
        };
        let mut block = pane::composer_block(theme, pane::title(&title), self.focused);
        if self.focused
            && matches!(
                self.input.mode,
                Mode::Normal | Mode::Edit(_) | Mode::Reply(_)
            )
        {
            block = block.title(
                Line::from(Span::styled(
                    format!(" {character_count} / {MESSAGE_MAX_CHARS} "),
                    counter_style,
                ))
                .right_aligned(),
            );
        }

        let input = Paragraph::new(text).style(style_text).block(block);

        frame.render_widget(input, area);

        if self.focused {
            frame.set_cursor_position(Position {
                x: area.x + self.input.cursor_x() as u16 + 1,
                y: area.y + self.input.cursor_y() as u16 + 1,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/prompt_window.rs"]
mod tests;
