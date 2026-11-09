//! Interactive login on the terminal, before the TUI starts.

use std::io::{self, BufRead, IsTerminal, Write};

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    terminal,
};
use libtgrs::client::{Authenticator, Login};
use qrcode::{QrCode, render::unicode::Dense1x2};

/// Prompts on stdout and reads answers from stdin.
#[derive(Default)]
pub struct StdinAuthenticator {
    /// A QR code was shown already, so the next one replaces an expired one.
    qr_code_shown: bool,
}

impl StdinAuthenticator {
    fn ask(prompt: &str) -> String {
        print!("{prompt} ");
        let _ = io::stdout().flush();
        Self::ask_after_prompt()
    }

    /// Like [`Self::ask`], but echoes `*` for every typed character.
    fn ask_secret(prompt: &str) -> String {
        if !io::stdin().is_terminal() {
            return Self::ask(prompt);
        }
        print!("{prompt} ");
        let _ = io::stdout().flush();
        let raw = match RawMode::enable() {
            Ok(raw) => raw,
            Err(err) => {
                tracing::error!("failed to hide the input: {err}");
                return Self::ask_after_prompt();
            }
        };
        let keys = std::iter::from_fn(|| match event::read() {
            Ok(Event::Key(key)) => Some(Some(key)),
            Ok(_) => Some(None),
            Err(err) => {
                tracing::error!("failed to read from the terminal: {err}");
                None
            }
        })
        .flatten();
        let secret = read_masked(keys, &mut io::stdout());
        drop(raw);
        secret.unwrap_or_else(|| {
            // Ctrl+C: raw mode swallows SIGINT, so quit as the signal would
            std::process::exit(130)
        })
    }

    fn ask_after_prompt() -> String {
        let mut input = String::new();
        match io::stdin().lock().read_line(&mut input) {
            Ok(0) => {
                // End of input (Ctrl+D): asking again would loop forever
                println!();
                std::process::exit(1)
            }
            Ok(_) => input.trim().to_owned(),
            Err(err) => {
                tracing::error!("failed to read from stdin: {err}");
                eprintln!("tgrs: failed to read from stdin: {err}");
                std::process::exit(1)
            }
        }
    }
}

/// What was answered to the login prompt: `qr` (in any case) for a QR code,
/// anything else is a phone number.
pub(crate) fn parse_login(answer: &str) -> Login {
    if answer.trim().eq_ignore_ascii_case("qr") {
        Login::QrCode
    } else {
        Login::PhoneNumber(answer.trim().to_owned())
    }
}

/// `link` as a QR code of half-height blocks, with its quiet zone. With
/// `colors`, it's drawn black on white, as scanners expect even on a dark
/// terminal; without, dark modules are the terminal's text color.
pub(crate) fn qr_code_text(link: &str, colors: bool) -> Option<String> {
    let code = QrCode::new(link).ok()?;
    let text = code.render::<Dense1x2>().quiet_zone(true).build();
    if !colors {
        return Some(text);
    }
    Some(
        text.lines()
            .map(|line| format!("\x1b[30;107m{line}\x1b[0m"))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

/// Restores the terminal when dropped, also on panic.
struct RawMode;

impl RawMode {
    fn enable() -> io::Result<Self> {
        terminal::enable_raw_mode().map(|()| Self)
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

/// Reads a line from `keys`, writing `*` to `out` for each character.
/// Backspace removes the last character and Ctrl+U the whole line. Returns
/// `None` on Ctrl+C.
pub(crate) fn read_masked(
    keys: impl IntoIterator<Item = KeyEvent>,
    out: &mut impl Write,
) -> Option<String> {
    let mut secret = String::new();
    for key in keys {
        if key.kind == KeyEventKind::Release {
            continue;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Enter => break,
            KeyCode::Char('c') if ctrl => {
                let _ = write!(out, "\r\n");
                let _ = out.flush();
                return None;
            }
            KeyCode::Char('u') if ctrl => {
                for _ in secret.chars() {
                    let _ = write!(out, "\x08 \x08");
                }
                secret.clear();
            }
            KeyCode::Char(_) if ctrl => {}
            KeyCode::Char(c) => {
                secret.push(c);
                let _ = write!(out, "*");
            }
            KeyCode::Backspace if secret.pop().is_some() => {
                let _ = write!(out, "\x08 \x08");
            }
            _ => {}
        }
        let _ = out.flush();
    }
    // Raw mode doesn't translate newlines
    let _ = write!(out, "\r\n");
    let _ = out.flush();
    Some(secret)
}

impl Authenticator for StdinAuthenticator {
    fn login(&mut self) -> Login {
        parse_login(&Self::ask(
            "Enter your phone number (include the country calling code), or `qr` to log in with a QR code:",
        ))
    }

    fn code(&mut self) -> String {
        Self::ask("Enter the verification code:")
    }

    fn email_address(&mut self) -> String {
        Self::ask("Enter your email address:")
    }

    fn email_code(&mut self) -> String {
        Self::ask("Enter the email authentication code:")
    }

    fn password(&mut self) -> String {
        Self::ask_secret("Enter your password:")
    }

    fn registration(&mut self) -> (String, String) {
        (
            Self::ask("Enter your first name:"),
            Self::ask("Enter your last name:"),
        )
    }

    fn confirm_link(&mut self, link: &str) {
        if std::mem::replace(&mut self.qr_code_shown, true) {
            println!("\nThe QR code expired, scan this one instead:");
        } else {
            println!(
                "On a phone logged in to Telegram, open Settings > Devices > Link Desktop Device and scan this QR code:"
            );
        }
        match qr_code_text(link, io::stdout().is_terminal()) {
            Some(code) => println!("{code}"),
            None => tracing::error!("the login link doesn't fit in a QR code"),
        }
        println!("Or open this link there: {link}");
    }

    fn report_error(&mut self, message: &str) {
        eprintln!("tgrs: {message}");
    }
}
