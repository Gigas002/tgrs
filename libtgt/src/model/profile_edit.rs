//! Your own profile as you edit it: name, bio and username.

/// Longest first or last name Telegram takes.
pub const NAME_MAX_LENGTH: usize = 64;
/// Shortest username you can set yourself.
const USERNAME_MIN_LENGTH: usize = 5;
/// Longest username.
const USERNAME_MAX_LENGTH: usize = 32;

/// A field of your profile you can edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProfileField {
    FirstName,
    LastName,
    Bio,
    Username,
}

impl ProfileField {
    /// Every field, in the order they're edited.
    pub const ALL: [Self; 4] = [Self::FirstName, Self::LastName, Self::Bio, Self::Username];

    pub fn label(self) -> &'static str {
        match self {
            Self::FirstName => "First name",
            Self::LastName => "Last name",
            Self::Bio => "Bio",
            Self::Username => "Username",
        }
    }

    /// The field after this one, wrapping around.
    pub fn next(self) -> Self {
        let index = Self::ALL.iter().position(|f| *f == self).unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    /// The field before this one, wrapping around.
    pub fn previous(self) -> Self {
        let index = Self::ALL.iter().position(|f| *f == self).unwrap_or(0);
        Self::ALL[(index + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

/// The parts of your profile you can edit. The username is the editable one,
/// without `@`; collectible usernames can't be changed here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileEdit {
    pub first_name: String,
    pub last_name: String,
    pub bio: String,
    pub username: String,
}

/// What saving an edit changes on Telegram, one request each.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileChange {
    Name {
        first_name: String,
        last_name: String,
    },
    Bio(String),
    Username(String),
}

impl ProfileChange {
    /// The field to point at when this change fails.
    pub fn field(&self) -> ProfileField {
        match self {
            Self::Name { .. } => ProfileField::FirstName,
            Self::Bio(_) => ProfileField::Bio,
            Self::Username(_) => ProfileField::Username,
        }
    }
}

impl ProfileEdit {
    pub fn get(&self, field: ProfileField) -> &str {
        match field {
            ProfileField::FirstName => &self.first_name,
            ProfileField::LastName => &self.last_name,
            ProfileField::Bio => &self.bio,
            ProfileField::Username => &self.username,
        }
    }

    pub fn set(&mut self, field: ProfileField, value: impl Into<String>) {
        let value = value.into();
        match field {
            ProfileField::FirstName => self.first_name = value,
            ProfileField::LastName => self.last_name = value,
            ProfileField::Bio => self.bio = value,
            ProfileField::Username => self.username = value,
        }
    }

    /// This edit with surrounding spaces and the `@` of the username left
    /// out, as it's saved.
    pub fn normalized(&self) -> Self {
        let username = self.username.trim();
        Self {
            first_name: self.first_name.trim().to_owned(),
            last_name: self.last_name.trim().to_owned(),
            bio: self.bio.trim().to_owned(),
            username: username.strip_prefix('@').unwrap_or(username).to_owned(),
        }
    }

    /// Why this edit can't be saved, and the field to fix; None if it can be
    /// tried. Telegram has the last word: a bio's length limit depends on
    /// Premium and a username may be taken.
    pub fn problem(&self) -> Option<(ProfileField, String)> {
        let edit = self.normalized();
        if edit.first_name.is_empty() {
            return Some((
                ProfileField::FirstName,
                "The first name can't be empty".into(),
            ));
        }
        for field in [ProfileField::FirstName, ProfileField::LastName] {
            if edit.get(field).chars().count() > NAME_MAX_LENGTH {
                let message = format!("{} has at most {NAME_MAX_LENGTH} characters", field.label());
                return Some((field, message));
            }
        }
        let username = &edit.username;
        let valid_username = username.is_empty()
            || ((USERNAME_MIN_LENGTH..=USERNAME_MAX_LENGTH).contains(&username.len())
                && username.starts_with(|c: char| c.is_ascii_alphabetic())
                && username
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !username.ends_with('_'));
        if !valid_username {
            return Some((
                ProfileField::Username,
                "A username has 5-32 letters, digits or _".into(),
            ));
        }
        None
    }

    /// The requests that turn `original` into this edit, in order: the name,
    /// the bio, then the username. Empty when nothing changed.
    pub fn changes(&self, original: &Self) -> Vec<ProfileChange> {
        let (edit, original) = (self.normalized(), original.normalized());
        let mut changes = Vec::new();
        if edit.first_name != original.first_name || edit.last_name != original.last_name {
            changes.push(ProfileChange::Name {
                first_name: edit.first_name,
                last_name: edit.last_name,
            });
        }
        if edit.bio != original.bio {
            changes.push(ProfileChange::Bio(edit.bio));
        }
        if edit.username != original.username {
            changes.push(ProfileChange::Username(edit.username));
        }
        changes
    }
}

/// What to tell when Telegram refuses `change`, and the field to fix.
pub fn profile_change_failure(change: &ProfileChange, error: &str) -> (ProfileField, String) {
    let message = match change {
        ProfileChange::Username(name) if error.contains("OCCUPIED") => {
            format!("@{name} is taken")
        }
        ProfileChange::Username(name) if error.contains("INVALID") => {
            format!("@{name} isn't a valid username")
        }
        ProfileChange::Username(_) if error.contains("FLOOD") => {
            "Too many username changes, try again later".to_owned()
        }
        ProfileChange::Bio(_) if error.contains("TOO_LONG") => "The bio is too long".to_owned(),
        ProfileChange::Name { .. } if error.contains("FIRSTNAME_INVALID") => {
            "The first name isn't valid".to_owned()
        }
        ProfileChange::Name { .. } if error.contains("LASTNAME_INVALID") => {
            return (
                ProfileField::LastName,
                "The last name isn't valid".to_owned(),
            );
        }
        change => format!("Can't save the {}: {error}", noun(change)),
    };
    (change.field(), message)
}

fn noun(change: &ProfileChange) -> &'static str {
    match change {
        ProfileChange::Name { .. } => "name",
        ProfileChange::Bio(_) => "bio",
        ProfileChange::Username(_) => "username",
    }
}
