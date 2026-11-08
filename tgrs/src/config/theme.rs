//! `theme.toml`: Highlight Group overrides, border shapes and the selection
//! marker. Every group and field is optional; built-in defaults follow the
//! terminal's own colors.
//!
//! Invalid leaves are skipped with a warning instead of failing, so one typo
//! doesn't keep tgrs from starting.

use std::{collections::BTreeMap, path::Path};

use unicode_width::UnicodeWidthStr;

use super::{ConfigError, read_toml};

pub const FILE_NAME: &str = "theme.toml";

/// Validated contents of `theme.toml`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ThemeFile {
    highlights: BTreeMap<HighlightGroup, HighlightOptions>,
    border_shapes: BorderShapeOptions,
    selection_marker: Option<String>,
}

impl ThemeFile {
    pub fn highlights(&self) -> &BTreeMap<HighlightGroup, HighlightOptions> {
        &self.highlights
    }

    pub const fn border_shapes(&self) -> &BorderShapeOptions {
        &self.border_shapes
    }

    pub fn selection_marker(&self) -> Option<&str> {
        self.selection_marker.as_deref()
    }

    fn highlight_mut(&mut self, group: HighlightGroup) -> &mut HighlightOptions {
        self.highlights.entry(group).or_default()
    }
}

/// Loads `theme.toml` from `config_dir`; a missing file is the default theme.
/// Returns the warnings about skipped entries.
pub fn load(config_dir: &Path) -> Result<(ThemeFile, Vec<String>), ConfigError> {
    let table: Option<toml::Table> = read_toml(&config_dir.join(FILE_NAME))?;
    Ok(table.map_or_else(Default::default, |table| parse(&table)))
}

/// Parses `theme.toml` contents, collecting a warning per skipped entry.
pub fn parse(table: &toml::Table) -> (ThemeFile, Vec<String>) {
    let mut parser = Parser::default();
    for (section, value) in table {
        match (section.as_str(), value.as_table()) {
            ("highlight", Some(groups)) => parser.parse_highlights(groups),
            ("ui", Some(ui)) => parser.parse_ui(ui),
            _ => parser
                .warnings
                .push(format!("[{section}] is unknown and was ignored")),
        }
    }
    (parser.theme, parser.warnings)
}

macro_rules! define_border_surfaces {
    ($($variant:ident => $name:literal, rounded_by_default = $rounded:literal),+ $(,)?) => {
        /// A kind of framed surface whose border shape is configurable.
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        #[repr(usize)]
        pub enum BorderSurface {
            $($variant),+
        }

        impl BorderSurface {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];
            pub const COUNT: usize = Self::ALL.len();

            pub fn from_name(name: &str) -> Option<Self> {
                match name {
                    $($name => Some(Self::$variant),)+
                    _ => None,
                }
            }

            pub const fn rounded_by_default(self) -> bool {
                match self {
                    $(Self::$variant => $rounded),+
                }
            }
        }
    };
}

define_border_surfaces! {
    Pane => "pane", rounded_by_default = false,
    Composer => "composer", rounded_by_default = true,
    Modal => "modal", rounded_by_default = false,
    Picker => "picker", rounded_by_default = false,
    Message => "message", rounded_by_default = true,
}

/// `[ui.border]`: a default shape and per-surface overrides.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BorderShapeOptions {
    pub default: Option<BorderShape>,
    surfaces: [Option<BorderShape>; BorderSurface::COUNT],
}

impl BorderShapeOptions {
    pub const fn get(&self, surface: BorderSurface) -> Option<BorderShape> {
        self.surfaces[surface as usize]
    }

    pub fn set(&mut self, surface: BorderSurface, shape: BorderShape) {
        self.surfaces[surface as usize] = Some(shape);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BorderShape {
    Plain,
    Rounded,
    Double,
    Thick,
    LightDoubleDashed,
    HeavyDoubleDashed,
    LightTripleDashed,
    HeavyTripleDashed,
    LightQuadrupleDashed,
    HeavyQuadrupleDashed,
    QuadrantInside,
    QuadrantOutside,
}

impl BorderShape {
    fn from_name(name: &str) -> Option<Self> {
        match name {
            "plain" => Some(Self::Plain),
            "rounded" => Some(Self::Rounded),
            "double" => Some(Self::Double),
            "thick" => Some(Self::Thick),
            "light_double_dashed" => Some(Self::LightDoubleDashed),
            "heavy_double_dashed" => Some(Self::HeavyDoubleDashed),
            "light_triple_dashed" => Some(Self::LightTripleDashed),
            "heavy_triple_dashed" => Some(Self::HeavyTripleDashed),
            "light_quadruple_dashed" => Some(Self::LightQuadrupleDashed),
            "heavy_quadruple_dashed" => Some(Self::HeavyQuadrupleDashed),
            "quadrant_inside" => Some(Self::QuadrantInside),
            "quadrant_outside" => Some(Self::QuadrantOutside),
            _ => None,
        }
    }
}

/// `[highlight.<Group>]`: fields left unset are inherited.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HighlightOptions {
    pub link: Option<HighlightLink>,
    pub foreground: Option<String>,
    pub background: Option<String>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub dim: Option<bool>,
    pub underline: Option<bool>,
    pub strikethrough: Option<bool>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HighlightLink {
    Inherit(HighlightGroup),
    /// `link = "none"`: stop inheriting from the built-in parent.
    Detached,
}

macro_rules! define_highlight_groups {
    ($($variant:ident => $name:literal),+ $(,)?) => {
        /// A named style used by the UI, like a Vim highlight group.
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        #[repr(usize)]
        pub enum HighlightGroup {
            $($variant),+
        }

        impl HighlightGroup {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];
            pub const COUNT: usize = Self::ALL.len();

            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $name),+
                }
            }

            pub fn from_name(name: &str) -> Option<Self> {
                match name {
                    $($name => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }
    };
}

define_highlight_groups! {
    Normal => "Normal",
    Strong => "Strong",
    Emphasis => "Emphasis",
    Muted => "Muted",
    Title => "Title",
    Heading => "Heading",
    Decoration => "Decoration",
    Hint => "Hint",
    Description => "Description",
    Shortcut => "Shortcut",
    ChatTypeMarker => "ChatTypeMarker",
    FieldLabel => "FieldLabel",
    SearchContext => "SearchContext",
    Timestamp => "Timestamp",
    Placeholder => "Placeholder",
    Disabled => "Disabled",
    Loading => "Loading",
    Edited => "Edited",
    Unavailable => "Unavailable",
    PaneTitle => "PaneTitle",
    ModalTitle => "ModalTitle",
    ComposerTitle => "ComposerTitle",
    HeaderTitle => "HeaderTitle",
    HeaderLabel => "HeaderLabel",
    MessageAuthor => "MessageAuthor",
    SelfMessageAuthor => "SelfMessageAuthor",
    MessageTimestamp => "MessageTimestamp",
    MessageSecondary => "MessageSecondary",
    ChatPreview => "ChatPreview",
    HeaderError => "HeaderError",
    HeaderWarning => "HeaderWarning",
    Border => "Border",
    FocusBorder => "FocusBorder",
    Selection => "Selection",
    SelectionBorder => "SelectionBorder",
    PaneBorder => "PaneBorder",
    FocusedPaneBorder => "FocusedPaneBorder",
    ComposerBorder => "ComposerBorder",
    ActiveComposerBorder => "ActiveComposerBorder",
    ModalBorder => "ModalBorder",
    ComposerPickerBorder => "ComposerPickerBorder",
    SelectedRow => "SelectedRow",
    SelectionMarker => "SelectionMarker",
    ActiveField => "ActiveField",
    ActiveTab => "ActiveTab",
    MessageSelectedBorder => "MessageSelectedBorder",
    ReplyTargetBorder => "ReplyTargetBorder",
    ScrollbarThumb => "ScrollbarThumb",
    ScrollbarTrack => "ScrollbarTrack",
    UnreadNotice => "UnreadNotice",
    Editing => "Editing",
    Reaction => "Reaction",
    SelfReaction => "SelfReaction",
    PresenceOnline => "PresenceOnline",
    PresenceOffline => "PresenceOffline",
    NavigationActive => "NavigationActive",
    NavigationMentioned => "NavigationMentioned",
    NavigationUnread => "NavigationUnread",
    MentionBadge => "MentionBadge",
    NotificationBadge => "NotificationBadge",
    MutedChat => "MutedChat",
    PinnedBadge => "PinnedBadge",
    MessageBody => "MessageBody",
    MarkdownQuote => "MarkdownQuote",
    MarkdownMarker => "MarkdownMarker",
    MessageAttachment => "MessageAttachment",
    InlineCode => "InlineCode",
    MessageLink => "MessageLink",
    MentionSelf => "MentionSelf",
    MentionOther => "MentionOther",
    UnreadDivider => "UnreadDivider",
    DateDivider => "DateDivider",
    BotBadge => "BotBadge",
    ReadReceipt => "ReadReceipt",
    Error => "Error",
    Warning => "Warning",
    Success => "Success",
    Info => "Info",
}

#[derive(Default)]
struct Parser {
    theme: ThemeFile,
    warnings: Vec<String>,
}

impl Parser {
    fn parse_ui(&mut self, ui: &toml::Table) {
        for (section, value) in ui {
            match (section.as_str(), value.as_table()) {
                ("border", Some(fields)) => self.parse_border_shapes(fields),
                ("indicator", Some(fields)) => self.parse_indicators(fields),
                _ => self
                    .warnings
                    .push(format!("[ui.{section}] is unknown and was ignored")),
            }
        }
    }

    fn parse_indicators(&mut self, fields: &toml::Table) {
        for (field, value) in fields {
            if field != "selection" {
                self.warnings
                    .push(format!("[ui.indicator] {field} is unknown and was ignored"));
                continue;
            }
            let Some(raw) = value.as_str() else {
                self.warnings
                    .push("[ui.indicator] selection must be a string and was ignored".to_owned());
                continue;
            };
            if raw.contains(['\n', '\r']) {
                self.warnings.push(
                    "[ui.indicator] selection must be a single line and was ignored".to_owned(),
                );
                continue;
            }
            if raw.width() == 0 {
                self.warnings.push(
                    "[ui.indicator] selection must have non-zero display width and was ignored"
                        .to_owned(),
                );
                continue;
            }
            self.theme.selection_marker = Some(raw.to_owned());
        }
    }

    fn parse_border_shapes(&mut self, fields: &toml::Table) {
        for (field, value) in fields {
            let surface = if field == "default" {
                None
            } else {
                match BorderSurface::from_name(field) {
                    Some(surface) => Some(surface),
                    None => {
                        self.warnings
                            .push(format!("[ui.border] {field} is unknown and was ignored"));
                        continue;
                    }
                }
            };
            let Some(raw) = value.as_str() else {
                self.warnings.push(format!(
                    "[ui.border] {field} must be a string and was ignored"
                ));
                continue;
            };
            let Some(shape) = BorderShape::from_name(raw) else {
                self.warnings.push(format!(
                    "[ui.border] {field} = \"{raw}\" is not a supported border shape and was ignored"
                ));
                continue;
            };
            match surface {
                Some(surface) => self.theme.border_shapes.set(surface, shape),
                None => self.theme.border_shapes.default = Some(shape),
            }
        }
    }

    fn parse_highlights(&mut self, table: &toml::Table) {
        for (name, value) in table {
            let Some(group) = HighlightGroup::from_name(name) else {
                self.warnings
                    .push(format!("[highlight] {name} is unknown and was ignored"));
                continue;
            };
            let Some(fields) = value.as_table() else {
                self.warnings.push(format!(
                    "[highlight.{name}] must be a table and was ignored"
                ));
                continue;
            };
            self.parse_highlight_fields(group, fields);
        }
    }

    fn parse_highlight_fields(&mut self, group: HighlightGroup, fields: &toml::Table) {
        for (field, value) in fields {
            match field.as_str() {
                "link" => match value.as_str() {
                    Some("none") => {
                        self.theme.highlight_mut(group).link = Some(HighlightLink::Detached);
                    }
                    Some(name) => match HighlightGroup::from_name(name) {
                        Some(link) => {
                            self.theme.highlight_mut(group).link =
                                Some(HighlightLink::Inherit(link));
                        }
                        None => self.warnings.push(format!(
                            "[highlight.{}] link references unknown group {name} and was ignored",
                            group.name()
                        )),
                    },
                    None => self.type_warning(group, field, "a string"),
                },
                "foreground" | "background" => match value.as_str() {
                    Some(raw) => {
                        let options = self.theme.highlight_mut(group);
                        let channel = if field == "foreground" {
                            &mut options.foreground
                        } else {
                            &mut options.background
                        };
                        *channel = Some(raw.to_owned());
                    }
                    None => self.type_warning(group, field, "a string"),
                },
                "bold" | "italic" | "dim" | "underline" | "strikethrough" => {
                    let Some(enabled) = value.as_bool() else {
                        self.type_warning(group, field, "a boolean");
                        continue;
                    };
                    let options = self.theme.highlight_mut(group);
                    let flag = match field.as_str() {
                        "bold" => &mut options.bold,
                        "italic" => &mut options.italic,
                        "dim" => &mut options.dim,
                        "underline" => &mut options.underline,
                        _ => &mut options.strikethrough,
                    };
                    *flag = Some(enabled);
                }
                _ => self.warnings.push(format!(
                    "[highlight.{}] {field} is unknown and was ignored",
                    group.name()
                )),
            }
        }
    }

    fn type_warning(&mut self, group: HighlightGroup, field: &str, expected: &str) {
        self.warnings.push(format!(
            "[highlight.{}] {field} must be {expected} and was ignored",
            group.name()
        ));
    }
}
