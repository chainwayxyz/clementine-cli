# Clementine CLI

A wallet-agnostic command-line tool for interacting with Citrea, supporting secure Bitcoin deposits and withdrawals without requiring wallet connection.

## Features

- **Bridge Operations**: Deposit to and withdraw from Citrea network
- **Airgapped Security**: Key generation and signing in secure environments
- **Wallet Management**: Create, import, and manage Clementine wallets locally
- **Wallet-agnostic**: No external wallet connection required
- **Recovery Support**: Built-in fund recovery mechanisms

## Installation

### Prerequisites
- **Online Device**: Bitcoin node access or mempool API
- **Both Devices**: Rust and Clementine CLI installation, secure data transfer method (USB, QR codes)

### Install

1. Install Rust:
   ```sh
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. Install Clementine CLI:
   ```sh
   cargo install --path .
   ```

3. Install your configuration by modifying according to your own Bitcoin configurations and copying your config file to `~/.clementine/`:
   ```sh
   mkdir -p ~/.clementine/  
   cp ./bridge_cli_config.toml ~/.clementine/  
   ```
Please make sure that you did not rename the file, as this will prevent the CLI from detecting the file.

## Quick Usage

```sh
# Get help
clementine-cli --help

# Create wallet for deposit (airgapped device only)
clementine-cli wallet create --network testnet4 my-deposit-wallet deposit

# Monitor deposits (online device)
clementine-cli deposit status --network testnet4 <DEPOSIT_ADDRESS>
```

## Two-Device Security

Clementine CLI requires two devices for maximum security:

- **Airgapped Device**: All wallet creation, key generation, and signing operations
- **Online Device**: Status monitoring, address generation, broadcasting
- **Never**: Connect airgapped device to internet
- **Always**: Verify the correctness of operations before interacting with Citrea or Bitcoin to prevent loss of funds

## Documentation

- [Wallet Guide](docs/wallet.md) - Airgapped wallet operations
- [Deposit Guide](docs/deposit.md) - Deposit Bitcoin to Citrea
- [Withdrawal Guide](docs/withdrawal.md) - Withdrawal from Citrea to Bitcoin

## License

MIT
