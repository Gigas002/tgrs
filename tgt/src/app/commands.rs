//! Subcommands that don't open the TUI and don't need Telegram.

use std::{
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
};

use crate::{app::error::AppError, config::BUNDLED_FILES, settings::Dirs};

/// Writes the bundled config files into the config directory. Existing files
/// are kept unless `force` is set.
pub fn init_config(dirs: &Dirs, force: bool) -> Result<(), AppError> {
    for (relative, content) in BUNDLED_FILES {
        let dest = dirs.config.join(relative);
        if dest.exists() && !force {
            println!("Kept:    {}", dest.display());
            continue;
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&dest, content)?;
        println!("Written: {}", dest.display());
    }
    Ok(())
}

/// Removes the selected tgt directories, asking first unless `yes` is set.
pub fn clear(
    dirs: &Dirs,
    config: bool,
    data: bool,
    state: bool,
    yes: bool,
) -> Result<(), AppError> {
    if !(config || data || state) {
        return Err(AppError::Command(
            "nothing to clear: pass --config, --data, --state or --all".into(),
        ));
    }

    let targets: Vec<(&str, &PathBuf)> = [
        (config, "config", &dirs.config),
        (data, "data", &dirs.data),
        (state, "state", &dirs.state),
    ]
    .into_iter()
    .filter(|(selected, _, path)| *selected && path.exists())
    .map(|(_, label, path)| (label, path))
    .collect();

    if targets.is_empty() {
        println!("Nothing to clear.");
        return Ok(());
    }
    if let Some((label, path)) = targets.iter().find(|(_, path)| is_unsafe_to_remove(path)) {
        return Err(AppError::Command(format!(
            "refusing to remove {label} directory {}: it looks like a project or home directory",
            path.display()
        )));
    }

    if !yes {
        println!("The following will be removed:");
        for (label, path) in &targets {
            println!("  {label}: {}", path.display());
        }
        if !confirm("Proceed? [y/N] ", io::stdin().lock())? {
            println!("Aborted.");
            return Ok(());
        }
    }

    for (label, path) in targets {
        std::fs::remove_dir_all(path)?;
        println!("Removed {label}: {}", path.display());
    }
    Ok(())
}

/// Guards against a misconfigured directory pointing at something precious.
pub(crate) fn is_unsafe_to_remove(path: &Path) -> bool {
    path.parent().is_none()
        || ["Cargo.toml", ".git", ".bashrc", ".profile"]
            .iter()
            .any(|name| path.join(name).exists())
}

pub(crate) fn confirm(prompt: &str, mut input: impl BufRead) -> io::Result<bool> {
    print!("{prompt}");
    io::stdout().flush()?;
    let mut line = String::new();
    input.read_line(&mut line)?;
    Ok(matches!(
        line.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}
