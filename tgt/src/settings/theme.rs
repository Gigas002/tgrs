//! The resolved theme: a style per [`HighlightGroup`] and a border shape per
//! [`BorderSurface`]. Built-in defaults use the terminal's own colors (ANSI
//! palette and default foreground/background) plus text modifiers, so tgt
//! follows the terminal theme; `theme.toml` overrides them.

use ratatui::{
    style::{Color, Modifier, Style},
    symbols::border,
    widgets::BorderType,
};

use crate::config::theme::{
    BorderShape, BorderShapeOptions, HighlightLink, HighlightOptions, ThemeFile,
};
pub use crate::config::theme::{BorderSurface, HighlightGroup};

pub const DEFAULT_SELECTION_MARKER: &str = "▸ ";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Theme {
    highlights: [ResolvedHighlight; HighlightGroup::COUNT],
    borders: [BorderType; BorderSurface::COUNT],
    selection_marker: String,
}

#[derive(Clone, Copy, Debug, Default)]
struct HighlightDefinition {
    link: Option<HighlightGroup>,
    style: Style,
    clear_foreground: bool,
    clear_background: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ResolvedHighlight {
    style: Style,
    clear_background: bool,
}

#[derive(Clone, Copy, Debug)]
enum ColorOverride {
    Set(Color),
    Clear,
}

#[derive(Clone, Copy, Debug, Default)]
struct HighlightPatch {
    style: Style,
    foreground: Option<ColorOverride>,
    background: Option<ColorOverride>,
}

impl Default for Theme {
    fn default() -> Self {
        Self::resolve(&ThemeFile::default(), &mut Vec::new())
    }
}

impl Theme {
    /// Applies `file` over the built-in defaults; problems are added to
    /// `warnings` and the offending entries ignored.
    pub fn resolve(file: &ThemeFile, warnings: &mut Vec<String>) -> Self {
        let mut definitions = default_definitions();
        apply_highlight_overrides(&mut definitions, file, warnings);
        Self {
            highlights: resolve_definitions(&definitions, warnings),
            borders: resolve_border_types(file.border_shapes()),
            selection_marker: file
                .selection_marker()
                .unwrap_or(DEFAULT_SELECTION_MARKER)
                .to_owned(),
        }
    }

    pub fn style(&self, group: HighlightGroup) -> Style {
        self.highlights[group as usize].style
    }

    /// `base` with the group's style on top.
    pub fn apply(&self, group: HighlightGroup, base: Style) -> Style {
        base.patch(self.style(group))
    }

    /// Whether the group explicitly clears the background (`"none"`), so it
    /// shouldn't paint over the cells below.
    pub fn background_is_cleared(&self, group: HighlightGroup) -> bool {
        self.highlights[group as usize].clear_background
    }

    pub const fn border_type(&self, surface: BorderSurface) -> BorderType {
        self.borders[surface as usize]
    }

    pub fn border_set(&self, surface: BorderSurface) -> border::Set<'static> {
        self.border_type(surface).to_border_set()
    }

    /// The glyph before the selected row in lists and pickers.
    pub fn selection_marker(&self) -> &str {
        &self.selection_marker
    }
}

fn resolve_border_types(options: &BorderShapeOptions) -> [BorderType; BorderSurface::COUNT] {
    let configured_default = options.default.map(border_type);
    let default = configured_default.unwrap_or(BorderType::Plain);
    std::array::from_fn(|index| {
        let surface = BorderSurface::ALL[index];
        options.get(surface).map(border_type).unwrap_or_else(|| {
            if surface.rounded_by_default() {
                configured_default.unwrap_or(BorderType::Rounded)
            } else {
                default
            }
        })
    })
}

const fn border_type(shape: BorderShape) -> BorderType {
    match shape {
        BorderShape::Plain => BorderType::Plain,
        BorderShape::Rounded => BorderType::Rounded,
        BorderShape::Double => BorderType::Double,
        BorderShape::Thick => BorderType::Thick,
        BorderShape::LightDoubleDashed => BorderType::LightDoubleDashed,
        BorderShape::HeavyDoubleDashed => BorderType::HeavyDoubleDashed,
        BorderShape::LightTripleDashed => BorderType::LightTripleDashed,
        BorderShape::HeavyTripleDashed => BorderType::HeavyTripleDashed,
        BorderShape::LightQuadrupleDashed => BorderType::LightQuadrupleDashed,
        BorderShape::HeavyQuadrupleDashed => BorderType::HeavyQuadrupleDashed,
        BorderShape::QuadrantInside => BorderType::QuadrantInside,
        BorderShape::QuadrantOutside => BorderType::QuadrantOutside,
    }
}

fn default_definitions() -> [HighlightDefinition; HighlightGroup::COUNT] {
    use HighlightGroup as H;

    let mut definitions = [None; HighlightGroup::COUNT];
    let mut define = |group: H, link: Option<H>, style: Style| {
        let slot = &mut definitions[group as usize];
        assert!(slot.is_none(), "highlight group has one default definition");
        *slot = Some(HighlightDefinition {
            link,
            style,
            ..HighlightDefinition::default()
        });
    };
    let plain = Style::default;
    let fg = |color| Style::default().fg(color);
    let bold = |style: Style| style.add_modifier(Modifier::BOLD);

    // Common semantic groups
    define(H::Normal, None, plain().fg(Color::Reset).bg(Color::Reset));
    define(H::Strong, None, bold(plain()));
    define(H::Emphasis, None, plain().add_modifier(Modifier::ITALIC));
    define(H::Muted, None, plain().add_modifier(Modifier::DIM));
    for group in [H::Title, H::Heading] {
        define(group, Some(H::Strong), plain());
    }
    for group in [
        H::Decoration,
        H::Hint,
        H::Description,
        H::Shortcut,
        H::ChatTypeMarker,
        H::FieldLabel,
        H::SearchContext,
        H::Timestamp,
        H::Placeholder,
        H::Disabled,
        H::Loading,
        H::MessageSecondary,
        H::ChatPreview,
    ] {
        define(group, Some(H::Muted), plain());
    }
    define(
        H::Edited,
        Some(H::Muted),
        plain().add_modifier(Modifier::ITALIC),
    );
    define(
        H::Unavailable,
        Some(H::Muted),
        plain().add_modifier(Modifier::CROSSED_OUT),
    );

    // Titles and headers
    for group in [H::PaneTitle, H::ModalTitle, H::ComposerTitle] {
        define(group, Some(H::Title), plain());
    }
    define(H::HeaderTitle, Some(H::Title), fg(Color::Cyan));
    define(H::HeaderLabel, Some(H::Muted), plain());
    define(H::HeaderError, Some(H::Error), bold(plain()));
    define(H::HeaderWarning, Some(H::Warning), bold(plain()));

    // Messages
    define(H::MessageAuthor, Some(H::Strong), plain());
    define(
        H::SelfMessageAuthor,
        Some(H::MessageAuthor),
        fg(Color::Cyan),
    );
    define(H::MessageTimestamp, Some(H::Timestamp), plain());
    define(H::MessageBody, None, fg(Color::Reset));
    define(H::MarkdownQuote, None, fg(Color::DarkGray));
    define(H::MarkdownMarker, None, fg(Color::DarkGray));
    define(H::MessageAttachment, None, fg(Color::Cyan));
    define(H::InlineCode, None, fg(Color::Rgb(255, 165, 0)));
    define(
        H::MessageLink,
        None,
        fg(Color::Cyan).add_modifier(Modifier::UNDERLINED),
    );
    define(
        H::MentionSelf,
        None,
        fg(Color::Yellow).bg(Color::Rgb(92, 76, 35)),
    );
    define(
        H::MentionOther,
        None,
        fg(Color::Rgb(193, 206, 247)).bg(Color::Rgb(40, 50, 92)),
    );
    define(H::UnreadDivider, None, fg(Color::Rgb(237, 66, 69)));
    define(H::DateDivider, Some(H::Decoration), plain());
    define(
        H::BotBadge,
        Some(H::Normal),
        bold(plain().bg(Color::Rgb(88, 101, 242))),
    );
    define(H::ReadReceipt, None, fg(Color::Cyan));
    define(H::Reaction, None, fg(Color::Cyan));
    define(H::SelfReaction, None, fg(Color::Yellow));

    // Borders and selection
    define(H::Border, None, fg(Color::DarkGray));
    define(H::FocusBorder, None, fg(Color::Cyan));
    define(
        H::Selection,
        None,
        bold(fg(Color::Cyan)).remove_modifier(Modifier::DIM),
    );
    define(H::SelectionBorder, None, bold(fg(Color::Green)));
    define(H::PaneBorder, Some(H::Border), plain());
    define(H::FocusedPaneBorder, Some(H::FocusBorder), bold(plain()));
    define(H::ComposerBorder, Some(H::Border), plain());
    define(H::ActiveComposerBorder, Some(H::FocusBorder), bold(plain()));
    define(H::ModalBorder, Some(H::FocusBorder), bold(plain()));
    define(H::ComposerPickerBorder, Some(H::FocusBorder), plain());
    define(H::SelectedRow, Some(H::Selection), plain());
    define(H::SelectionMarker, Some(H::Selection), plain());
    define(H::ActiveField, None, bold(fg(Color::Cyan)));
    define(H::ActiveTab, Some(H::Selection), plain());
    define(H::MessageSelectedBorder, Some(H::SelectionBorder), plain());
    define(H::ReplyTargetBorder, None, bold(fg(Color::Yellow)));
    define(H::ScrollbarThumb, None, fg(Color::Rgb(170, 170, 170)));
    define(
        H::ScrollbarTrack,
        Some(H::ScrollbarThumb),
        plain().add_modifier(Modifier::DIM),
    );

    // States and navigation
    define(H::UnreadNotice, None, bold(fg(Color::Cyan)));
    define(H::Editing, None, fg(Color::Yellow));
    define(H::PresenceOnline, None, fg(Color::Green));
    define(
        H::PresenceOffline,
        Some(H::Normal),
        plain().add_modifier(Modifier::DIM),
    );
    define(H::NavigationActive, None, bold(fg(Color::Green)));
    define(H::NavigationMentioned, None, fg(Color::Rgb(255, 165, 0)));
    define(H::NavigationUnread, None, fg(Color::Reset));
    define(H::MentionBadge, None, fg(Color::Rgb(255, 165, 0)));
    define(H::NotificationBadge, None, fg(Color::Reset));
    define(H::MutedChat, Some(H::Muted), plain());
    define(H::PinnedBadge, None, bold(fg(Color::Yellow)));
    define(H::Error, None, fg(Color::Red));
    define(H::Warning, None, fg(Color::Yellow));
    define(H::Success, None, fg(Color::Green));
    define(H::Info, None, fg(Color::Cyan));

    definitions[H::Selection as usize]
        .as_mut()
        .expect("selection highlight is defined")
        .clear_background = true;

    definitions.map(|definition| definition.expect("every highlight group has a default"))
}

fn apply_highlight_overrides(
    definitions: &mut [HighlightDefinition; HighlightGroup::COUNT],
    file: &ThemeFile,
    warnings: &mut Vec<String>,
) {
    for (&group, options) in file.highlights() {
        let definition = &mut definitions[group as usize];
        if let Some(link) = options.link {
            definition.link = match link {
                HighlightLink::Inherit(group) => Some(group),
                HighlightLink::Detached => None,
            };
        }
        let patch = highlight_patch(group, options, warnings);
        definition.style = definition.style.patch(patch.style);
        apply_color_override(
            &mut definition.style.fg,
            &mut definition.clear_foreground,
            patch.foreground,
        );
        apply_color_override(
            &mut definition.style.bg,
            &mut definition.clear_background,
            patch.background,
        );
    }
}

fn apply_color_override(
    channel: &mut Option<Color>,
    clear: &mut bool,
    override_value: Option<ColorOverride>,
) {
    match override_value {
        Some(ColorOverride::Set(color)) => {
            *channel = Some(color);
            *clear = false;
        }
        Some(ColorOverride::Clear) => {
            *channel = None;
            *clear = true;
        }
        None => {}
    }
}

fn highlight_patch(
    group: HighlightGroup,
    options: &HighlightOptions,
    warnings: &mut Vec<String>,
) -> HighlightPatch {
    let mut patch = HighlightPatch::default();
    if let Some(raw) = options.foreground.as_deref() {
        patch.foreground = parse_color_override(group, "foreground", raw, warnings);
    }
    if let Some(raw) = options.background.as_deref() {
        patch.background = parse_color_override(group, "background", raw, warnings);
    }
    for (enabled, modifier) in [
        (options.bold, Modifier::BOLD),
        (options.italic, Modifier::ITALIC),
        (options.dim, Modifier::DIM),
        (options.underline, Modifier::UNDERLINED),
        (options.strikethrough, Modifier::CROSSED_OUT),
    ] {
        if let Some(enabled) = enabled {
            patch.style = if enabled {
                patch.style.add_modifier(modifier)
            } else {
                patch.style.remove_modifier(modifier)
            };
        }
    }
    patch
}

fn parse_color_override(
    group: HighlightGroup,
    field: &str,
    raw: &str,
    warnings: &mut Vec<String>,
) -> Option<ColorOverride> {
    if raw == "none" {
        return Some(ColorOverride::Clear);
    }
    match parse_color(raw) {
        Some(color) => Some(ColorOverride::Set(color)),
        None => {
            warnings.push(format!(
                "[highlight.{}] {field} = \"{raw}\" is not a supported color and was ignored",
                group.name()
            ));
            None
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ResolveState {
    Unvisited,
    Visiting,
    Resolved,
}

fn resolve_definitions(
    definitions: &[HighlightDefinition; HighlightGroup::COUNT],
    warnings: &mut Vec<String>,
) -> [ResolvedHighlight; HighlightGroup::COUNT] {
    let mut highlights = [ResolvedHighlight::default(); HighlightGroup::COUNT];
    let mut states = [ResolveState::Unvisited; HighlightGroup::COUNT];
    for &group in HighlightGroup::ALL {
        let _ = resolve_group(group, definitions, &mut highlights, &mut states, warnings);
    }
    highlights
}

fn resolve_group(
    group: HighlightGroup,
    definitions: &[HighlightDefinition; HighlightGroup::COUNT],
    highlights: &mut [ResolvedHighlight; HighlightGroup::COUNT],
    states: &mut [ResolveState; HighlightGroup::COUNT],
    warnings: &mut Vec<String>,
) -> Result<ResolvedHighlight, ()> {
    let index = group as usize;
    match states[index] {
        ResolveState::Resolved => return Ok(highlights[index]),
        ResolveState::Visiting => {
            warnings.push(format!(
                "[highlight.{}] link cycle detected; cyclic inheritance was ignored",
                group.name()
            ));
            return Err(());
        }
        ResolveState::Unvisited => {}
    }

    states[index] = ResolveState::Visiting;
    let definition = definitions[index];
    let inherited = match definition.link {
        Some(link) => match resolve_group(link, definitions, highlights, states, warnings) {
            Ok(highlight) => highlight,
            Err(()) => {
                highlights[index] = resolve_definition(ResolvedHighlight::default(), definition);
                states[index] = ResolveState::Resolved;
                return Err(());
            }
        },
        None => ResolvedHighlight::default(),
    };
    let resolved = resolve_definition(inherited, definition);
    highlights[index] = resolved;
    states[index] = ResolveState::Resolved;
    Ok(resolved)
}

fn resolve_definition(
    inherited: ResolvedHighlight,
    definition: HighlightDefinition,
) -> ResolvedHighlight {
    let mut resolved = ResolvedHighlight {
        style: inherited.style.patch(definition.style),
        ..inherited
    };
    if definition.style.bg.is_some() {
        resolved.clear_background = false;
    }
    if definition.clear_foreground {
        resolved.style.fg = None;
    }
    if definition.clear_background {
        resolved.style.bg = None;
        resolved.clear_background = true;
    }
    resolved
}

/// `terminal_default`, the 16 ANSI color names, or `#RRGGBB` (`#` optional).
pub fn parse_color(value: &str) -> Option<Color> {
    match value {
        "terminal_default" => Some(Color::Reset),
        "black" => Some(Color::Black),
        "red" => Some(Color::Red),
        "green" => Some(Color::Green),
        "yellow" => Some(Color::Yellow),
        "blue" => Some(Color::Blue),
        "magenta" => Some(Color::Magenta),
        "cyan" => Some(Color::Cyan),
        "gray" => Some(Color::Gray),
        "dark_gray" => Some(Color::DarkGray),
        "light_red" => Some(Color::LightRed),
        "light_green" => Some(Color::LightGreen),
        "light_yellow" => Some(Color::LightYellow),
        "light_blue" => Some(Color::LightBlue),
        "light_magenta" => Some(Color::LightMagenta),
        "light_cyan" => Some(Color::LightCyan),
        "white" => Some(Color::White),
        raw => parse_hex_color(raw),
    }
}

fn parse_hex_color(value: &str) -> Option<Color> {
    let hex = value.strip_prefix('#').unwrap_or(value);
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let channel = |range| u8::from_str_radix(&hex[range], 16).ok();
    Some(Color::Rgb(channel(0..2)?, channel(2..4)?, channel(4..6)?))
}
