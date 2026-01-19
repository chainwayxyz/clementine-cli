# Clementine CLI

A wallet-agnostic command-line tool for interacting with Citrea, supporting secure Bitcoin deposits and withdrawals without requiring wallet connection.

## Features

- **Bridge Operations**: Deposit to and withdraw from Citrea network
- **Airgapped Security**: Key generation and signing in secure environments
- **Wallet Management**: Create, import, and manage Clementine wallets locally
- **Wallet-agnostic**: No external wallet connection required
- **Recovery Support**: Built-in fund recovery mechanisms

## Installation

1. Install Rust:

   ```sh
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. Install Clementine CLI:

   ```sh
   cargo install --path .
   ```

3. Install the default configuration by running the CLI init command which
   creates the `~/.clementine/bridge_cli_config.toml` file for you:

   ```sh
   clementine-cli init
   ```

4. Show and update configuration

   ```sh
   # Show current config for a network
   clementine-cli show-config --network testnet

   # Update one or more keys (interactive confirmation). Use -y to skip confirmation prompts.
   clementine-cli update-config --network testnet bitcoin_config.user=admin bitcoin_config.password=admin
   clementine-cli update-config --network testnet -y bitcoin_config.user=admin bitcoin_config.password=admin
   ```

> [!CAUTION]
> Please make sure that you did not rename the file, as this will prevent the CLI from detecting the file.

## Quick Usage

By default, `clementine-cli` uses `bitcoin` (mainnet) network. If you wish to
make deposits and withdrawals on other networks, please provide `--network` flag
every time you invoke `clementine-cli`.

```sh
# Get help
clementine-cli --help

# Create wallet for deposit (Recovery Taproot Address) (airgapped device only)
clementine-cli wallet create my-deposit-wallet deposit # Mainnet
clementine-cli wallet create --network testnet my-deposit-wallet deposit

# Generate Deposit Address
clementine-cli deposit get-deposit-address --network testnet <RECOVERY_TAPROOT_ADDRESS> <CITREA_ADDRESS>

# Monitor deposits (online device)
clementine-cli deposit status <DEPOSIT_ADDRESS> # Mainnet
clementine-cli deposit status --network testnet <DEPOSIT_ADDRESS>
```

## Two-Device Security

Clementine CLI requires two devices for maximum security:

- **Airgapped Device**: All wallet creation, key generation, and signing operations
- **Online Device**: Status monitoring, address generation, broadcasting
- **Never**: Connect airgapped device to internet
- **Always**: Verify the correctness of operations before interacting with Citrea or Bitcoin to prevent loss of funds

## Documentation

See [docs/README.md](docs/README.md) for an overview of how to use this CLI to
deposit to and withdraw from Citrea.

- [Wallet Guide](docs/wallet.md) - Airgapped wallet operations
- [Deposit Guide](docs/deposit.md) - Deposit Bitcoin to Citrea
- [Withdrawal Guide](docs/withdraw.md) - Withdrawal from Citrea to Bitcoin
