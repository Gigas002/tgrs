//! Interactive login on the terminal, before the TUI starts.

use std::io::{self, BufRead, Write};

use libtgt::client::Authenticator;

/// Prompts on stdout and reads answers from stdin.
pub struct StdinAuthenticator;

impl StdinAuthenticator {
    fn ask(prompt: &str) -> String {
        print!("{prompt} ");
        let _ = io::stdout().flush();
        let mut input = String::new();
        if let Err(err) = io::stdin().lock().read_line(&mut input) {
            tracing::error!("failed to read from stdin: {err}");
        }
        input.trim().to_owned()
    }
}

impl Authenticator for StdinAuthenticator {
    fn phone_number(&mut self) -> String {
        Self::ask("Enter your phone number (include the country calling code):")
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
        Self::ask("Enter your password:")
    }

    fn registration(&mut self) -> (String, String) {
        (
            Self::ask("Enter your first name:"),
            Self::ask("Enter your last name:"),
        )
    }

    fn confirm_link(&mut self, link: &str) {
        println!("Confirm this login link on another device: {link}");
    }

    fn report_error(&mut self, message: &str) {
        eprintln!("tgt: {message}");
    }
}
