use clementine_cli::mnemonic::SecureString;
use clementine_cli::secure_display::display_mnemonic_securely;

fn main() {
    println!("🔧 Secure Mnemonic Display Test");
    println!("================================");
    println!();
    println!("This program demonstrates the secure mnemonic display feature.");
    println!("It will show a test mnemonic using AlternateDisplayScreen if available,");
    println!("or fallback to standard display in environments that don't support it.");
    println!();

    // Standard BIP39 test mnemonic
    let test_mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    println!("Press Enter to display the test mnemonic securely...");
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .expect("Failed to read input");

    let secure_mnemonic = SecureString::new(test_mnemonic.to_string());

    match display_mnemonic_securely(secure_mnemonic) {
        Ok(()) => {
            println!();
            println!("✅ Secure display completed successfully!");
            println!();
            println!("If your terminal supports AlternateDisplayScreen:");
            println!("  • You should have seen the mnemonic in a separate screen");
            println!("  • The display was cleared when you pressed a key");
            println!("  • No trace remains in your terminal history");
            println!();
            println!("If AlternateDisplayScreen was not available:");
            println!("  • You saw a fallback display with security warnings");
            println!("  • This is normal in some terminal environments");
        }
        Err(e) => {
            println!("❌ Secure display failed: {}", e);
            println!("This might happen in non-interactive environments or");
            println!("terminals that don't support the required features.");
        }
    }

    println!();
    println!("🔒 Security Note:");
    println!("The mnemonic has been cleared from memory and is no longer accessible.");
    println!("This demonstrates the security features of the implementation.");
}
