# Clementine CLI

A wallet-agnostic command-line tool for depositing 10 BTC from Bitcoin to Citrea and withdrawing 10 cBTC from Citrea to Bitcoin.

For more information about the Clementine bridge, visit [https://docs.citrea.xyz/essentials/clementine-trust-minimized-bitcoin-bridge](https://docs.citrea.xyz/essentials/clementine-trust-minimized-bitcoin-bridge).

If you are looking for bridging smaller amounts, you can use third party bridges, visit [https://citrea.xyz/bridge](https://citrea.xyz/bridge) for more information.

If you encounter any issues, email us at [clementine-cli@citrea.xyz](mailto:clementine-cli@citrea.xyz).

## Installation

Choose one of the following installation paths.

### Option A: Build from source

Install Rust:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Install Clementine CLI:

```sh
cargo install --git https://github.com/chainwayxyz/clementine-cli --tag v0.1.0-rc.1 --locked --force
```

### Option B: Download a pre-built binary and verify it

- [Download & Verify](docs/download-verify.md)

### Initialize and configure

Install the default configuration by running the CLI init command which creates
the `~/.clementine/bridge_cli_config.toml` file for you:

```sh
clementine-cli init
```

Show configuration:

```sh
# Show current config for a network
clementine-cli show-config
```

Show help:
```sh
clementine-cli --help
```

## Quick Usage

By default, `clementine-cli` uses `bitcoin` (mainnet) network. If you wish to
make deposits and withdrawals on testnet, please provide `--network testnet` flag
every time you invoke `clementine-cli`.

### Deposit

```sh
# Get help
clementine-cli --help

# Create wallet for deposit (Recovery Taproot Address) (airgapped device only)
clementine-cli wallet create my-deposit-wallet deposit

# Start deposit (generate deposit address)
clementine-cli deposit start <RECOVERY_TAPROOT_ADDRESS> <CITREA_ADDRESS>

# Send 10 BTC to the shown address as prompted by the start command

# Monitor deposits (online device)
clementine-cli deposit status <DEPOSIT_ADDRESS>
```

### Withdrawal

```sh
# Create wallet for withdrawal (Withdrawal Taproot Address) (airgapped device only)
clementine-cli wallet create my-withdrawal-wallet withdrawal

# Start withdrawal (generate withdrawal address)
clementine-cli withdraw start <WITHDRAWAL_TAPROOT_ADDRESS> <DESTINATION_ADDRESS>

# Send 330 sats to the shown address as prompted by the start command
# Then run withdrawal scan command to find available withdrawal UTXOs
clementine-cli withdraw scan <WITHDRAWAL_TAPROOT_ADDRESS> <DESTINATION_ADDRESS>

# Run the prompted commands to generate withdrawal signatures and send withdrawal request to Citrea for optimistic withdrawal

# Monitor withdrawals (online device)
clementine-cli withdraw status <WITHDRAWAL_ADDRESS>
```

## Two-Device Security

Clementine CLI can be used with two devices for maximum security:

- **Offline device**: All wallet creation, key generation, and signing operations
- **Online Device**: Status monitoring, address generation, broadcasting

## Documentation

See [docs/README.md](docs/README.md) for an overview of how to use this CLI to
deposit to and withdraw from Citrea.

- [Download & Verify](docs/download-verify.md) - Download pre-built binaries and verify signatures
- [Wallet Guide](docs/wallet.md) - Airgapped wallet operations
- [Deposit Guide](docs/deposit.md) - Deposit Bitcoin to Citrea
- [Withdrawal Guide](docs/withdraw.md) - Withdrawal from Citrea to Bitcoin
- [Advanced Usage](docs/advanced.md) - Advanced users only: signet/regtest config, API/RPC selection, hidden CLI commands
