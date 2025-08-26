# Clementine CLI

A wallet-agnostic command-line tool for interacting with Citrea, supporting secure Bitcoin deposits and withdrawals without requiring wallet connection.

## Features

- Deposit and withdrawal flows for Citrea
- Airgapped key generation and signing
- Bitcoin address and transaction utilities
- Backend integration via HTTP (using reqwest)
- Wallet-agnostic: no wallet connection required

## Installation

Install Rust and Cargo if you haven't already:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Configure Bitcoin sections in the [bridge_cli_config.toml](bridge_cli_config.toml)
with the correct credentials (only networks that are going to be used need
correct values). Than, install Clementine CLI (from project root):

```sh
mkdir -p ~/.clementine
cp bridge_cli_config.toml ~/.clementine/
cargo install --path .
```

## Usage

Run `clementine --help` to see available commands.

## Security Warning

Some commands (notably key generation and signing) must be run in an airgapped environment. The CLI will prompt for confirmation before proceeding with sensitive operations.

## Documentation

See the [CLI documentation](./docs/cli.md) for detailed command usage.

## Contributing

- If you have suggestions for naming or structure, please open an issue or PR.
- For musig2 and advanced cryptography, see TODOs and stubs in the codebase.

## License

MIT
