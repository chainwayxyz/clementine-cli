# Clementine CLI

A wallet-agnostic command-line tool for interacting with Citrea, supporting
secure Bitcoin deposits and withdrawals without requiring wallet connection.

## Features

- Deposit to and withdrawal from Citrea
- Airgapped key generation and signing
- Bitcoin address and transaction generator/signer utilities
- Backend interaction via HTTP
- Wallet-agnostic: No wallet connection required

## Installation

Install Rust and Cargo if you haven't already:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Install Clementine CLI (from project root):

```sh
cargo install --path . # TODO: check what happens to config file
```

## Usage and Documentation

Run `clementine --help` to see available commands. Or visit [documentation](docs/)
for detailed steps.

## Security Warning

Some commands (notably key generation and signing) must be run in an airgapped
environment. The CLI will prompt for confirmation before proceeding with
sensitive operations.

## Contributing

- If you have suggestions for naming or structure, please open an issue or a PR
- For musig2 and advanced cryptography, see stubs in the codebase

## License

MIT
