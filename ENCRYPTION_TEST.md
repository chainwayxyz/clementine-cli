# 🔐 Encrypted Secret Key Integration Test

This directory contains an interactive integration test for the encrypted secret key storage functionality in Clementine CLI.

## What This Test Demonstrates

The test provides an end-to-end demonstration of:

1. **Key Generation**: Creates a new Bitcoin keypair using secure random number generation
2. **Passphrase Entry**: Interactive passphrase entry with confirmation and strength validation
3. **Encryption**: Encrypts the private key using:
   - **Argon2id** key derivation (memory-hard, side-channel resistant)
   - **AES-256-GCM** authenticated encryption
   - **Random salt and nonce** generation for each encryption
4. **Secure Storage**: Stores encrypted key to disk with restricted permissions
5. **Key Loading**: Loads and decrypts the key with the correct passphrase
6. **Verification**: Verifies that decrypted key matches the original
7. **Security Demo**: Shows that wrong passphrases fail to decrypt

## Running the Test

### Option 1: Using the Script (Recommended)
```bash
./run_encryption_test.sh
```

### Option 2: Direct Binary Execution
```bash
cargo build --bin test-encrypted-keys
./target/debug/test-encrypted-keys
```

### Option 3: Using Cargo
```bash
cargo run --bin test-encrypted-keys
```

## Security Features

### Encryption Parameters
- **Key Derivation**: Argon2id with 3 iterations, 64MB memory, 4 threads
- **Cipher**: AES-256-GCM authenticated encryption
- **Salt**: 32 bytes of cryptographically secure random data
- **Nonce**: 12 bytes of cryptographically secure random data

### Security Properties
- **Memory Zeroization**: Sensitive data is zeroed when dropped
- **File Permissions**: Key files are created with 600 permissions (owner read/write only)
- **Authentication**: GCM mode provides both confidentiality and authenticity
- **Salt Randomization**: Each encryption uses a unique salt and nonce

## File Structure

After running the test, you'll find:

```
.clementine/
└── keys/
    ├── addresses.json          # Address registry
    └── key_[address].json      # Encrypted private key
```

## Expected Test Flow

1. **Keypair Generation**: The test generates a new random Bitcoin keypair
2. **Address Display**: Shows the corresponding Taproot address and network
3. **Encryption Setup**: Prompts for a passphrase (min 8 characters, with confirmation)
4. **Storage**: Encrypts and stores the key with secure file permissions
5. **Decryption**: Prompts for passphrase and loads the encrypted key
6. **Verification**: Confirms the loaded key matches the original
7. **Security Demo**: Tests wrong passphrase handling

## Sample Output

```
╔════════════════════════════════════════════════════════════════╗
║                    🔐 Encrypted Key Storage Test                ║
║                        Interactive Demo                         ║
╚════════════════════════════════════════════════════════════════╝

🔹 Step 1: Generating Test Keypair
─────────────────────────────────────────────────────────────────
✓ Generated new keypair
Address: tb1p...
Network: testnet4
Private Key: L5J8...

🔹 Step 2: Encrypting and Storing the Key
─────────────────────────────────────────────────────────────────
Passphrase protection:
Enter a passphrase to encrypt your private key.
Enter passphrase: ********
Confirm passphrase: ********

🔐 Encrypting private key...
✓ Key stored successfully!
```

## Integration with Main CLI

The storage functions used in this test are the same ones used by the main Clementine CLI commands:

- `deposit generate-recovery-key` - Uses the same encryption when storing keys
- `withdrawal generate-signer-address` - Uses the same encryption when storing keys
- All key-loading operations use the same decryption process

## Security Notes

⚠️ **Important**: This test uses the Testnet4 network for safety. Never use mainnet private keys in testing.

🔒 **Passphrase Security**: The strength of your encryption depends on your passphrase. Use a strong, unique passphrase.

🗂️ **File Security**: The encrypted key files have restricted permissions, but the security ultimately depends on your system's access controls.

## Cleanup

To clean up test files:
```bash
rm -rf .clementine/
```

## Technical Details

### Storage Format
```json
{
  "version": 2,
  "encrypted": true,
  "network": "testnet4",
  "address": "tb1p...",
  "crypto": {
    "kdf": "argon2id",
    "salt": "hex_encoded_salt",
    "iterations": 3,
    "memory": 65536,
    "parallelism": 4,
    "cipher": "aes-256-gcm",
    "nonce": "hex_encoded_nonce",
    "ciphertext": "hex_encoded_encrypted_key"
  },
  "stored_at": "2024-01-01T00:00:00Z"
}
```

This format ensures compatibility and provides all necessary parameters for secure decryption.