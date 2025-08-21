//! Secure display for sensitive cryptographic data (mnemonics and private keys).

use crate::secure_structs::SecureString;
use bitcoin::secp256k1::SecretKey;
use colored::*;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind, poll},
    execute,
    style::{Color, Print, SetForegroundColor},
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use eyre::{Result, eyre};
use secrecy::ExposeSecret;
use std::io::{self, IsTerminal, Write};
use std::time::{Duration, Instant};

/// Display timeout for individual words (30 seconds)
const WORD_TIMEOUT_SECS: u64 = 30;
const WORD_TIMEOUT_DURATION: Duration = Duration::from_secs(WORD_TIMEOUT_SECS);

/// Display timeout for private keys (30 seconds)
const PRIVATE_KEY_TIMEOUT_SECS: u64 = 30;
const PRIVATE_KEY_TIMEOUT_DURATION: Duration = Duration::from_secs(PRIVATE_KEY_TIMEOUT_SECS);

/// Polling interval for event checking (100ms)
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Position for countdown display in alternate screen
const COUNTDOWN_CURSOR_Y: u16 = 15;
const PRIVATE_KEY_COUNTDOWN_Y: u16 = 12;

/// Custom error types for secure display operations
#[derive(Debug)]
pub enum SecureDisplayError {
    /// Terminal does not support required features
    TerminalUnsupported(String),
    /// Failed to enable raw mode
    RawModeError(String),
    /// Failed to enter alternate screen
    AlternateScreenError(String),
    /// Failed to clear screen
    ClearScreenError(String),
    /// Failed to display content
    DisplayError(String),
    /// Failed to read user input
    InputError(String),
    /// User cancelled the operation
    UserCancelled,
    /// IO error occurred
    IoError(std::io::Error),
}

impl std::fmt::Display for SecureDisplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SecureDisplayError::TerminalUnsupported(msg) => {
                write!(f, "Terminal does not support required features: {}", msg)
            }
            SecureDisplayError::RawModeError(msg) => {
                write!(f, "Failed to enable raw mode: {}", msg)
            }
            SecureDisplayError::AlternateScreenError(msg) => {
                write!(f, "Failed to enter alternate screen: {}", msg)
            }
            SecureDisplayError::ClearScreenError(msg) => {
                write!(f, "Failed to clear screen: {}", msg)
            }
            SecureDisplayError::DisplayError(msg) => {
                write!(f, "Failed to display content: {}", msg)
            }
            SecureDisplayError::InputError(msg) => write!(f, "Failed to read user input: {}", msg),
            SecureDisplayError::UserCancelled => write!(f, "Operation cancelled by user"),
            SecureDisplayError::IoError(err) => write!(f, "IO error: {}", err),
        }
    }
}

impl std::error::Error for SecureDisplayError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SecureDisplayError::IoError(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for SecureDisplayError {
    fn from(err: std::io::Error) -> Self {
        SecureDisplayError::IoError(err)
    }
}

/// Type alias for Results with SecureDisplayError
pub type SecureResult<T> = std::result::Result<T, SecureDisplayError>;

/// Common functionality for secure display operations
pub trait SecureDisplay {
    /// Display content securely with fallback support
    fn display_securely(&mut self) -> Result<()>;

    /// Display content in alternate screen mode
    fn display_in_alternate_screen(&self) -> Result<()>;

    /// Display content using fallback mode (normal terminal)
    fn display_fallback(&self) -> Result<()>;
}

/// Display a security header with the given title
fn display_security_header(title: &str) -> Result<()> {
    execute!(
        io::stdout(),
        SetForegroundColor(Color::Red),
        Print(format!("🔐 {}\r\n", title)),
        Print("Keep Secret!\r\n\r\n"),
        SetForegroundColor(Color::Reset)
    )
    .map_err(|e| anyhow!("Failed to display header: {}", e))?;
    Ok(())
}

/// Handle timeout waiting with threading (shared between fallback displays)
fn handle_timeout_wait(timeout_duration: Duration, prompt_msg: &str) -> Result<()> {
    print!("{}", prompt_msg);
    io::stdout().flush()?;

    // Use a separate thread to handle the timeout
    let (tx, rx) = std::sync::mpsc::channel();

    // Spawn thread for user input
    let tx_input = tx.clone();
    std::thread::spawn(move || {
        let mut input = String::new();
        if std::io::stdin().read_line(&mut input).is_ok() {
            let _ = tx_input.send(true);
        }
    });

    // Spawn thread for timeout
    std::thread::spawn(move || {
        std::thread::sleep(timeout_duration);
        let _ = tx.send(false);
    });

    // Wait for timeout or user input
    rx.recv().unwrap_or(false);
    Ok(())
}

/// Terminal helper for managing alternate screen and raw mode
struct TerminalHelper {
    /// Whether alternate screen is currently active
    alternate_screen_active: bool,
    /// Whether raw mode is currently active
    raw_mode_active: bool,
}

impl TerminalHelper {
    /// Create a new terminal helper
    fn new() -> Self {
        Self {
            alternate_screen_active: false,
            raw_mode_active: false,
        }
    }

    /// Check if terminal supports the features we need
    fn is_alternate_screen_supported(&self) -> bool {
        // Must be a real terminal (not redirected)
        if !std::io::stdout().is_terminal() {
            return false;
        }

        // Check if we can detect terminal size (good indicator of terminal features)
        if let Ok((_, _)) = terminal::size() {
            // Additional check: see if we're not in a non-interactive environment
            std::env::var("TERM").is_ok_and(|term| !term.is_empty() && term != "dumb")
        } else {
            false
        }
    }

    /// Enter alternate screen mode with proper error handling
    fn enter_alternate_screen(&mut self) -> SecureResult<()> {
        // Check if we're in a real terminal first
        if !self.is_alternate_screen_supported() {
            return Err(SecureDisplayError::TerminalUnsupported(
                "Terminal does not support alternate screen features".to_string(),
            ));
        }

        // Try to enable raw mode first (safer to fail here than after screen change)
        terminal::enable_raw_mode().map_err(|e| {
            SecureDisplayError::RawModeError(format!("Failed to enable raw mode: {}", e))
        })?;
        self.raw_mode_active = true;

        // Only enter alternate screen if raw mode worked
        match execute!(io::stdout(), EnterAlternateScreen) {
            Ok(()) => {
                self.alternate_screen_active = true;
                Ok(())
            }
            Err(e) => {
                // Clean up raw mode if alternate screen failed
                self.cleanup_raw_mode();
                Err(SecureDisplayError::AlternateScreenError(format!(
                    "Failed to enter alternate screen: {}",
                    e
                )))
            }
        }
    }

    /// Clear the screen and position cursor at top
    fn clear_screen(&self) -> SecureResult<()> {
        execute!(
            io::stdout(),
            terminal::Clear(terminal::ClearType::All),
            cursor::MoveTo(0, 0)
        )
        .map_err(|e| {
            SecureDisplayError::ClearScreenError(format!("Failed to clear screen: {}", e))
        })?;
        Ok(())
    }

    /// Cleanup raw mode
    fn cleanup_raw_mode(&mut self) {
        if self.raw_mode_active {
            let _ = terminal::disable_raw_mode();
            self.raw_mode_active = false;
        }
    }

    /// Cleanup alternate screen and return to normal mode
    fn cleanup(&mut self) {
        if self.alternate_screen_active {
            let _ = execute!(io::stdout(), LeaveAlternateScreen);
            // Reset terminal colors when leaving alternate screen
            let _ = execute!(io::stdout(), SetForegroundColor(Color::Reset));
            self.alternate_screen_active = false;
        }

        self.cleanup_raw_mode();

        // Flush stdout to ensure all output is written
        let _ = io::stdout().flush();
    }
}

impl Drop for TerminalHelper {
    fn drop(&mut self) {
        self.cleanup();
    }
}

/// Secure display manager for sensitive information like mnemonic phrases
/// Uses alternate screen to prevent shell history contamination
pub struct SecureMnemonicDisplay<'a> {
    /// The secure mnemonic phrase to display
    mnemonic: &'a SecureString,
    /// Terminal helper for managing alternate screen and raw mode
    terminal: TerminalHelper,
}

impl<'a> SecureMnemonicDisplay<'a> {
    /// Create a new secure display instance
    pub fn new(mnemonic: &'a SecureString) -> Self {
        Self {
            mnemonic,
            terminal: TerminalHelper::new(),
        }
    }

    /// Display the mnemonic in a secure alternate screen
    pub fn display_securely(&mut self) -> Result<()> {
        // Show pre-display warning
        self.show_timeout_warning()?;

        // Try to enter alternate screen, fallback to normal display if it fails
        match self.terminal.enter_alternate_screen() {
            Ok(()) => {
                // Cleanup is handled by Drop trait
                self.display_in_alternate_screen()
            }
            Err(e) => {
                // Ensure we're in a clean state before showing fallback
                self.terminal.cleanup();

                eprintln!(
                    "{} Could not enter secure display mode: {}",
                    "WARNING".yellow().bold(),
                    e
                );
                eprintln!("{} Falling back to standard display", "INFO".blue().bold());
                self.display_fallback()
            }
        }
    }

    /// Show timeout warning before displaying the mnemonic
    fn show_timeout_warning(&self) -> Result<()> {
        println!("Have pen and paper ready.");
        print!("Press Enter to begin... ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(|e| eyre!("Failed to read user input: {}", e))?;

        io::stdin().read_line(&mut input)?;
        Ok(())
    }

    /// Display mnemonic with security warnings and user interaction
    fn display_in_alternate_screen(&self) -> Result<()> {
        self.terminal.clear_screen()?;
        self.display_header()?;
        self.display_mnemonic_step_by_step()?;
        self.display_completion_message()?;
        Ok(())
    }

    /// Display the security header
    fn display_header(&self) -> Result<()> {
        display_security_header("MNEMONIC")
    }

    /// Display the mnemonic words step by step, one word at a time
    fn display_mnemonic_step_by_step(&self) -> Result<()> {
        let words: Vec<SecureString> = self
            .mnemonic
            .expose_secret()
            .split_whitespace()
            .map(|w| SecureString::init_with(|| w.to_string()))
            .collect();

        for (index, word) in words.iter().enumerate() {
            let word_num = index + 1;
            let total_words = words.len();

            // Clear screen and show header for each word
            self.terminal.clear_screen()?;
            self.display_header()?;

            let word = word.expose_secret();

            // Display progress and current word
            execute!(
                io::stdout(),
                SetForegroundColor(Color::Cyan),
                Print(format!("Word {word_num}/{total_words}: {word}\r\n\r\n")),
                SetForegroundColor(Color::Red),
                Print("Keep secret!\r\n\r\n"),
                SetForegroundColor(Color::Green),
                Print("Press Enter to continue..."),
                SetForegroundColor(Color::Reset)
            )
            .map_err(|e| eyre!("Failed to display word {}: {}", word_num, e))?;

            // Wait for user input or timeout (30 seconds per word)
            if let Err(e) = self.wait_for_word_confirmation() {
                return Err(eyre!("Error during word display: {}", e));
            }
        }

        Ok(())
    }

    /// Wait for user confirmation for each word with timeout
    fn wait_for_word_confirmation(&self) -> Result<()> {
        let start_time = Instant::now();

        loop {
            let elapsed = start_time.elapsed();
            let remaining = WORD_TIMEOUT_DURATION.saturating_sub(elapsed);

            if remaining.is_zero() {
                // Timeout reached, automatically proceed to next word
                return Ok(());
            }

            // Update countdown display for current word
            self.update_word_countdown_display(remaining)?;

            // Check for user input with a short timeout
            if poll(POLL_INTERVAL).map_err(|e| anyhow!("Failed to poll for input: {}", e))?
                && let Event::Key(key_event) =
                    event::read().map_err(|e| anyhow!("Failed to read user input: {}", e))?
                && key_event.kind == KeyEventKind::Press
            {
                match key_event.code {
                    KeyCode::Enter => {
                        // User pressed Enter, proceed to next word
                        return Ok(());
                    }
                    KeyCode::Esc => {
                        return Err(anyhow!("User cancelled mnemonic display"));
                    }
                    _ => {
                        // Ignore other keys
                        continue;
                    }
                }
            }
        }
    }

    /// Update countdown display for individual word
    fn update_word_countdown_display(&self, remaining: Duration) -> Result<()> {
        let seconds_left = remaining.as_secs();

        // Position cursor at bottom of screen for countdown
        execute!(
            io::stdout(),
            cursor::MoveTo(0, COUNTDOWN_CURSOR_Y),
            SetForegroundColor(Color::Yellow),
            Print(format!("⏰ Auto-advance in {seconds_left} seconds ")),
            SetForegroundColor(Color::Blue),
            Print("| Press Enter to continue immediately | Press ESC to cancel"),
            SetForegroundColor(Color::Reset)
        )
        .map_err(|e| eyre!("Failed to update word countdown: {}", e))?;

        Ok(())
    }

    /// Display completion message after all words have been shown
    fn display_completion_message(&self) -> Result<()> {
        self.terminal.clear_screen()?;

        execute!(
            io::stdout(),
            SetForegroundColor(Color::Green),
            Print("✅ MNEMONIC COMPLETE\r\n\r\n"),
            SetForegroundColor(Color::Yellow),
            Print("Store securely and never share.\r\n"),
            Print("Press any key to exit..."),
            SetForegroundColor(Color::Reset)
        )
        .map_err(|e| eyre!("Failed to display completion message: {}", e))?;

        // Wait for final confirmation
        loop {
            if poll(POLL_INTERVAL).map_err(|e| anyhow!("Failed to poll for input: {}", e))?
                && let Event::Key(key_event) =
                    event::read().map_err(|e| anyhow!("Failed to read user input: {}", e))?
                && key_event.kind == KeyEventKind::Press
            {
                return Ok(());
            }
        }
    }

    /// Fallback display method when alternate screen is not available
    fn display_fallback(&self) -> Result<()> {
        println!("{}", "🔐 MNEMONIC DISPLAY".red().bold());

        let words: Vec<SecureString> = self
            .mnemonic
            .expose_secret()
            .split_whitespace()
            .map(|w| SecureString::init_with(|| w.to_string()))
            .collect();

        for (index, word) in words.iter().enumerate() {
            let word_num = index + 1;
            let total_words = words.len();
            let word = word.expose_secret();

            println!("\nWord {word_num}/{total_words}: {word}");
            print!("Press Enter... ");

            self.fallback_word_timeout_wait()?;
        }

        println!("\n{}", "✅ Complete! Store securely.".green());
        print!("Press Enter to exit... ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        Ok(())
    }

    /// Handle timeout for each word in fallback mode
    fn fallback_word_timeout_wait(&self) -> Result<()> {
        handle_timeout_wait(WORD_TIMEOUT_DURATION, "")
    }
}

impl<'a> Drop for SecureMnemonicDisplay<'a> {
    fn drop(&mut self) {
        // TerminalHelper handles its own cleanup via Drop trait
        // SecureString handles its own zeroization
    }
}

/// Secure display manager for private keys
pub struct SecurePrivateKeyDisplay {
    /// The private key to display
    private_key: SecretKey,
    /// Terminal helper for managing alternate screen and raw mode
    terminal: TerminalHelper,
}

impl SecurePrivateKeyDisplay {
    /// Create a new secure private key display instance
    pub fn new(private_key: SecretKey) -> Self {
        Self {
            private_key,
            terminal: TerminalHelper::new(),
        }
    }

    /// Display the private key securely
    pub fn display_securely(&mut self) -> Result<()> {
        // Try to enter alternate screen, fallback to normal display if it fails
        match self.terminal.enter_alternate_screen() {
            Ok(()) => self.display_in_alternate_screen(),
            Err(_) => self.display_fallback(),
        }
    }

    /// Display private key in alternate screen
    fn display_in_alternate_screen(&self) -> Result<()> {
        self.terminal.clear_screen()?;
        self.display_header()?;
        self.display_private_key_content()?;
        self.wait_for_user_confirmation()?;
        Ok(())
    }

    /// Display the security header for private key
    fn display_header(&self) -> Result<()> {
        display_security_header("PRIVATE KEY")?;
        // Add extra newlines for private key display
        println!();
        Ok(())
    }

    /// Display the private key content
    fn display_private_key_content(&self) -> Result<()> {
        execute!(
            io::stdout(),
            SetForegroundColor(Color::Cyan),
            Print(format!("{}\r\n\r\n", self.private_key.display_secret())),
            SetForegroundColor(Color::Red),
            Print("Keep secret!\r\n\r\n"),
            SetForegroundColor(Color::Green),
            Print("Press any key to close (auto-close in 30s)..."),
            SetForegroundColor(Color::Reset)
        )
        .map_err(|e| anyhow!("Failed to display private key content: {}", e))?;
        Ok(())
    }

    /// Wait for user confirmation with timeout
    fn wait_for_user_confirmation(&self) -> Result<()> {
        let start_time = Instant::now();

        loop {
            let elapsed = start_time.elapsed();
            let remaining = PRIVATE_KEY_TIMEOUT_DURATION.saturating_sub(elapsed);

            if remaining.is_zero() {
                break;
            }

            // Update countdown display
            let seconds_left = remaining.as_secs();
            execute!(
                io::stdout(),
                cursor::MoveTo(0, PRIVATE_KEY_COUNTDOWN_Y),
                SetForegroundColor(Color::Yellow),
                Print(format!(
                    "⏰ Auto-close in {} seconds | Press any key to close immediately   ",
                    seconds_left
                )),
                SetForegroundColor(Color::Reset)
            )
            .map_err(|e| anyhow!("Failed to update countdown: {}", e))?;

            if poll(POLL_INTERVAL)?
                && let Event::Key(key_event) = event::read()?
                && key_event.kind == KeyEventKind::Press
            {
                break;
            }
        }

        Ok(())
    }

    /// Fallback display for private key when alternate screen is not available
    fn display_fallback(&self) -> Result<()> {
        println!("{}", "🔐 PRIVATE KEY".red().bold());
        println!("{}", "Keep secret!".red());
        println!("{}", self.private_key.display_secret());
        print!("Press Enter to close... ");
        io::stdout().flush()?;

        self.fallback_timeout_wait()
    }

    /// Handle timeout for private key display in fallback mode
    fn fallback_timeout_wait(&self) -> Result<()> {
        handle_timeout_wait(PRIVATE_KEY_TIMEOUT_DURATION, "")
    }
}

/// Convenience function to display a mnemonic securely
pub fn display_mnemonic_securely(mnemonic: &SecureString) -> Result<()> {
    let mut display = SecureMnemonicDisplay::new(mnemonic);
    display.display_securely()
}

/// Secure display for private keys (convenience function)
pub fn display_private_key_securely(private_key: &SecretKey) -> Result<()> {
    let mut display = SecurePrivateKeyDisplay::new(*private_key);
    display.display_securely()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secure_display_creation() {
        let test_mnemonic = SecureString::init_with(|| {
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about".to_string()
        });

        let display = SecureMnemonicDisplay::new(&test_mnemonic);
        // Test that the display is created successfully
        assert_eq!(
            std::mem::size_of_val(&display),
            std::mem::size_of::<SecureMnemonicDisplay>()
        );
    }

    #[test]
    fn test_mnemonic_word_parsing() {
        let test_mnemonic =
            SecureString::init_with(|| "word1 word2 word3 word4 word5 word6".to_string());

        let words: Vec<&str> = test_mnemonic.expose_secret().split_whitespace().collect();
        assert_eq!(words.len(), 6);
        assert_eq!(words[0], "word1");
        assert_eq!(words[5], "word6");
    }

    #[test]
    fn test_word_chunking() {
        let words = ["w1", "w2", "w3", "w4", "w5", "w6", "w7"];
        let chunks: Vec<_> = words.chunks(3).collect();

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0], &["w1", "w2", "w3"]);
        assert_eq!(chunks[1], &["w4", "w5", "w6"]);
        assert_eq!(chunks[2], &["w7"]);
    }
}
