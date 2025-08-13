#!/bin/bash
# Interactive Encrypted Secret Key Test
# This script runs the interactive demonstration of encrypted key storage

echo "🔐 Clementine CLI - Encrypted Key Storage Test"
echo "=============================================="
echo ""
echo "This test will demonstrate the secure key storage functionality:"
echo "• Argon2id key derivation with salt"
echo "• AES-256-GCM authenticated encryption"
echo "• Secure passphrase handling"
echo "• Key storage and retrieval"
echo "• Wrong passphrase protection"
echo ""

# Build the test binary if it doesn't exist
if [ ! -f "target/debug/test-encrypted-keys" ]; then
    echo "Building test binary..."
    cargo build --bin test-encrypted-keys
    if [ $? -ne 0 ]; then
        echo "❌ Build failed!"
        exit 1
    fi
fi

echo "Starting interactive test..."
echo ""

# Run the interactive test
./target/debug/test-encrypted-keys

echo ""
echo "🎉 Test completed!"
echo ""
echo "What happened during this test:"
echo "1. A new Bitcoin keypair was generated"
echo "2. You entered a passphrase to encrypt the private key"
echo "3. The key was encrypted using Argon2id + AES-256-GCM"
echo "4. The encrypted key was stored to .clementine/keys/"
echo "5. The key was loaded back and decrypted with your passphrase"
echo "6. We verified that the decrypted key matches the original"
echo "7. We demonstrated that wrong passphrases fail to decrypt"
echo ""
echo "Security features tested:"
echo "• Memory-hard key derivation (Argon2id)"
echo "• Authenticated encryption (AES-256-GCM)"
echo "• Random salt and nonce generation"
echo "• Secure memory handling (zeroization)"
echo "• File permission restrictions"
echo ""
echo "Your encrypted keys are stored in: .clementine/keys/"