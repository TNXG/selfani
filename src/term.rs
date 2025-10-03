use once_cell::sync::Lazy;
use std::env;

// Detect whether to enable colors: only on TTY and when NO_COLOR is not set
static COLOR_ENABLED: Lazy<bool> = Lazy::new(|| {
    let no_color = env::var_os("NO_COLOR").is_some();
    let is_tty = is_terminal::is_terminal(&std::io::stderr());
    is_tty && !no_color
});

pub fn paint<T: std::fmt::Display>(text: T, style: Style) -> String {
    if !*COLOR_ENABLED { return text.to_string(); }
    use owo_colors::OwoColorize;
    match style {
        Style::Title => text.bold().bright_blue().to_string(),
        Style::Label => text.bold().green().to_string(),
        Style::Dim => text.bright_black().to_string(),
        Style::Warn => text.bold().yellow().to_string(),
        Style::Err => text.bold().red().to_string(),
        Style::Ok => text.bold().bright_green().to_string(),
        Style::Info => text.cyan().to_string(),
    }
}

#[allow(dead_code)]
pub enum Style { Title, Label, Dim, Warn, Err, Ok, Info }
