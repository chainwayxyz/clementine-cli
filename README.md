# Clementine CLI

A wallet-agnostic command-line tool for depositing 10 BTC from Bitcoin to Citrea and withdrawing 10 cBTC from Citrea to Bitcoin.

For more information about the Clementine bridge, visit [https://docs.citrea.xyz/essentials/clementine-trust-minimized-bitcoin-bridge](https://docs.citrea.xyz/essentials/clementine-trust-minimized-bitcoin-bridge).

If you are looking for bridging smaller amounts, you can use third party bridges, visit [https://citrea.xyz/bridge](https://citrea.xyz/bridge) for more information.

If you encounter any issues, email us at [clementine-cli@citrea.xyz](mailto:clementine-cli@citrea.xyz).

## Installation

1. Install Rust:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

2. Install Clementine CLI:

```sh
cargo install --git https://github.com/chainwayxyz/clementine-cli --tag v0.1.0-rc.1 --locked --force
```

3. Install the default configuration by running the CLI init command which
   creates the `~/.clementine/bridge_cli_config.toml` file for you:

```sh
clementine-cli init
```

4. Show configuration:

```sh
clementine-cli show-config
```

5. Show help:

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

- [Wallet Guide](docs/wallet.md) - Wallet operations
- [Deposit Guide](docs/deposit.md) - Deposit from Bitcoin to Citrea
- [Withdrawal Guide](docs/withdraw.md) - Withdraw from Citrea to Bitcoin