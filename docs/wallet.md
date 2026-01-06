# Wallet Operations

Clementine CLI provides comprehensive wallet management capabilities with strong security features. All wallet operations can be performed offline for maximum security.

> [!CAUTION]
> Don't forget to specify the `--network` flag if you plan to use a different Bitcoin
> network other than mainnet. Mainnet is selected implicitly for every
> command.

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

> [!CAUTION]
> Never manually remove or modify address prefixes, as this can lead to:
>
> - Failed Clementine interactions
> - Permanent loss of funds due to misinterpretation or confusion
> - Inability to recover funds from bridge operations

View all wallet commands: `clementine-cli wallet --help`

> [!IMPORTANT]
> It is strongly advised to perform all wallet-related operations on an air-gapped device.

## Create Wallet

```sh
clementine-cli wallet create [--network <BITCOIN_NETWORK>] <WALLET-LABEL> <PURPOSE>
```

**Example:**

```sh
# For mainnet:
clementine-cli wallet create my-wallet deposit
# For a different network:
clementine-cli wallet --network testnet create my-wallet deposit
```

> [!WARNING]
Do not send any funds to your Clementine wallet addresses unless otherwise specified.

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
clementine-cli wallet import-file <FILE-NAME> [WALLET-LABEL]

# Example of importing a wallet in the current directory:
clementine-cli wallet import-file wallet_<address>.json
# Example with label:
clementine-cli wallet import-file wallet_<address>.json wallet1
```

- `<WALLET-LABEL>` is optional. If omitted, the label will be inferred from the file.

### Import Private Key

If you already have a recovery taproot address, you can import it as a wallet
using the import utility with your secret key. It will be marked as imported via
private key when you list wallets using `clementine-cli wallet list`.

> [!NOTE]
> This is especially useful if you generated your recovery taproot address using the [website](https://citrea.xyz/bridge); however, this is not recommended. We suggest using those keys only for testing purposes.

```sh
clementine-cli wallet import-private-key [--network <BITCOIN_NETWORK>] <WALLET-LABEL> <PURPOSE>
```

### Import Using Mnemonic

You can also import a wallet using the 12 word mnemonic:

```sh
clementine-cli wallet import-mnemonic [--network <BITCOIN_NETWORK>] <WALLET-LABEL> <PURPOSE>
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
clementine-cli wallet show-mnemonic <WALLET_ADDRESS>
```

### Show Private Key

Display the private key for a wallet (use with extreme caution):

```sh
clementine-cli wallet show-private-key <WALLET_ADDRESS>
```

## Security Best Practices

### Critical Security Requirements

- **Airgapped Environment**: Perform all wallet creation and key operations offline
- **Secure Storage**: Store mnemonic phrases and private keys in encrypted, offline storage
- **Multiple Backups**: Keep wallet backups in multiple secure, geographically distributed locations
- **Access Control**: Limit access to wallet files and ensure proper file system permissions

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
