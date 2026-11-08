//! Public usernames as people type or paste them.

/// Shortest public username (collectible ones have 4 characters).
const MIN_LENGTH: usize = 4;
/// Longest public username.
const MAX_LENGTH: usize = 32;

/// The username in `input`: `name`, `@name` or a `t.me/name` link, with or
/// without `https://`. None if it isn't a valid username: 4 to 32 Latin
/// letters, digits and underscores, starting with a letter.
pub fn parse_username(input: &str) -> Option<String> {
    let input = input.trim();
    let input = input
        .strip_prefix("https://")
        .or_else(|| input.strip_prefix("http://"))
        .unwrap_or(input);
    let name = ["t.me/", "telegram.me/", "telegram.dog/"]
        .iter()
        .find_map(|host| input.strip_prefix(host))
        // A link may go on to a message or have a query
        .map(|rest| rest.split(['/', '?', '#']).next().unwrap_or_default())
        .unwrap_or_else(|| input.strip_prefix('@').unwrap_or(input));
    let valid = (MIN_LENGTH..=MAX_LENGTH).contains(&name.len())
        && name.starts_with(|c: char| c.is_ascii_alphabetic())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    valid.then(|| name.to_owned())
}
