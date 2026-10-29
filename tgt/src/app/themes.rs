//! Switching between the discovered themes at runtime.

use crate::{
    app::{context::AppContext, error::AppError},
    settings,
};

/// Cycles through theme names, tracking the selected one.
#[derive(Debug, Clone, Default)]
pub struct ThemeSwitcher {
    /// Index of the current theme in `themes`.
    current_theme_index: usize,
    themes: Vec<String>,
}

impl ThemeSwitcher {
    pub fn with_themes(themes: Vec<String>) -> Self {
        Self {
            current_theme_index: 0,
            themes,
        }
    }

    /// All discovered themes, positioned at the active one.
    pub fn from_context(app_context: &AppContext) -> Self {
        let mut switcher = Self::with_themes(
            app_context
                .themes()
                .iter()
                .map(|(name, _)| name.clone())
                .collect(),
        );
        let current = app_context.theme().name.clone();
        if switcher.switch_to_theme(&current).is_err() {
            tracing::debug!("active theme {current:?} is not among the discovered themes");
        }
        switcher
    }

    pub fn current_theme(&self) -> Option<&str> {
        self.themes
            .get(self.current_theme_index)
            .map(String::as_str)
    }

    pub fn available_themes(&self) -> &[String] {
        &self.themes
    }

    pub fn next_theme(&mut self) -> Option<&str> {
        if self.themes.is_empty() {
            return None;
        }
        self.current_theme_index = (self.current_theme_index + 1) % self.themes.len();
        self.current_theme()
    }

    pub fn previous_theme(&mut self) -> Option<&str> {
        if self.themes.is_empty() {
            return None;
        }
        self.current_theme_index = self
            .current_theme_index
            .checked_sub(1)
            .unwrap_or(self.themes.len() - 1);
        self.current_theme()
    }

    pub fn switch_to_theme(&mut self, theme_name: &str) -> Result<(), AppError> {
        let index = self
            .themes
            .iter()
            .position(|t| t == theme_name)
            .ok_or_else(|| AppError::InvalidAction(format!("theme {theme_name:?} not found")))?;
        self.current_theme_index = index;
        Ok(())
    }

    /// Applies the selected theme.
    pub fn apply_current_theme(&self, app_context: &AppContext) -> Result<(), AppError> {
        let name = self
            .current_theme()
            .ok_or_else(|| AppError::InvalidAction("no theme available".into()))?;
        Self::apply_theme(app_context, name)
    }

    /// Loads `theme_name`, makes it active and remembers it for the next start.
    pub fn apply_theme(app_context: &AppContext, theme_name: &str) -> Result<(), AppError> {
        let theme = settings::load_theme(app_context.themes(), theme_name)?;
        app_context.set_theme(theme);
        tracing::info!("switched to theme {theme_name}");

        let state_file = app_context.dirs().theme_state_file();
        let persisted = state_file
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&state_file, theme_name));
        if let Err(err) = persisted {
            tracing::warn!(
                "failed to remember theme in {}: {err}",
                state_file.display()
            );
        }
        Ok(())
    }
}
