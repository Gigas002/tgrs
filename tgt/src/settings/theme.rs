use std::collections::HashMap;

use ratatui::style::{Color, Modifier, Style};

use crate::config::{ThemeEntry, ThemeFile, theme::ThemeSection};

/// The default theme, used when nothing else is selected.
pub const DEFAULT_THEME: &str = "theme";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ThemeError {
    #[error("theme {theme:?}: invalid color {color:?}")]
    InvalidColor { theme: String, color: String },

    #[error("unknown theme {0:?}")]
    Unknown(String),
}

/// A resolved theme: palette applied, one style per element.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Theme {
    pub name: String,
    pub common: HashMap<String, Style>,
    pub chat_list: HashMap<String, Style>,
    pub chat: HashMap<String, Style>,
    pub prompt: HashMap<String, Style>,
    pub status_bar: HashMap<String, Style>,
    pub title_bar: HashMap<String, Style>,
    pub reply_message: HashMap<String, Style>,
}

impl Theme {
    /// Resolves a theme file: palette names are looked up first, then plain colors.
    pub fn resolve(name: &str, file: ThemeFile) -> Result<Self, ThemeError> {
        let invalid = |color: &str| ThemeError::InvalidColor {
            theme: name.to_owned(),
            color: color.to_owned(),
        };
        let palette = file
            .palette
            .iter()
            .map(|(key, value)| {
                parse_color(value)
                    .map(|c| (key.clone(), c))
                    .ok_or_else(|| invalid(value))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;
        let section = |section: ThemeSection| {
            section
                .into_iter()
                .map(|(key, entry)| {
                    style(&entry, &palette)
                        .map(|s| (key, s))
                        .map_err(|c| invalid(&c))
                })
                .collect::<Result<HashMap<_, _>, _>>()
        };

        let mut theme = Self {
            name: name.to_owned(),
            common: section(file.common)?,
            chat_list: section(file.chat_list)?,
            chat: section(file.chat)?,
            prompt: section(file.prompt)?,
            status_bar: section(file.status_bar)?,
            title_bar: section(file.title_bar)?,
            reply_message: section(file.reply_message)?,
        };
        adapt_for_readability(name, &mut theme);
        Ok(theme)
    }
}

/// Builds a style; `Err` carries the color that failed to parse.
fn style(entry: &ThemeEntry, palette: &HashMap<String, Color>) -> Result<Style, String> {
    let color = |value: &Option<String>| -> Result<Option<Color>, String> {
        match value.as_deref() {
            None | Some("") => Ok(None),
            Some(value) => palette
                .get(value)
                .copied()
                .or_else(|| parse_color(value))
                .map(Some)
                .ok_or_else(|| value.to_owned()),
        }
    };
    let mut modifier = Modifier::empty();
    for (enabled, flag) in [
        (entry.italic, Modifier::ITALIC),
        (entry.bold, Modifier::BOLD),
        (entry.underline, Modifier::UNDERLINED),
    ] {
        if enabled == Some(true) {
            modifier.insert(flag);
        }
    }
    Ok(Style {
        fg: color(&entry.fg)?,
        bg: color(&entry.bg)?,
        add_modifier: modifier,
        ..Style::default()
    })
}

/// Parses a color name, `#rgb`, `#rrggbb`, `r,g,b` or an ANSI index.
pub fn parse_color(s: &str) -> Option<Color> {
    let color = match s {
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "gray" => Color::Gray,
        "dark_gray" => Color::DarkGray,
        "light_red" => Color::LightRed,
        "light_green" => Color::LightGreen,
        "light_yellow" => Color::LightYellow,
        "light_blue" => Color::LightBlue,
        "light_magenta" => Color::LightMagenta,
        "light_cyan" => Color::LightCyan,
        "white" => Color::White,
        "reset" | "" => Color::Reset,
        s if s.starts_with('#') => {
            let hex = &s[1..];
            let channel = |i: usize, len: usize| u8::from_str_radix(hex.get(i..i + len)?, 16).ok();
            match hex.len() {
                // #rgb is shorthand for #rrggbb
                3 => Color::Rgb(
                    channel(0, 1)? * 17,
                    channel(1, 1)? * 17,
                    channel(2, 1)? * 17,
                ),
                6 => Color::Rgb(channel(0, 2)?, channel(2, 2)?, channel(4, 2)?),
                _ => return None,
            }
        }
        s => {
            if let [r, g, b] = s.split(',').map(str::trim).collect::<Vec<_>>().as_slice() {
                Color::Rgb(r.parse().ok()?, g.parse().ok()?, b.parse().ok()?)
            } else {
                Color::Indexed(s.parse().ok()?)
            }
        }
    };
    Some(color)
}

/// Theme post-processing to improve readability for VSCode-imported themes.
///
/// Many VSCode themes look great in an editor but can map poorly to a TUI where we rely heavily on
/// a small number of foreground colors over a mostly-flat background. This function enforces:
/// - minimum contrast for chat list title + message preview against the chat list background
/// - differentiation between chat name and message preview so they don't look identical
///
/// We intentionally **skip** the built-in `theme` and `first_theme` which are already tuned.
fn adapt_for_readability(theme_name: &str, theme: &mut Theme) {
    if theme_name == "theme" || theme_name == "first_theme" {
        return;
    }

    // ----- Color utilities -----
    fn srgb_to_linear(c: f32) -> f32 {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }
    fn rel_luminance(rgb: (u8, u8, u8)) -> f32 {
        let (r, g, b) = (
            rgb.0 as f32 / 255.0,
            rgb.1 as f32 / 255.0,
            rgb.2 as f32 / 255.0,
        );
        let (r, g, b) = (srgb_to_linear(r), srgb_to_linear(g), srgb_to_linear(b));
        0.2126 * r + 0.7152 * g + 0.0722 * b
    }
    fn contrast_ratio(fg: (u8, u8, u8), bg: (u8, u8, u8)) -> f32 {
        let (l1, l2) = {
            let a = rel_luminance(fg);
            let b = rel_luminance(bg);
            if a >= b { (a, b) } else { (b, a) }
        };
        (l1 + 0.05) / (l2 + 0.05)
    }
    fn rgb_distance(a: (u8, u8, u8), b: (u8, u8, u8)) -> u32 {
        let dr = a.0 as i32 - b.0 as i32;
        let dg = a.1 as i32 - b.1 as i32;
        let db = a.2 as i32 - b.2 as i32;
        (dr * dr + dg * dg + db * db) as u32
    }
    fn clamp_u8(v: f32) -> u8 {
        v.round().clamp(0.0, 255.0) as u8
    }
    fn lerp(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
        (
            clamp_u8(a.0 as f32 + (b.0 as f32 - a.0 as f32) * t),
            clamp_u8(a.1 as f32 + (b.1 as f32 - a.1 as f32) * t),
            clamp_u8(a.2 as f32 + (b.2 as f32 - a.2 as f32) * t),
        )
    }

    // Ratatui Color -> RGB (best-effort)
    fn xterm_index_to_rgb(idx: u8) -> (u8, u8, u8) {
        // 0-15: ANSI colors
        const ANSI16: [(u8, u8, u8); 16] = [
            (0, 0, 0),
            (128, 0, 0),
            (0, 128, 0),
            (128, 128, 0),
            (0, 0, 128),
            (128, 0, 128),
            (0, 128, 128),
            (192, 192, 192),
            (128, 128, 128),
            (255, 0, 0),
            (0, 255, 0),
            (255, 255, 0),
            (0, 0, 255),
            (255, 0, 255),
            (0, 255, 255),
            (255, 255, 255),
        ];
        if idx < 16 {
            return ANSI16[idx as usize];
        }
        if (16..=231).contains(&idx) {
            let i = idx - 16;
            let r = i / 36;
            let g = (i % 36) / 6;
            let b = i % 6;
            let conv = |c: u8| -> u8 {
                match c {
                    0 => 0,
                    1 => 95,
                    2 => 135,
                    3 => 175,
                    4 => 215,
                    _ => 255,
                }
            };
            return (conv(r), conv(g), conv(b));
        }
        // 232-255: grayscale ramp
        let gray = 8 + (idx - 232) * 10;
        (gray, gray, gray)
    }

    fn color_to_rgb(color: Color) -> Option<(u8, u8, u8)> {
        use Color::*;
        match color {
            Reset => None,
            Black => Some((0, 0, 0)),
            Red => Some((205, 49, 49)),
            Green => Some((13, 188, 121)),
            Yellow => Some((229, 229, 16)),
            Blue => Some((36, 114, 200)),
            Magenta => Some((188, 63, 188)),
            Cyan => Some((17, 168, 205)),
            Gray => Some((204, 204, 204)),
            DarkGray => Some((102, 102, 102)),
            LightRed => Some((241, 76, 76)),
            LightGreen => Some((35, 209, 139)),
            LightYellow => Some((245, 245, 67)),
            LightBlue => Some((59, 142, 234)),
            LightMagenta => Some((214, 112, 214)),
            LightCyan => Some((41, 184, 219)),
            White => Some((255, 255, 255)),
            Rgb(r, g, b) => Some((r, g, b)),
            Indexed(i) => Some(xterm_index_to_rgb(i)),
        }
    }

    // ----- Pull baseline background -----
    let bg = theme
        .chat_list
        .get("self")
        .and_then(|s| s.bg)
        .and_then(color_to_rgb);
    let Some(chat_list_bg) = bg else {
        return;
    };

    // Minimum contrast ratios.
    // - Chat name: primary label (higher)
    // - Message preview: secondary text (slightly lower but still readable)
    const MIN_CR_NAME: f32 = 5.0;
    const MIN_CR_MSG: f32 = 4.2;
    // Minimum separation between name and message colors (squared distance in RGB space).
    // 40^2 = 1600
    const MIN_NAME_MSG_DIST2: u32 = 1600;
    // How strongly we "mute" message preview towards background for a more professional hierarchy.
    // (We still enforce MIN_CR_MSG after muting.)
    const MSG_MUTE_TOWARDS_BG: f32 = 0.35;
    // Ensure a noticeable luminance difference between name and message (helps when hues are similar).
    const MIN_NAME_MSG_LUMA_SEP: f32 = 0.12;

    // Adjust a foreground color to satisfy contrast against bg by blending towards white/black.
    fn ensure_contrast(fg: (u8, u8, u8), bg: (u8, u8, u8), min_ratio: f32) -> (u8, u8, u8) {
        if contrast_ratio(fg, bg) >= min_ratio {
            return fg;
        }
        let bg_l = rel_luminance(bg);
        let target = if bg_l < 0.5 {
            (255, 255, 255)
        } else {
            (0, 0, 0)
        };
        // Binary-search-ish stepping.
        let mut best = fg;
        let mut lo = 0.0f32;
        let mut hi = 1.0f32;
        for _ in 0..12 {
            let t = (lo + hi) * 0.5;
            let candidate = lerp(fg, target, t);
            if contrast_ratio(candidate, bg) >= min_ratio {
                best = candidate;
                hi = t;
            } else {
                lo = t;
            }
        }
        best
    }

    // Helper to mutate a ThemeStyle fg when it is RGB/Indexed/basic.
    fn set_fg_rgb(style: &mut Style, rgb: (u8, u8, u8)) {
        style.fg = Some(Color::Rgb(rgb.0, rgb.1, rgb.2));
    }

    // Grab chat list relevant colors
    let name_rgb = theme
        .chat_list
        .get("item_chat_name")
        .and_then(|s| s.fg)
        .and_then(color_to_rgb);
    let msg_rgb = theme
        .chat_list
        .get("item_message_content")
        .and_then(|s| s.fg)
        .and_then(color_to_rgb);

    // If missing, nothing to do.
    if name_rgb.is_none() || msg_rgb.is_none() {
        return;
    }
    let mut name_rgb_v = name_rgb.unwrap();
    let mut msg_rgb_v = msg_rgb.unwrap();

    // Ensure both are readable against background first.
    name_rgb_v = ensure_contrast(name_rgb_v, chat_list_bg, MIN_CR_NAME);
    msg_rgb_v = ensure_contrast(msg_rgb_v, chat_list_bg, MIN_CR_MSG);

    // Establish a professional visual hierarchy:
    // - keep chat name as the more prominent label
    // - mute message preview slightly towards background (secondary text), while keeping it readable
    {
        let muted = lerp(msg_rgb_v, chat_list_bg, MSG_MUTE_TOWARDS_BG);
        msg_rgb_v = ensure_contrast(muted, chat_list_bg, MIN_CR_MSG);
    }

    // Ensure name and message are clearly distinguishable (both by color distance and luminance).
    let mut name_l = rel_luminance(name_rgb_v);
    let mut msg_l = rel_luminance(msg_rgb_v);
    if rgb_distance(name_rgb_v, msg_rgb_v) < MIN_NAME_MSG_DIST2
        || (name_l - msg_l).abs() < MIN_NAME_MSG_LUMA_SEP
    {
        // Prefer keeping message as "secondary": try moving it closer to background first.
        let c_towards_bg = ensure_contrast(
            lerp(msg_rgb_v, chat_list_bg, 0.25),
            chat_list_bg,
            MIN_CR_MSG,
        );

        // Alternative: move it away from background (if towards-bg doesn't separate enough).
        let bg_l = rel_luminance(chat_list_bg);
        let away_target = if bg_l < 0.5 {
            (255, 255, 255)
        } else {
            (0, 0, 0)
        };
        let c_away = ensure_contrast(lerp(msg_rgb_v, away_target, 0.25), chat_list_bg, MIN_CR_MSG);

        let score = |cand: (u8, u8, u8)| -> (u32, f32) {
            let dist = rgb_distance(name_rgb_v, cand);
            let l_sep = (rel_luminance(cand) - name_l).abs();
            (dist, l_sep)
        };
        let s1 = score(c_towards_bg);
        let s2 = score(c_away);

        msg_rgb_v = if (s2.0, (s2.1 * 1000.0) as u32) > (s1.0, (s1.1 * 1000.0) as u32) {
            c_away
        } else {
            c_towards_bg
        };

        // Final nudge: if luminance is still too close, push message slightly opposite direction.
        name_l = rel_luminance(name_rgb_v);
        msg_l = rel_luminance(msg_rgb_v);
        if (name_l - msg_l).abs() < MIN_NAME_MSG_LUMA_SEP {
            let target = if msg_l >= name_l {
                // message too bright vs name → darken a bit (towards bg if bg is dark; towards black otherwise)
                if bg_l < 0.5 { chat_list_bg } else { (0, 0, 0) }
            } else {
                // message too dark vs name → brighten a bit (towards white)
                (255, 255, 255)
            };
            msg_rgb_v = ensure_contrast(lerp(msg_rgb_v, target, 0.15), chat_list_bg, MIN_CR_MSG);
        }
    }

    // Write back adjusted colors.
    if let Some(s) = theme.chat_list.get_mut("item_chat_name") {
        set_fg_rgb(s, name_rgb_v);
    }
    if let Some(s) = theme.chat_list.get_mut("item_message_content") {
        set_fg_rgb(s, msg_rgb_v);
    }
}
