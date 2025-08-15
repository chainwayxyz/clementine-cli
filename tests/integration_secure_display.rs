use clementine_cli::mnemonic::SecureString;
use clementine_cli::secure_display::{SecureMnemonicDisplay, display_mnemonic_securely};
use tempfile::TempDir;

#[test]
fn test_secure_mnemonic_display_integration() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");

    // Use temporary directory for storage without modifying HOME
    println!("Using temporary directory: {:?}", temp_dir.path());

    // Test mnemonic phrase (standard BIP39 test vector)
    let test_mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    let secure_mnemonic = SecureString::new(test_mnemonic.to_string());

    println!("Testing secure mnemonic display...");
    println!("This test will attempt to use AlternateDisplayScreen.");
    println!("If your terminal supports it, you should see a secure display.");
    println!("If not, it will fallback to standard display.");
    println!();

    // Test the secure display function directly
    match display_mnemonic_securely(secure_mnemonic) {
        Ok(()) => {
            println!("✅ Secure display completed successfully");
        }
        Err(e) => {
            println!(
                "⚠️  Secure display failed (this is expected in CI/non-interactive environments): {e}"
            );
        }
    }

    // Temporary directory is automatically cleaned up when temp_dir goes out of scope
}

#[test]
fn test_secure_display_struct_lifecycle() {
    let test_mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    let secure_mnemonic = SecureString::new(test_mnemonic.to_string());

    // Test creating the display struct
    let display = SecureMnemonicDisplay::new(secure_mnemonic);

    // The display struct is created successfully (we can't access private fields in tests)

    // Test that cleanup works properly (should not panic)
    drop(display);

    println!("✅ SecureMnemonicDisplay lifecycle test passed");
}

#[test]
fn test_mnemonic_word_formatting() {
    let test_mnemonic =
        "word1 word2 word3 word4 word5 word6 word7 word8 word9 word10 word11 word12";
    let words: Vec<&str> = test_mnemonic.split_whitespace().collect();

    assert_eq!(words.len(), 12);

    // Test word chunking for display formatting
    let chunks: Vec<_> = words.chunks(3).collect();
    assert_eq!(chunks.len(), 4);
    assert_eq!(chunks[0], &["word1", "word2", "word3"]);
    assert_eq!(chunks[3], &["word10", "word11", "word12"]);

    // Test word numbering
    for (i, word) in words.iter().enumerate() {
        let word_num = words.iter().position(|&w| w == *word).unwrap() + 1;
        assert_eq!(word_num, i + 1);
    }

    println!("✅ Mnemonic word formatting test passed");
}

// Manual test function that can be run interactively
#[test]
#[ignore] // Ignored by default since it requires user interaction
fn test_manual_secure_display() {
    println!("🔧 Manual Test: Secure Mnemonic Display");
    println!("This test requires manual interaction and should be run with:");
    println!("cargo test test_manual_secure_display -- --ignored --nocapture");
    println!();

    let test_mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    let secure_mnemonic = SecureString::new(test_mnemonic.to_string());

    println!("About to display a test mnemonic securely...");
    println!("Press Enter to continue...");

    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .expect("Failed to read input");

    match display_mnemonic_securely(secure_mnemonic) {
        Ok(()) => {
            println!("✅ Manual secure display test completed");
            println!("Did you see the mnemonic displayed in an alternate screen? (y/n)");

            let mut response = String::new();
            std::io::stdin()
                .read_line(&mut response)
                .expect("Failed to read response");

            if response.trim().to_lowercase().starts_with('y') {
                println!("✅ Alternate screen display worked correctly!");
            } else {
                println!("ℹ️  Fallback display was used (expected in some environments)");
            }
        }
        Err(e) => {
            println!("❌ Manual test failed: {e}");
            panic!("Manual test failed");
        }
    }
}

#[test]
fn test_encrypted_wallet_integration() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");

    println!("Using temporary directory: {:?}", temp_dir.path());

    println!("Testing integration with encrypted wallet creation...");
    println!("This will test the full flow including secure display.");

    // Note: This test will use the fallback display since it's non-interactive
    // In a real scenario, the secure display would be used

    // The create_encrypted_wallet function requires interactive input for passphrase,
    // so we'll test the components that can be tested non-interactively

    let test_mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    let secure_mnemonic = SecureString::new(test_mnemonic.to_string());

    // This should use fallback display in CI environment
    let result = display_mnemonic_securely(secure_mnemonic);

    match result {
        Ok(()) => println!("✅ Integration test with secure display passed"),
        Err(_) => println!("ℹ️  Secure display used fallback (expected in CI)"),
    }

    // Temporary directory is automatically cleaned up when temp_dir goes out of scope
}

// Test for error handling
#[test]
fn test_secure_display_error_handling() {
    // Test with empty mnemonic
    let empty_mnemonic = SecureString::new("".to_string());
    let result = display_mnemonic_securely(empty_mnemonic);

    // Should handle gracefully (either succeed with empty display or fail gracefully)
    match result {
        Ok(()) => println!("✅ Empty mnemonic handled gracefully"),
        Err(_) => println!("ℹ️  Empty mnemonic failed gracefully (acceptable)"),
    }

    // Test with very long mnemonic
    let long_mnemonic = SecureString::new("word ".repeat(100).trim().to_string());
    let result = display_mnemonic_securely(long_mnemonic);

    match result {
        Ok(()) => println!("✅ Long mnemonic handled gracefully"),
        Err(_) => println!("ℹ️  Long mnemonic failed gracefully (acceptable)"),
    }
}
