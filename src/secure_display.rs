use anyhow::{anyhow, Result};
use colored::*;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind, poll},
    execute,
    style::{Color, Print, SetForegroundColor},
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::time::{Duration, Instant};
use std::io::{self, Write, IsTerminal};

use crate::mnemonic::SecureString;

/// Secure display manager for sensitive information like mnemonic phrases
/// Uses alternate screen to prevent shell history contamination
pub struct SecureMnemonicDisplay {
    /// The secure mnemonic phrase to display
    mnemonic: SecureString,
    /// Whether alternate screen is currently active
    alternate_screen_active: bool,
}

impl SecureMnemonicDisplay {
    /// Create a new secure display instance
    pub fn new(mnemonic: SecureString) -> Self {
        Self {
            mnemonic,
            alternate_screen_active: false,
        }
    }

    /// Display the mnemonic in a secure alternate screen
    pub fn display_securely(&mut self) -> Result<()> {
        // Show pre-display warning
        self.show_timeout_warning()?;
        
        // Try to enter alternate screen, fallback to normal display if it fails
        match self.enter_alternate_screen() {
            Ok(()) => {
                let result = self.display_in_alternate_screen();
                self.cleanup_alternate_screen();
                result
            }
            Err(e) => {
                // Ensure we're in a clean state before showing fallback
                self.cleanup_alternate_screen();
                
                eprintln!(
                    "{} Could not enter secure display mode: {}",
                    "WARNING".yellow().bold(),
                    e
                );
                eprintln!(
                    "{} Falling back to standard display",
                    "INFO".blue().bold()
                );
                self.display_fallback()
            }
        }
    }

    /// Show timeout warning before displaying the mnemonic
    fn show_timeout_warning(&self) -> Result<()> {
        println!();
        println!("{}", "⚠️  IMPORTANT SECURITY NOTICE ⚠️".red().bold());
        println!();
        println!("{}", "🕐 TIMED DISPLAY WARNING:".yellow().bold());
        println!("   • The mnemonic will be displayed for {} ONLY", "60 seconds".red().bold());
        println!("   • After the timeout, the display will automatically close");
        println!("   • You must write down ALL words before the timer expires");
        println!("   • This is a security feature to prevent prolonged exposure");
        println!();
        println!("{}", "📝 PREPARATION CHECKLIST:".cyan().bold());
        println!("   ✓ Have pen and paper ready");
        println!("   ✓ Ensure you have good lighting");
        println!("   ✓ Find a private, secure location");
        println!("   ✓ Remove any recording devices or cameras");
        println!();
        println!("{}", "Press Enter when you are ready to view the mnemonic...".green().bold());
        
        let mut input = String::new();
        io::stdin().read_line(&mut input)
            .map_err(|e| anyhow!("Failed to read user input: {}", e))?;
            
        Ok(())
    }

    /// Enter alternate screen mode with proper error handling
    fn enter_alternate_screen(&mut self) -> Result<()> {
        // Check if we're in a real terminal first
        if !self.is_terminal_compatible() {
            return Err(anyhow!("Terminal does not support alternate screen features"));
        }
        
        // Try to enable raw mode first (safer to fail here than after screen change)
        terminal::enable_raw_mode()
            .map_err(|e| anyhow!("Failed to enable raw mode: {}", e))?;
        
        // Only enter alternate screen if raw mode worked
        match execute!(io::stdout(), EnterAlternateScreen) {
            Ok(()) => {
                self.alternate_screen_active = true;
                Ok(())
            }
            Err(e) => {
                // Clean up raw mode if alternate screen failed
                let _ = terminal::disable_raw_mode();
                Err(anyhow!("Failed to enter alternate screen: {}", e))
            }
        }
    }
    
    /// Check if terminal supports the features we need
    fn is_terminal_compatible(&self) -> bool {
        // Must be a real terminal (not redirected)
        if !std::io::stdout().is_terminal() {
            return false;
        }
        
        // Check if we can detect terminal size (good indicator of terminal features)
        if let Ok((_, _)) = terminal::size() {
            // Additional check: see if we're not in a non-interactive environment
            std::env::var("TERM").map_or(false, |term| {
                !term.is_empty() && term != "dumb"
            })
        } else {
            false
        }
    }

    /// Display mnemonic with security warnings and user interaction
    fn display_in_alternate_screen(&self) -> Result<()> {
        self.clear_screen()?;
        self.display_header()?;
        self.display_mnemonic_words()?;
        self.display_security_warnings()?;
        self.display_instructions()?;
        self.wait_for_user_confirmation()?;
        Ok(())
    }

    /// Clear the screen and position cursor at top
    fn clear_screen(&self) -> Result<()> {
        execute!(
            io::stdout(),
            terminal::Clear(terminal::ClearType::All),
            cursor::MoveTo(0, 0)
        )
        .map_err(|e| anyhow!("Failed to clear screen: {}", e))?;
        Ok(())
    }

    /// Display the security header
    fn display_header(&self) -> Result<()> {
        execute!(
            io::stdout(),
            SetForegroundColor(Color::Red),
            Print("╔══════════════════════════════════════════════════════════════════════════════╗\n"),
            Print("║                          🔐 SECURE MNEMONIC DISPLAY 🔐                       ║\n"),
            Print("║                                                                              ║\n"),
            Print("║  ⚠️  CRITICAL SECURITY INFORMATION - HANDLE WITH EXTREME CARE  ⚠️           ║\n"),
            Print("╚══════════════════════════════════════════════════════════════════════════════╝\n\n"),
            SetForegroundColor(Color::Reset)
        )
        .map_err(|e| anyhow!("Failed to display header: {}", e))?;
        Ok(())
    }

    /// Display the mnemonic words in a formatted grid
    fn display_mnemonic_words(&self) -> Result<()> {
        execute!(
            io::stdout(),
            SetForegroundColor(Color::Yellow),
            Print("Your Recovery Phrase (write down in order):\n\n")
        )
        .map_err(|e| anyhow!("Failed to display mnemonic header: {}", e))?;

        let words: Vec<&str> = self.mnemonic.as_str().split_whitespace().collect();
        
        // Display words in a 3-column format for better readability
        for (chunk_idx, chunk) in words.chunks(3).enumerate() {
            let mut line = String::new();
            for (word_idx, word) in chunk.iter().enumerate() {
                let word_num = chunk_idx * 3 + word_idx + 1;
                line.push_str(&format!("{:2}. {:12} ", word_num, word));
            }
            execute!(
                io::stdout(),
                SetForegroundColor(Color::Cyan),
                Print(format!("   {}\n", line))
            )
            .map_err(|e| anyhow!("Failed to display mnemonic words: {}", e))?;
        }

        execute!(
            io::stdout(),
            SetForegroundColor(Color::Reset),
            Print("\n")
        )
        .map_err(|e| anyhow!("Failed to reset color: {}", e))?;
        Ok(())
    }

    /// Display security warnings and best practices
    fn display_security_warnings(&self) -> Result<()> {
        execute!(
            io::stdout(),
            SetForegroundColor(Color::Red),
            Print("🚨 SECURITY WARNINGS:\n"),
            SetForegroundColor(Color::Yellow),
            Print("   • Write down these words on PAPER - never store digitally\n"),
            Print("   • Store in a SECURE LOCATION (fireproof safe, safety deposit box)\n"),
            Print("   • NEVER share this phrase with anyone\n"),
            Print("   • This is the ONLY way to recover your wallet\n"),
            Print("   • Anyone with this phrase can access your funds\n\n"),
            SetForegroundColor(Color::Green),
            Print("✅ BEST PRACTICES:\n"),
            Print("   • Double-check each word for spelling\n"),
            Print("   • Consider making multiple secure copies\n"),
            Print("   • Verify you can read your handwriting\n"),
            Print("   • Test recovery with a small amount first\n\n"),
            SetForegroundColor(Color::Reset)
        )
        .map_err(|e| anyhow!("Failed to display security warnings: {}", e))?;
        Ok(())
    }

    /// Display user instructions
    fn display_instructions(&self) -> Result<()> {
        execute!(
            io::stdout(),
            SetForegroundColor(Color::Blue),
            Print("📝 INSTRUCTIONS:\n"),
            Print("   1. Write down all words in the exact order shown above\n"),
            Print("   2. Verify you have written them correctly\n"),
            Print("   3. Store your written copy in a secure location\n"),
            Print("   4. Press any key to continue once you have safely stored the phrase\n\n"),
            SetForegroundColor(Color::Magenta),
            Print("⚡ This display will be cleared from memory when you continue.\n"),
            Print("   You will NOT be able to see this phrase again!\n\n"),
            SetForegroundColor(Color::White),
            Print("Press any key to continue..."),
            SetForegroundColor(Color::Reset)
        )
        .map_err(|e| anyhow!("Failed to display instructions: {}", e))?;
        Ok(())
    }

    /// Wait for user confirmation with timeout
    fn wait_for_user_confirmation(&self) -> Result<()> {
        const TIMEOUT_DURATION: Duration = Duration::from_secs(60);
        let start_time = Instant::now();
        
        loop {
            let elapsed = start_time.elapsed();
            let remaining = TIMEOUT_DURATION.saturating_sub(elapsed);
            
            if remaining.is_zero() {
                self.display_timeout_message()?;
                return Ok(()); // Timeout reached, exit gracefully
            }
            
            // Update countdown display
            self.update_countdown_display(remaining)?;
            
            // Check for user input with a short timeout
            if poll(Duration::from_millis(100))
                .map_err(|e| anyhow!("Failed to poll for input: {}", e))?
            {
                if let Event::Key(key_event) = event::read()
                    .map_err(|e| anyhow!("Failed to read user input: {}", e))?
                {
                    if key_event.kind == KeyEventKind::Press {
                        match key_event.code {
                            KeyCode::Esc => {
                                return Err(anyhow!("User cancelled mnemonic display"));
                            }
                            _ => {
                                // Any other key continues (exits the display)
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }
    }
    
    /// Display timeout message
    fn display_timeout_message(&self) -> Result<()> {
        self.clear_screen()?;
        execute!(
            io::stdout(),
            SetForegroundColor(Color::Red),
            Print("╔══════════════════════════════════════════════════════════════════════════════╗\n"),
            Print("║                               ⏰ TIMEOUT REACHED ⏰                           ║\n"),
            Print("║                                                                              ║\n"),
            Print("║  The secure display has automatically closed after 60 seconds.             ║\n"),
            Print("║  This is a security feature to prevent prolonged exposure.                  ║\n"),
            Print("║                                                                              ║\n"),
            Print("║  If you need to view the mnemonic again, please restart the process.        ║\n"),
            Print("╚══════════════════════════════════════════════════════════════════════════════╝\n"),
            SetForegroundColor(Color::Reset)
        )
        .map_err(|e| anyhow!("Failed to display timeout message: {}", e))?;
        
        // Wait a moment for user to read the message
        std::thread::sleep(Duration::from_secs(3));
        Ok(())
    }
    
    /// Update countdown display
    fn update_countdown_display(&self, remaining: Duration) -> Result<()> {
        let seconds_left = remaining.as_secs();
        let minutes = seconds_left / 60;
        let seconds = seconds_left % 60;
        
        // Position cursor at bottom of screen for countdown
        execute!(
            io::stdout(),
            cursor::MoveTo(0, 25), // Move to bottom area
            SetForegroundColor(Color::Red),
            Print(format!("⏰ Time remaining: {:02}:{:02} ", minutes, seconds)),
            SetForegroundColor(Color::Yellow),
            Print("Press any key to continue or ESC to cancel"),
            SetForegroundColor(Color::Reset)
        )
        .map_err(|e| anyhow!("Failed to update countdown: {}", e))?;
        
        Ok(())
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
        println!("{}", "🔐 MNEMONIC PHRASE (SECURE DISPLAY UNAVAILABLE) 🔐".red().bold());
        println!();
        println!("{}", "⚠️  TIMEOUT WARNING: You have 60 seconds to write this down!".red().bold());
        println!();
        println!("{}", "Please write down your mnemonic phrase and store it in a safe place:".yellow());
        println!();
        
        let words: Vec<&str> = self.mnemonic.as_str().split_whitespace().collect();
        for (i, word) in words.iter().enumerate() {
            print!("{:2}. {:12} ", i + 1, word);
            if (i + 1) % 3 == 0 {
                println!();
            }
        }
        if words.len() % 3 != 0 {
            println!();
        }
        
        println!();
        println!("{}", "This is the ONLY way to recover your wallet if you lose your passphrase!".red().bold());
        println!("{}", "The mnemonic will be cleared from memory after this display.".yellow());
        
        // Timeout for fallback mode as well
        self.fallback_timeout_wait()?;
        
        Ok(())
    }
    
    /// Handle timeout in fallback mode
    fn fallback_timeout_wait(&self) -> Result<()> {
        const TIMEOUT_DURATION: Duration = Duration::from_secs(60);
        let start_time = Instant::now();
        
        println!();
        print!("Press Enter to continue or wait for automatic timeout... ");
        io::stdout().flush().map_err(|e| anyhow!("Failed to flush stdout: {}", e))?;
        
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
            std::thread::sleep(TIMEOUT_DURATION);
            let _ = tx.send(false);
        });
        
        // Update countdown while waiting
        loop {
            let elapsed = start_time.elapsed();
            let remaining = TIMEOUT_DURATION.saturating_sub(elapsed);
            
            if remaining.is_zero() {
                println!();
                println!("{}", "⏰ Timeout reached - display automatically closed".red().bold());
                break;
            }
            
            // Check if we received a signal
            if let Ok(user_input) = rx.try_recv() {
                if user_input {
                    println!("{}", "User confirmed - continuing...".green());
                } else {
                    println!();
                    println!("{}", "⏰ Timeout reached - display automatically closed".red().bold());
                }
                break;
            }
            
            // Update countdown every second
            let seconds_left = remaining.as_secs();
            let minutes = seconds_left / 60;
            let seconds = seconds_left % 60;
            print!("\r⏰ Time remaining: {:02}:{:02} - Press Enter to continue... ", minutes, seconds);
            io::stdout().flush().map_err(|e| anyhow!("Failed to flush stdout: {}", e))?;
            
            std::thread::sleep(Duration::from_millis(1000));
        }
        
        Ok(())
    }
}

impl Drop for SecureMnemonicDisplay {
    fn drop(&mut self) {
        self.cleanup_alternate_screen();
        // SecureString handles its own zeroization
    }
}

/// Convenience function to display a mnemonic securely
pub fn display_mnemonic_securely(mnemonic: SecureString) -> Result<()> {
    let mut display = SecureMnemonicDisplay::new(mnemonic);
    display.display_securely()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secure_display_creation() {
        let test_mnemonic = SecureString::new(
            "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about".to_string()
        );
        
        let display = SecureMnemonicDisplay::new(test_mnemonic);
        assert!(!display.alternate_screen_active);
    }

    #[test]
    fn test_mnemonic_word_parsing() {
        let test_mnemonic = SecureString::new(
            "word1 word2 word3 word4 word5 word6".to_string()
        );
        
        let words: Vec<&str> = test_mnemonic.as_str().split_whitespace().collect();
        assert_eq!(words.len(), 6);
        assert_eq!(words[0], "word1");
        assert_eq!(words[5], "word6");
    }

    #[test]
    fn test_word_chunking() {
        let words = vec!["w1", "w2", "w3", "w4", "w5", "w6", "w7"];
        let chunks: Vec<_> = words.chunks(3).collect();
        
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0], &["w1", "w2", "w3"]);
        assert_eq!(chunks[1], &["w4", "w5", "w6"]);
        assert_eq!(chunks[2], &["w7"]);
    }
}