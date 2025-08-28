# Utilizing Clementine Wallet

Clementine CLI provides commands for managing wallet operations. To view all
commands, run `clementine-cli wallet`.

## Create Wallet

You can create a wallet using `wallet create` command. Choose a name for your
wallet and follow the instructions.

```sh
clementine-cli wallet create <WALLET NAME>
```

## Exporting and Importing a Wallet

### Export/Backup a Wallet

A wallet can be backed up using:

```sh
clementine-cli wallet backup <BACKUP DIRECTORY> <WALLET NAME>

# Example of backing up a wallet named `wallet1` to the current directory
clementine-cli wallet backup . wallet1
```

After that, a `wallet_<name>.json` file will be available as a backup.

### Import Wallet Using File

Previously exported wallets can be imported using `import-file` command:

```sh
clementine-cli wallet import-file <FILE NAME> <WALLET NAME>

# Example of importing a wallet named `wallet1` in the current directory
clementine-cli wallet import-file wallet_wallet1.json wallet1
```

### Import Private Key

If you already have a recovery taproot address, you can import it as a wallet
using the import utility with your secret key. It will be marked as imported via
private key when you list wallets using `clementine-cli wallet list`.

```sh
clementine-cli wallet import-private-key <WALLET NAME>
```

### Import Using Mnemonic

You can also import a wallet using the 12 word mnemonic:

```sh
clementine-cli wallet import-mnemonic <WALLET NAME>
```
