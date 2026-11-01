use crate::{
    app::action::Action,
    app::context::AppContext,
    app::event::Event,
    settings::ActionBinding,
    ui::component_name::ComponentName,
    ui::component_traits::{Component, HandleFocus},
};
use ratatui::{
    Frame,
    layout::{Alignment, Rect},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use std::{collections::HashMap, io, sync::Arc};
use tokio::sync::mpsc::UnboundedSender;

/// `CommandGuide` is a struct that represents a popup window displaying keybindings.
/// It shows all available keybindings organized by component and explains configuration options.
pub struct CommandGuide {
    /// The application context.
    app_context: Arc<AppContext>,
    /// The name of the `CommandGuide`.
    name: String,
    /// An unbounded sender that send action for processing.
    action_tx: Option<UnboundedSender<Action>>,
    /// Indicates whether the `CommandGuide` is focused or not.
    focused: bool,
    /// Indicates whether the command guide should be shown.
    visible: bool,
    /// Vertical scroll offset (number of lines scrolled).
    scroll_offset: u16,
    /// Last inner area height (from previous draw) for clamping scroll in update.
    last_inner_height: u16,
    /// Last content line count (from previous draw) for max scroll.
    last_content_lines: usize,
}

impl CommandGuide {
    /// Create a new instance of the `CommandGuide` struct.
    ///
    /// # Arguments
    /// * `app_context` - An Arc wrapped AppContext struct.
    ///
    /// # Returns
    /// * `Self` - The new instance of the `CommandGuide` struct.
    pub fn new(app_context: Arc<AppContext>) -> Self {
        CommandGuide {
            app_context,
            name: "".to_string(),
            action_tx: None,
            focused: false,
            visible: false,
            scroll_offset: 0,
            last_inner_height: 0,
            last_content_lines: 0,
        }
    }

    /// Set the name of the `CommandGuide`.
    ///
    /// # Arguments
    /// * `name` - The name of the `CommandGuide`.
    ///
    /// # Returns
    /// * `Self` - The modified instance of the `CommandGuide`.
    pub fn with_name(mut self, name: impl AsRef<str>) -> Self {
        self.name = name.as_ref().to_string();
        self
    }

    /// Show the command guide.
    pub fn show(&mut self) {
        self.visible = true;
        self.scroll_offset = 0;
    }

    /// Hide the command guide.
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Check if the command guide is visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Format a keybinding for display.
    fn format_keybinding(event: &Event, description: Option<&String>) -> String {
        let key_str = format!("{}", event);
        let desc_str = description.map(|d| format!(" - {}", d)).unwrap_or_default();
        format!("{:<20}{}", key_str, desc_str)
    }

    /// Get keybindings for a component section.
    fn get_keybindings_section(
        &self,
        title: &str,
        keymap: &HashMap<Event, ActionBinding>,
    ) -> Vec<String> {
        let mut bindings = Vec::new();
        bindings.push(format!("[{}]", title));
        bindings.push(String::new());

        let mut sorted_bindings: Vec<(&Event, &ActionBinding)> = keymap.iter().collect();
        sorted_bindings.sort_by(|a, b| format!("{}", a.0).cmp(&format!("{}", b.0)));

        for (event, binding) in sorted_bindings {
            match binding {
                ActionBinding::Single { description, .. } => {
                    bindings.push(Self::format_keybinding(event, description.as_ref()));
                }
                ActionBinding::Multiple(_) => {
                    // For multiple key bindings, just show the first key
                    bindings.push(Self::format_keybinding(
                        event,
                        Some(&"Multiple keys...".to_string()),
                    ));
                }
            }
        }

        bindings.push(String::new());
        bindings
    }

    /// Build the help text content.
    fn build_help_text(&self) -> Text<'_> {
        let keymap_config = self.app_context.keymap_config();
        let mut lines = vec![
            Line::from(vec![Span::styled(
                "Command Guide - Keybindings",
                self.app_context.style_title_bar(),
            )]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Scroll: ↑/↓ PgUp/PgDn  |  Close: Esc or Alt+F1",
                self.app_context.style_timestamp(),
            )]),
            Line::from(""),
        ];

        let empty = HashMap::new();
        // None state (global) — keybindings available everywhere
        let core_window_lines =
            self.get_keybindings_section("None state (global)", keymap_config.core_window());
        for line in core_window_lines {
            lines.push(Line::from(line));
        }

        // Chat List
        let chat_list_lines = self.get_keybindings_section(
            "Chat List",
            keymap_config
                .section(ComponentName::ChatList)
                .unwrap_or(&empty),
        );
        for line in chat_list_lines {
            lines.push(Line::from(line));
        }

        // Archive List
        let archive_list_lines = self.get_keybindings_section(
            "Archive List",
            keymap_config
                .section(ComponentName::ArchiveList)
                .unwrap_or(&empty),
        );
        for line in archive_list_lines {
            lines.push(Line::from(line));
        }

        // Topic List
        let topic_list_lines = self.get_keybindings_section(
            "Topic List",
            keymap_config
                .section(ComponentName::TopicList)
                .unwrap_or(&empty),
        );
        for line in topic_list_lines {
            lines.push(Line::from(line));
        }

        // Chat — copy (y/Ctrl+C), edit (e), reply (r), delete (d/D)
        let chat_lines = self.get_keybindings_section(
            "Chat",
            keymap_config.section(ComponentName::Chat).unwrap_or(&empty),
        );
        for line in chat_lines {
            lines.push(Line::from(line));
        }

        // Prompt
        let prompt_lines = self.get_keybindings_section(
            "Prompt",
            keymap_config
                .section(ComponentName::Prompt)
                .unwrap_or(&empty),
        );
        for line in prompt_lines {
            lines.push(Line::from(line));
        }

        // Popups opened from the chat
        for (title, component) in [
            ("Message Actions", ComponentName::MessageActions),
            ("Pinned Messages", ComponentName::PinnedMessagesPopup),
            ("Reaction and URL Picker", ComponentName::Picker),
            ("Sticker Picker", ComponentName::StickerPicker),
            ("Chat Actions", ComponentName::ChatActions),
            ("Profile", ComponentName::Profile),
            ("Open Chat by Username", ComponentName::FindChat),
            ("Edit Profile", ComponentName::EditProfile),
            ("Report", ComponentName::Report),
            ("Blocked List", ComponentName::BlockedList),
            ("Sessions", ComponentName::Sessions),
            ("Forward", ComponentName::Forward),
            ("Leader Menu (Space)", ComponentName::LeaderMenu),
        ] {
            let section_lines = self
                .get_keybindings_section(title, keymap_config.section(component).unwrap_or(&empty));
            lines.extend(section_lines.into_iter().map(Line::from));
        }

        lines.push(Line::from(vec![Span::styled(
            "Media",
            self.app_context.style_title_bar(),
        )]));
        lines.push(Line::from(""));
        lines.push(Line::from(
            "  Media is opened with the default application (xdg-open), e.g. mpv or an image viewer.",
        ));
        lines.push(Line::from(format!(
            "  Downloads are saved to {}.",
            self.app_context.app_config().download_dir.display()
        )));
        lines.push(Line::from(""));

        // Mouse
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            "Mouse",
            self.app_context.style_title_bar(),
        )]));
        lines.push(Line::from(""));
        lines.push(Line::from(
            "  Scroll: chat list / chat to move selection or messages.",
        ));
        lines.push(Line::from(
            "  Chat list: first click focuses list, second click opens selected chat.",
        ));
        lines.push(Line::from(
            "  Menus: click a row to select it; click the selected row again to activate.",
        ));
        lines.push(Line::from(
            "  Stickers: click a sticker to select it and click again to send; click a set tab to switch sets.",
        ));
        lines.push(Line::from(
            "  Sticker sets: Tab / Shift+Tab switches sets; scroll moves the sticker selection.",
        ));
        lines.push(Line::from(""));

        // Configuration section
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            "Configuration",
            self.app_context.style_title_bar(),
        )]));
        lines.push(Line::from(""));
        lines.push(Line::from("Keybindings can be customized by editing:"));
        lines.push(Line::from(vec![Span::styled(
            "  ~/.config/tgt/keymap.toml",
            self.app_context.style_chat(),
        )]));
        lines.push(Line::from(""));
        lines.push(Line::from("Format:"));
        lines.push(Line::from("  [component_name]"));
        lines.push(Line::from("  keymap = ["));
        lines.push(Line::from(
            "    { keys = [\"key\"], command = \"action\", description = \"...\" },",
        ));
        lines.push(Line::from("  ]"));
        lines.push(Line::from(""));
        lines.push(Line::from(
            "Available components: core_window, chat_list, archive_list, topic_list, chat, prompt, command_guide, search_overlay, message_actions, file_upload_explorer, pinned_messages_popup, picker, sticker_picker, chat_actions, profile, find_chat, edit_profile, report, blocked_list, sessions, forward, leader_menu",
        ));
        lines.push(Line::from(""));
        lines.push(Line::from("Example:"));
        lines.push(Line::from(vec![Span::styled(
            "  [core_window]",
            self.app_context.style_chat(),
        )]));
        lines.push(Line::from(vec![Span::styled(
            "  keymap = [",
            self.app_context.style_chat(),
        )]));
        lines.push(Line::from(vec![Span::styled(
            "    { keys = [\"q\"], command = \"try_quit\", description = \"Quit\" },",
            self.app_context.style_chat(),
        )]));
        lines.push(Line::from(vec![Span::styled(
            "  ]",
            self.app_context.style_chat(),
        )]));

        Text::from(lines)
    }
}

impl HandleFocus for CommandGuide {
    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

impl Component for CommandGuide {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> io::Result<()> {
        self.action_tx = Some(tx);
        Ok(())
    }

    fn update(&mut self, action: Action) {
        match action {
            Action::ShowCommandGuide => {
                self.show();
            }
            Action::HideCommandGuide => {
                self.hide();
            }
            Action::Key(key_code, _modifiers) if self.visible => match key_code {
                crossterm::event::KeyCode::Up => {
                    self.scroll_offset = self.scroll_offset.saturating_sub(1);
                }
                crossterm::event::KeyCode::Down => {
                    let max_scroll = self
                        .last_content_lines
                        .saturating_sub(self.last_inner_height as usize)
                        .min(u16::MAX as usize) as u16;
                    self.scroll_offset = (self.scroll_offset + 1).min(max_scroll);
                }
                crossterm::event::KeyCode::PageUp => {
                    let page = self.last_inner_height;
                    self.scroll_offset = self.scroll_offset.saturating_sub(page);
                }
                crossterm::event::KeyCode::PageDown => {
                    let max_scroll = self
                        .last_content_lines
                        .saturating_sub(self.last_inner_height as usize)
                        .min(u16::MAX as usize) as u16;
                    let page = self.last_inner_height;
                    self.scroll_offset = (self.scroll_offset + page).min(max_scroll);
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn draw(&mut self, frame: &mut Frame<'_>, area: Rect) -> io::Result<()> {
        if !self.visible {
            return Ok(());
        }

        // Calculate popup size (80% of screen, centered)
        let popup_width = (area.width as f32 * 0.8) as u16;
        let popup_height = (area.height as f32 * 0.8) as u16;
        let popup_x = (area.width.saturating_sub(popup_width)) / 2;
        let popup_y = (area.height.saturating_sub(popup_height)) / 2;

        let popup_area = Rect::new(
            area.x + popup_x,
            area.y + popup_y,
            popup_width,
            popup_height,
        );

        // Clear the popup area first
        frame.render_widget(Clear, popup_area);

        let block = Block::new()
            .borders(Borders::ALL)
            .title("Command Guide")
            .title_alignment(Alignment::Center)
            .border_style(self.app_context.style_border_component_focused())
            .style(self.app_context.style_chat());

        // Create inner area for content
        let inner_area = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        // Build help text and update scroll state (build once for line count to avoid borrow conflict)
        let content_lines = self.build_help_text().lines.len();
        self.last_content_lines = content_lines;
        self.last_inner_height = inner_area.height;

        // Clamp scroll offset to valid range
        let max_scroll = content_lines
            .saturating_sub(inner_area.height as usize)
            .min(u16::MAX as usize) as u16;
        self.scroll_offset = self.scroll_offset.min(max_scroll);

        let help_text = self.build_help_text();
        let paragraph = Paragraph::new(help_text)
            .wrap(Wrap { trim: true })
            .scroll((self.scroll_offset, 0))
            .style(self.app_context.style_chat());

        frame.render_widget(paragraph, inner_area);

        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/command_guide.rs"]
mod tests;
