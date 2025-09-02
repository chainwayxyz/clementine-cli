# Wallet Operations

Clementine CLI provides comprehensive wallet management capabilities with strong security features. All wallet operations can be performed offline for maximum security.

## About Clementine Wallets

Clementine wallets are specialized Bitcoin key managers designed for secure bridge operations with Citrea. Unlike Bitcoin Core wallets that manage multiple addresses, each Clementine wallet corresponds to a single Bitcoin address with its associated private key. Clementine wallets generate addresses with specific prefixes that indicate their intended bridge use case and prevent accidental misuse of funds. 

### Address Prefixes and Purpose Field

**Safety Feature:** Clementine uses address prefixes to create specialized bridge addresses that prevent fund loss through accidental misuse:

- **"dep" prefix**: Used for deposit operations (e.g., `depbc1p...`)
- **"wit" prefix**: Used for withdrawal operations (e.g., `witbc1p...`)
- **Standard addresses**: Used for regular wallet operations (e.g., `bc1p...`)

**Purpose Field:** When creating a wallet, the `<PURPOSE>` parameter determines the address prefix and cryptographic properties:
- `deposit` → generates addresses with "dep" prefix for deposit operations
- `withdrawal` → generates addresses with "wit" prefix for withdrawal operations

**These are not ordinary Bitcoin addresses** - they are specialized bridge addresses with different spending conditions specifically generated for Clementine bridge operations.

**Why prefixes matter:**
- **Prevent accidental copy-paste errors** that could result in permanent fund loss
- **Ensure proper address derivation** with bridge-specific cryptographic schemes
- **Distinguish between different signature algorithms** required by deposit vs withdrawal operations
- **Provide validation layer** to catch user errors before interacting with Clementine protocol
- **Enable operation-specific security** tailored to each bridge function

**Critical Warning:** Never manually remove or modify address prefixes, as this can lead to:
- Failed Clementine interactions
- Permanent loss of funds due to misinterpretation or confusion
- Inability to recover funds from bridge operations

View all wallet commands: `clementine-cli --network <NETWORK> wallet --help`

## Create Wallet

**AIRGAPPED DEVICE ONLY:** Create a new wallet with secure mnemonic generation. This operation MUST be performed on the airgapped device.

```sh
clementine-cli --network <NETWORK> wallet create <WALLET-LABEL> <PURPOSE>
```

**Example:**
```sh
clementine-cli --network testnet4 wallet create my-wallet deposit
```

## Backup and Importing a Wallet

### Export/Backup a Wallet

A wallet can be backed up using:

```sh
clementine-cli wallet backup <BACKUP-DIRECTORY> <WALLET-ADDRESS>

# Example of backing up a wallet with address `tb1pd...` with `deposit` purpose to the current directory
clementine-cli wallet backup . deptb1pd...
```

After that, a `wallet_<address>.json` file will be available as a backup.

### Import Wallet Using File

Wallet files that are generated elsewhere or previously exported can be imported using `import-file` command:

```sh
clementine-cli wallet import-file <FILE-NAME> <WALLET-LABEL>

# Example of importing a wallet named `wallet1` in the current directory
clementine-cli wallet import-file wallet_wallet1.json wallet1
```

### Import Private Key

If you already have a recovery taproot address, you can import it as a wallet
using the import utility with your secret key. It will be marked as imported via
private key when you list wallets using `clementine-cli wallet list`.

**Note**: This is especially useful if you generated your recovery taproot address using the frontend; however, this is not recommended. We suggest using those keys only for testing purposes.

```sh
clementine-cli wallet import-private-key <WALLET-LABEL>
```

### Import Using Mnemonic

You can also import a wallet using the 12 word mnemonic:

```sh
clementine-cli --network <NETWORK> wallet import-mnemonic <WALLET-LABEL>
```
This command will prompt you to enter all the mnemonic words step by step.

## Wallet Management

### List All Wallets

View all wallets with their addresses, networks and import status:

```sh
clementine-cli wallet list
```

### Show Mnemonic

Securely display the mnemonic for an existing wallet (use with extreme caution):

```sh
clementine-cli --network <NETWORK> wallet show-mnemonic <WALLET_NAME>
```

### Show Private Key

Display the private key for a wallet (use with extreme caution):

```sh
clementine-cli --network <NETWORK> wallet show-private-key <WALLET_NAME>
```

### Verify Wallet Integrity

Check the integrity of wallet registry and files:

```sh
clementine-cli --network <NETWORK> wallet verify-integrity
```

## Security Best Practices

### Critical Security Requirements

- **Airgapped Environment**: Perform all wallet creation and key operations offline
- **Secure Storage**: Store mnemonic phrases and private keys in encrypted, offline storage
- **Multiple Backups**: Keep wallet backups in multiple secure, geographically distributed locations
- **Access Control**: Limit access to wallet files and ensure proper file system permissions

### Two-Device Workflow

**Airgapped Device Operations:**
1. **Create wallet**: `clementine-cli --network <NETWORK> wallet create <NAME>`
2. **Backup wallet**: `clementine-cli --network <NETWORK> wallet backup ./backup <NAME>`
3. **Generate signatures**: All signing operations stay on airgapped device
4. **Show addresses**: Copy addresses to online device for verification

**Online Device Operations:**
1. **Verify addresses**: Confirm addresses match airgapped device output
2. **Monitor operations**: Use addresses for status checking
3. **No sensitive operations**: Never import wallets or keys on online device

**Secure Data Transfer Protocol:**
- Use USB drives formatted with secure filesystems
- Employ QR codes for short data transfers
- Always verify data integrity after transfer
- Never transfer private keys or mnemonics to online device

## Troubleshooting

### Common Issues

**Wallet not found:**
- Check wallet name spelling
- Verify wallet exists with `wallet list`
- Ensure correct network parameter

**Import failures:**
- Verify mnemonic phrase accuracy (12 words, correct spelling)
- Check private key format
- Ensure backup file is not corrupted

**Permission errors:**
- Check file system permissions on wallet directory
- Ensure sufficient disk space for wallet operations

### Recovery Procedures

If wallet files are corrupted or lost:
1. Use `import-mnemonic` with your backed-up mnemonic phrase
2. Use `import-private-key` if you have the private key backup
3. Use `import-file` with your wallet backup file
