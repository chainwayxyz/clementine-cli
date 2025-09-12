//! Secure display for sensitive cryptographic data (mnemonics and private keys).

use bip39::Mnemonic;
use colored::Colorize;
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

use crate::secure_types::{SecureSecretKey, SecureString};

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

/// Secure display manager for sensitive information like mnemonic phrases
/// Uses alternate screen to prevent shell history contamination
struct SecureMnemonicDisplay<'a> {
    /// The secure mnemonic phrase to display
    mnemonic: &'a Mnemonic,
    /// Whether alternate screen is currently active
    alternate_screen_active: bool,
}

#[derive(PartialEq)]
enum UserInput {
    Continue,
    Exit,
}

#[derive(PartialEq)]
enum MnemomicDisplayResult {
    EarlyExit,
    Completed,
}

impl<'a> SecureMnemonicDisplay<'a> {
    /// Create a new secure display instance
    fn new(mnemonic: &'a Mnemonic) -> Self {
        Self {
            mnemonic,
            alternate_screen_active: false,
        }
    }

    /// Display the mnemonic in a secure alternate screen
    fn display_securely(&mut self) -> Result<()> {
        // Show pre-display warning
        self.show_timeout_warning()?;

        // Try to enter alternate screen, fallback to normal display if it fails
        match self.enter_alternate_screen() {
            Ok(()) => {
                let result = self.display_in_alternate_screen();
                self.cleanup_alternate_screen();
                match result {
                    Ok(MnemomicDisplayResult::EarlyExit) => {
                        println!();
                        println!("{}", "Mnemonic display cancelled by user.".bold());
                        println!();
                        Ok(())
                    }
                    Ok(MnemomicDisplayResult::Completed) => Ok(()),
                    Err(e) => Err(e),
                }
            }
            Err(e) => {
                // Ensure we're in a clean state before showing fallback
                self.cleanup_alternate_screen();

                eprintln!(
                    "{} Could not enter secure display mode: {}",
                    "WARNING".bold(),
                    e
                );
                eprintln!("{} Falling back to standard display", "INFO".bold());
                self.display_fallback()
            }
        }
    }

    /// Show timeout warning before displaying the mnemonic
    fn show_timeout_warning(&self) -> Result<()> {
        println!();
        println!("{}", "IMPORTANT SECURITY NOTICE".bold());
        println!();
        println!("{}", " STEP-BY-STEP DISPLAY MODE:".bold());
        println!(
            "   - The mnemonic will be displayed {} at a time",
            "ONE WORD".bold()
        );
        println!(
            "   - Each word has a {} timeout before auto-advancing",
            "30-second".bold()
        );
        println!("   - Press Enter to advance immediately to the next word");
        println!("   - Write down each word as it appears");
        println!("   - This is a security feature to prevent prolonged exposure");
        println!();
        println!("{}", "  PREPARATION CHECKLIST:".bold());
        println!("   - Have pen and paper ready");
        println!("   - Ensure you have good lighting");
        println!("   - Find a private, secure location");
        println!("   - Remove any recording devices or cameras");
        println!("   - Be ready to write quickly and legibly");
        println!();
        println!(
            "{}",
            "Press Enter when you are ready to view the mnemonic step-by-step...".bold()
        );

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(|e| eyre!("Failed to read user input: {}", e))?;

        Ok(())
    }

    /// Enter alternate screen mode with proper error handling
    fn enter_alternate_screen(&mut self) -> Result<()> {
        // Check if we're in a real terminal first
        if !self.is_alternate_screen_supported() {
            return Err(eyre!("Terminal does not support alternate screen features"));
        }

        // Try to enable raw mode first (safer to fail here than after screen change)
        terminal::enable_raw_mode().map_err(|e| eyre!("Failed to enable raw mode: {}", e))?;

        // Only enter alternate screen if raw mode worked
        match execute!(io::stdout(), EnterAlternateScreen) {
            Ok(()) => {
                self.alternate_screen_active = true;
                Ok(())
            }
            Err(e) => {
                // Clean up raw mode if alternate screen failed
                let _ = terminal::disable_raw_mode();
                Err(eyre!("Failed to enter alternate screen: {}", e))
            }
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

    /// Display mnemonic with security warnings and user interaction
    fn display_in_alternate_screen(&self) -> Result<MnemomicDisplayResult> {
        self.clear_screen()?;
        self.display_header()?;
        match self.display_mnemonic_step_by_step()? {
            MnemomicDisplayResult::EarlyExit => return Ok(MnemomicDisplayResult::EarlyExit),
            MnemomicDisplayResult::Completed => {}
        }
        self.display_completion_message()?;
        Ok(MnemomicDisplayResult::Completed)
    }

    /// Clear the screen and position cursor at top
    fn clear_screen(&self) -> Result<()> {
        execute!(
            io::stdout(),
            terminal::Clear(terminal::ClearType::All),
            cursor::MoveTo(0, 0)
        )
        .map_err(|e| eyre!("Failed to clear screen: {}", e))?;
        Ok(())
    }

    /// Display the security header
    fn display_header(&self) -> Result<()> {
        execute!(
            io::stdout(),
            Print("╔══════════════════════════════════════════════════════════════════════════════╗\r\n"),
            Print("║                           SECURE MNEMONIC DISPLAY                            ║\r\n"),
            Print("║                                                                              ║\r\n"),
            Print("║           CRITICAL SECURITY INFORMATION - HANDLE WITH EXTREME CARE           ║\r\n"),
            Print("╚══════════════════════════════════════════════════════════════════════════════╝\r\n"),
        )
        .map_err(|e| eyre!("Failed to display header: {}", e))?;
        Ok(())
    }

    /// Display the mnemonic words step by step, one word at a time
    fn display_mnemonic_step_by_step(&self) -> Result<MnemomicDisplayResult> {
        let words: Vec<SecureString> = self
            .mnemonic
            .words()
            .map(|w| SecureString::init_with(|| w.to_string()))
            .collect();

        for (index, word) in words.iter().enumerate() {
            let word_num = index + 1;
            let total_words = words.len();

            // Clear screen and show header for each word
            self.clear_screen()?;
            self.display_header()?;

            let word = word.expose_secret();

            // Display progress and current word
            execute!(
                io::stdout(),
                Print(format!("Word {word_num} of {total_words}:\r\n\r\n")),
                Print(format!("   {word_num:2}. {word}\r\n\r\n")),
                Print("  Write down this word and press Enter to continue\r\n"),
                Print("   (or wait 30 seconds for automatic progression)\r\n\r\n"),
                Print("   Remember: Anyone with your complete mnemonic can access your funds!\r\n"),
            )
            .map_err(|e| eyre!("Failed to display word {}: {}", word_num, e))?;

            let resp = self
                .wait_for_word_confirmation()
                .map_err(|e| eyre!("Error during word display: {}", e))?;

            if UserInput::Exit == resp {
                return Ok(MnemomicDisplayResult::EarlyExit);
            }
        }

        Ok(MnemomicDisplayResult::Completed)
    }

    /// Wait for user confirmation for each word with timeout
    fn wait_for_word_confirmation(&self) -> Result<UserInput> {
        let start_time = Instant::now();

        loop {
            let elapsed = start_time.elapsed();
            let remaining = WORD_TIMEOUT_DURATION.saturating_sub(elapsed);

            if remaining.is_zero() {
                // Timeout reached, automatically proceed to next word
                return Ok(UserInput::Continue);
            }

            // Update countdown display for current word
            self.update_word_countdown_display(remaining)?;

            // Check for user input with a short timeout
            if poll(POLL_INTERVAL).map_err(|e| eyre!("Failed to poll for input: {}", e))?
                && let Event::Key(key_event) =
                    event::read().map_err(|e| eyre!("Failed to read user input: {}", e))?
                && key_event.kind == KeyEventKind::Press
            {
                match key_event.code {
                    KeyCode::Enter => {
                        // User pressed Enter, proceed to next word
                        return Ok(UserInput::Continue);
                    }
                    KeyCode::Esc => {
                        return Ok(UserInput::Exit);
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
            Print(format!("⏰ Auto-advance in {seconds_left} seconds ")),
            Print("| Press Enter to continue immediately | Press ESC to cancel"),
        )
        .map_err(|e| eyre!("Failed to update word countdown: {}", e))?;

        Ok(())
    }

    /// Display completion message after all words have been shown
    fn display_completion_message(&self) -> Result<()> {
        self.clear_screen()?;

        execute!(
            io::stdout(),
            Print("╔══════════════════════════════════════════════════════════════════════════════╗\r\n"),
            Print("║                          MNEMONIC DISPLAY COMPLETED                          ║\r\n"),
            Print("║                                                                              ║\r\n"),
            Print("║  All words have been displayed. Please verify you have written them down.    ║\r\n"),
            Print("║                                                                              ║\r\n"),
            Print("║    IMPORTANT REMINDERS:                                                      ║\r\n"),
            Print("║  - Store your written mnemonic in a secure location                          ║\r\n"),
            Print("║  - Never share it with anyone                                                ║\r\n"),
            Print("║                                                                              ║\r\n"),
            Print("╚══════════════════════════════════════════════════════════════════════════════╝\r\n"),
            Print("\r\nPress any key to exit..."),
        )
        .map_err(|e| eyre!("Failed to display completion message: {}", e))?;

        // Wait for final confirmation
        loop {
            if poll(POLL_INTERVAL).map_err(|e| eyre!("Failed to poll for input: {}", e))?
                && let Event::Key(key_event) =
                    event::read().map_err(|e| eyre!("Failed to read user input: {}", e))?
                && key_event.kind == KeyEventKind::Press
            {
                return Ok(());
            }
        }
    }

    /// Cleanup alternate screen and return to normal mode
    fn cleanup_alternate_screen(&mut self) {
        if self.alternate_screen_active {
            let _ = terminal::disable_raw_mode();
            let _ = execute!(io::stdout(), LeaveAlternateScreen);
            // Reset terminal colors when leaving alternate screen
            let _ = execute!(io::stdout(), SetForegroundColor(Color::Reset));
            self.alternate_screen_active = false;
        }

        // Only try additional cleanup if we think we might be in raw mode
        // but avoid sending escape sequences that could appear in output
        let _ = terminal::disable_raw_mode();

        // Flush stdout to ensure all output is written
        let _ = io::stdout().flush();
    }

    /// Fallback display method when alternate screen is not available
    fn display_fallback(&self) -> Result<()> {
        println!();
        println!("{}", "  MNEMONIC PHRASE (STEP-BY-STEP DISPLAY)  ".bold());
        println!();
        println!("Each word will be displayed for 30 seconds or until you press Enter");
        println!();

        let words: Vec<SecureString> = self
            .mnemonic
            .words()
            .map(|w| SecureString::init_with(|| w.to_string()))
            .collect();

        for (index, word) in words.iter().enumerate() {
            let word_num = index + 1;
            let total_words = words.len();
            let word = word.expose_secret();

            println!();
            println!(
                "{}",
                format!("━━━ Word {word_num} of {total_words} ━━━").bold()
            );
            println!();
            println!("{}", format!("   {word_num:2}. {word}").bold());
            println!();
            println!("  Write down this word and press Enter to continue");
            println!("   (or wait 30 seconds for automatic progression)");
            println!();
            println!("   Remember: Anyone with your complete mnemonic can access your funds!");

            // Wait for user input or timeout for each word
            if let Err(e) = self.fallback_word_timeout_wait() {
                return Err(eyre!("Error during word display: {}", e));
            }
        }

        println!();
        println!("{}", "  All words have been displayed!".bold());
        println!("Store your written mnemonic in a secure location.");
        println!("The mnemonic will be cleared from memory after this display.");
        println!();
        println!("Press Enter to exit...");

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(|e| eyre!("Failed to read user input: {}", e))?;

        Ok(())
    }

    /// Handle timeout for each word in fallback mode
    fn fallback_word_timeout_wait(&self) -> Result<()> {
        let start_time = Instant::now();

        println!();
        print!("Press Enter to continue... ");
        io::stdout()
            .flush()
            .map_err(|e| eyre!("Failed to flush stdout: {}", e))?;

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
            std::thread::sleep(WORD_TIMEOUT_DURATION);
            let _ = tx.send(false);
        });

        // Update countdown while waiting
        loop {
            let elapsed = start_time.elapsed();
            let remaining = WORD_TIMEOUT_DURATION.saturating_sub(elapsed);

            if remaining.is_zero() {
                println!();
                println!("⏰ Auto-advancing to next word...");
                break;
            }

            // Check if we received a signal
            if let Ok(user_input) = rx.try_recv() {
                if user_input {
                    println!("Continuing to next word...");
                } else {
                    println!();
                    println!("⏰ Auto-advancing to next word...");
                }
                break;
            }

            // Update countdown every second
            let seconds_left = remaining.as_secs();
            print!("\r⏰ Auto-advance in {seconds_left} seconds - Press Enter to continue... ");
            io::stdout()
                .flush()
                .map_err(|e| eyre!("Failed to flush stdout: {}", e))?;

            std::thread::sleep(Duration::from_millis(1000));
        }

        Ok(())
    }
}

impl<'a> Drop for SecureMnemonicDisplay<'a> {
    fn drop(&mut self) {
        self.cleanup_alternate_screen();
        // SecureString handles its own zeroization
    }
}

/// Convenience function to display a mnemonic securely
pub(crate) fn display_mnemonic_securely(mnemonic: &Mnemonic) -> Result<()> {
    let mut display = SecureMnemonicDisplay::new(mnemonic);
    display.display_securely()
}

/// Simple secure display for private keys
pub(crate) fn display_private_key_securely(private_key: &SecureSecretKey) -> Result<()> {
    // Check if terminal supports alternate screen
    if !std::io::stdout().is_terminal() {
        return display_private_key_fallback(private_key);
    }

    // Try to enter alternate screen
    if terminal::enable_raw_mode().is_err() {
        return display_private_key_fallback(private_key);
    }

    if execute!(io::stdout(), EnterAlternateScreen).is_err() {
        let _ = terminal::disable_raw_mode();
        return display_private_key_fallback(private_key);
    }

    // Display private key in alternate screen
    let result = display_private_key_in_alternate_screen(private_key);

    // Cleanup
    let _ = terminal::disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen);
    let _ = execute!(io::stdout(), SetForegroundColor(Color::Reset));
    let _ = io::stdout().flush();

    result
}

/// Display private key in alternate screen
fn display_private_key_in_alternate_screen(private_key: &SecureSecretKey) -> Result<()> {
    // Clear screen
    execute!(
        io::stdout(),
        terminal::Clear(terminal::ClearType::All),
        cursor::MoveTo(0, 0)
    )?;

    // Display header
    execute!(
        io::stdout(),
        Print(
            "╔══════════════════════════════════════════════════════════════════════════════╗\r\n"
        ),
        Print(
            "║                          SECURE PRIVATE KEY DISPLAY                          ║\r\n"
        ),
        Print(
            "║                                                                              ║\r\n"
        ),
        Print(
            "║           CRITICAL SECURITY INFORMATION - HANDLE WITH EXTREME CARE           ║\r\n"
        ),
        Print(
            "╚══════════════════════════════════════════════════════════════════════════════╝\r\n\r\n"
        ),
        Print("Private Key:\r\n\r\n"),
        Print(format!(
            "   {}\r\n\r\n",
            private_key.as_ref_inner().display_secret()
        )),
        Print("   WARNING: Anyone with this private key can access your funds!\r\n"),
        Print("   Never share this key or store it in insecure locations!\r\n\r\n"),
        Print("Press any key to clear and exit (auto-close in 30 seconds)..."),
    )?;

    // Wait for key press with timeout
    let start_time = Instant::now();

    loop {
        let elapsed = start_time.elapsed();
        let remaining = PRIVATE_KEY_TIMEOUT_DURATION.saturating_sub(elapsed);

        if remaining.is_zero() {
            // Timeout reached, automatically close
            break;
        }

        // Update countdown display
        let seconds_left = remaining.as_secs();
        execute!(
            io::stdout(),
            cursor::MoveTo(0, PRIVATE_KEY_COUNTDOWN_Y),
            Print(format!(
                "⏰ Auto-close in {} seconds | Press any key to close immediately   ",
                seconds_left
            )),
        )?;

        if poll(POLL_INTERVAL)? {
            let event = event::read()?;
            if let Event::Key(key_event) = event
                && key_event.kind == KeyEventKind::Press
            {
                break;
            }
        }
    }

    Ok(())
}

/// Fallback display for private key when alternate screen is not available
fn display_private_key_fallback(private_key: &SecureSecretKey) -> Result<()> {
    println!();
    println!("{}", "  PRIVATE KEY DISPLAY  ".bold());
    println!();
    println!("{}", "   CRITICAL SECURITY WARNING  ".bold());
    println!("Anyone with this private key can access your funds!");
    println!("Never share this key or store it in insecure locations!");
    println!();
    println!("{}", "Private Key:".bold());
    println!("   {}", private_key.as_ref_inner().display_secret());
    println!();
    println!("Press Enter to clear and continue (auto-close in 30 seconds)...");

    // Use the same timeout pattern as the fallback_word_timeout_wait function
    let start_time = Instant::now();

    print!("Press Enter to continue... ");
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
        std::thread::sleep(PRIVATE_KEY_TIMEOUT_DURATION);
        let _ = tx.send(false);
    });

    // Update countdown while waiting
    loop {
        let elapsed = start_time.elapsed();
        let remaining = PRIVATE_KEY_TIMEOUT_DURATION.saturating_sub(elapsed);

        if remaining.is_zero() {
            println!();
            println!(" Auto-closing...");
            break;
        }

        // Check if we received a signal
        if let Ok(user_input) = rx.try_recv() {
            if user_input {
                println!("Closing...");
            } else {
                println!();
                println!(" Auto-closing...");
            }
            break;
        }

        // Update countdown every second
        let seconds_left = remaining.as_secs();
        print!(
            "\r Auto-close in {} seconds - Press Enter to close... ",
            seconds_left
        );
        io::stdout().flush()?;

        std::thread::sleep(Duration::from_millis(1000));
    }

    Ok(())
}
