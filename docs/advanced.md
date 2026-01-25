# Advanced Usage

This guide covers non-default networks, API/RPC configuration, and hidden CLI commands that are
available but not shown in `--help`.

## Enable Signet And Regtest

`clementine-cli init` creates the bridge config at `~/.clementine/bridge_cli_config.toml`.
The file is a TOML document with a top-level table per network.

By default, only `[bitcoin]` and `[testnet4]` are created. To use signet or regtest,
add `[signet]` and/or `[regtest]` sections to the same file.

Copy existing network sections. Update all parameters in the new network section to match
the target network you are adding.
Adjust endpoints and RPC settings for your environment. Exactly one of
`esplora_rest_api` or `bitcoin_config` must be set per network.

After saving the file, you can verify with:

```sh
clementine-cli show-config --network signet
clementine-cli show-config --network regtest
```

## Bitcoin RPC (Manual)

RPC settings are not added by default. If you want to use Bitcoin Core RPC instead of
Esplora, you must edit the config file manually to add a `bitcoin_config` section and
remove `esplora_rest_api` for that network.

Bitcoin API selection order:

1. `esplora_rest_api` if set (even if `bitcoin_config` is also set)
2. `bitcoin_config` if set
3. Error if neither is set (`No Bitcoin API configured`)

Example (regtest):

```toml
[regtest]
network = "regtest"
aggregated_public_key = "30ff95ec2726938072a2009f3276cd8fba2363d9284a7eb01217b2f302eb8577"
citrea_chain_id = 5655
citrea_backend_endpoint = "https://127.0.0.1/"
user_takes_after = 200
bridge_amount = 1000000000
optimistic_withdrawal_amount = 999999760
operator_withdrawal_amount = 997000000
dust_utxo_amount = 330
bridge_contract_address = "0x3100000000000000000000000000000000000002"
move_tx_finalization_blocks = 5

[regtest.bitcoin_config]
url = "http://127.0.0.1:18443/" # Add `/wallet/name` if necessary
user = "admin"
password = "admin"
```

## Hidden Commands

The CLI includes a few subcommands marked hidden in the help output. They are supported,
but intended for advanced workflows.

### Update Config

```sh
clementine-cli update-config --network <network> key=value ...
```

Updates existing keys in `~/.clementine/bridge_cli_config.toml`. It refuses to create
new keys, so add `[signet]` or `[regtest]` tables first if needed.

### Deposit Params

```sh
clementine-cli deposit get-deposit-params --network <network> <MOVE_TO_VAULT_TXID>
```

Outputs hex-encoded deposit parameters for a move-to-vault transaction, useful for manual deposit submission.

### Safe Withdraw

```sh
clementine-cli withdraw send-safe-withdraw --network <network> <SIGNER_ADDRESS> <DESTINATION_ADDRESS> <WITHDRAWAL_UTXO> <OPTIMISTIC_SIGNATURE>
```

Sends a safe withdrawal transaction directly to the bridge contract if the normal
browser flow fails. Requires `citrea_rpc_url` to be set in
`~/.clementine/bridge_cli_config.toml` under the selected network section, and it is
not included by default, so add it manually. See [Withdrawal Guide](withdraw.md) for
the primary withdrawal flow.
