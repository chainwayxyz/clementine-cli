# Withdrawing from Citrea

This guide covers the withdrawal process from Citrea back to Bitcoin using Clementine CLI. Withdrawals follow a specific sequential process.

All available withdrawal commands can be viewed using `clementine-cli withdraw --help` command and `clementine-cli withdraw <subcommand> --help`.

> [!CAUTION]
> Don't forget to specify the `--network` flag if you plan to use a different Bitcoin network other than mainnet. Mainnet is selected implicitly 
> for every command. Network aliases are used interchangeably throughout the docs: `bitcoin`/`mainnet`, `testnet4`/`testnet`, `signet`/`devnet`, 
> and `regtest`.

## Prerequisites

Before starting a withdrawal, ensure you have:

- A Clementine CLI wallet with `withdrawal` purpose ("wit" prefix address) which will be used as the `signer address`
- A Bitcoin address where funds will be sent (destination address)
- Access to Citrea wallet with 10 cBTC to withdraw. This can be on a MultiSig or an EOA wallet.

## Withdrawal Process Overview

The withdrawal process follows these sequential steps:

1. [**Create Withdrawal Wallet**](#step-1-create-a-wallet-for-withdrawal) - Create a withdrawal wallet for the signer address
2. [**Start Withdrawal**](#step-2-start-withdrawal) - Initiate withdrawal process (prompts for Bitcoin transaction)
3. [**Send Bitcoin Transaction**](#step-3-send-required-bitcoin-transaction) - Send 330 sats to the signer address to create withdrawal UTXO
4. [**Scan for Withdrawals**](#step-4-scan-for-withdrawals) - Find available withdrawal UTXOs
5. [**Generate Withdrawal Signatures**](#step-5-generate-withdrawal-signatures) - Create signatures for optimistic and operator-paid withdrawals
6. [**Send**](#step-6-send) - Send withdrawal request with signature to Citrea for optimistic withdrawal along with sending 10 cBTC to the bridge contract for registration of the withdrawal operation. After successful optimistic withdrawal, you will receive your ~10 BTC on your Bitcoin address.
7. [**Check Status**](#step-7-check-withdrawal-status) - Monitor withdrawal progress for optimistic withdrawal for 12 hours
8. [**Send Signature to Operators**](#step-8-send-the-signature-to-the-operators) - If Step 6 fails, submit signature to Clementine operators for operator-paid withdrawal
9. [**Check Status**](#step-7-check-withdrawal-status) - Monitor withdrawal progress for operator-paid withdrawal

> [!IMPORTANT]
> The `SIGNER_ADDRESS` and the `DESTINATION_ADDRESS` are different. The `SIGNER_ADDRESS` will belong to your Clementine wallet to be able to perform withdrawal specific signing operations, whereas `DESTINATION_ADDRESS` is the address that the withdrawn BTC funds will be sent to.

## Step 1: Create a Wallet for Withdrawal

A new address for withdrawal is required:

```sh
clementine-cli wallet create --network <BITCOIN_NETWORK> <WALLET_NAME> withdrawal
```

**Example:**

```sh
clementine-cli wallet create --network testnet my_withdrawal_wallet withdrawal
```

For more detailed wallet usage, please check the [wallet documentation](wallet.md).

## Step 2: Start Withdrawal

Start the withdrawal process:

```sh
clementine-cli withdraw start [--network <BITCOIN_NETWORK>] <SIGNER_ADDRESS> <DESTINATION_ADDRESS>
```

**Parameters:**

- `SIGNER_ADDRESS`: Withdrawal wallet address with "wit" prefix
- `DESTINATION_ADDRESS`: Bitcoin address where funds will be sent

**Example:**

```sh
clementine-cli withdraw start wittb1pf... tb1qg...
# For testnet4
clementine-cli withdraw start --network testnet wittb1pf... tb1qg...
```

This command will prompt the user to send 330 sats to the signer address to create the dust UTXO needed for the withdrawal operation.

> [!IMPORTANT]
> **About the "wit" prefix:** The signer address must belong to a Clementine wallet with `withdrawal` purpose and should be prefixed with "wit" to indicate it's being used for withdrawal operations. This ensures proper cryptographic derivation for withdrawal bridge operations.

**Important:** After running this command, you'll need to send the prompted Bitcoin transaction to the signer address before proceeding.

## Step 3: Send Required Bitcoin Transaction

Send 0.00000330 BTC to the prompted address given by the start command to create your withdrawal UTXO.

> [!NOTE]
> You'll notice the address used in the command above matches your signer address but lacks the `wit` prefix. This is intentional—the prefix is only needed for Clementine-specific operations, whereas regular transactions use the address in its standard form.
> [!IMPORTANT]
> If your wallet cannot send exactly 0.00000330 BTC, you may want to send a slightly higher amount. Be sure to update the config to match the amount you actually sent before proceeding.
> run `clementine-cli update-config` to update the `dust_utxo_amount` to the amount you actually sent.

This creates the small UTXO needed for the withdrawal operation.

## Step 4: Scan for Withdrawals

Scan for available withdrawal UTXOs:

```sh
clementine-cli withdraw scan [--network <BITCOIN_NETWORK>] <SIGNER_ADDRESS> <DESTINATION_ADDRESS>
```

**Example:**

```sh
clementine-cli withdraw scan wittb1pf... tb1qg...
# For testnet4
clementine-cli withdraw scan --network testnet wittb1pf... tb1qg...
```

**What this command does:**

The scan command provides detailed information about your withdrawal:

- **Why**: Explains that it's locating UTXOs to withdraw funds from Citrea back to Bitcoin
- **Where**: Displays the Bitcoin network, signer address, destination address, and expected UTXO amount
- **When**: Shows block height and confirmation status of each found UTXO, indicating whether they're ready for withdrawal or still pending confirmation

The command will:

1. Scan the Bitcoin network for UTXOs at your signer address
2. Display detailed information about each UTXO including:
   - OutPoint (transaction ID and output index)
   - Amount in BTC and satoshis
   - Block height (if confirmed) or mempool status (if unconfirmed)
   - Current status (ready for withdrawal or waiting for confirmation)
3. Filter and identify UTXOs that match the expected dust amount
4. Filter UTXOs that are already used in a withdrawal operation, and warn user not to reuse their signer address
5. Provide the exact command to generate withdrawal signatures for each valid UTXO

> [!TIP]
> If a UTXO is unconfirmed, you'll need to wait for Bitcoin network confirmation before proceeding with signature generation.

## Step 5: Generate Withdrawal Signatures

Generate the signatures for optimistic withdrawal and operator-paid withdrawal using your Clementine CLI wallet.

This command will generate two signatures: one for `optimistic` withdrawal (which has an exact amount of 999999760 satoshis, or 9.9999976 BTC), and one for `operator-paid` withdrawal (which has an exact amount of 997000000 satoshis, or 9.97 BTC).

```sh
clementine-cli withdraw generate-withdrawal-signatures [--network <BITCOIN_NETWORK>] <SIGNER_ADDRESS> <DESTINATION_ADDRESS> <WITHDRAWAL_UTXO>
```

**Example:**

```sh
clementine-cli withdraw generate-withdrawal-signatures wittb1pf... tb1qg... abc123def456...:0
# For testnet4
clementine-cli withdraw generate-withdrawal-signatures --network testnet wittb1pf... tb1qg... abc123def456...:0
```

> [!CAUTION]
> Save the generated signatures since they will be used to authorize the withdrawal operations that will be done later.

## Step 6: Send 10 cBTC to the Bridge Contract

Send 10 cBTC to the bridge contract for registration of the withdrawal operation. After successful optimistic withdrawal, you will receive your ~10 BTC on your Bitcoin address.

Run the following command to open the webpage where you can send the 10 cBTC to the bridge contract along with the generated withdrawal signature that will be used for optimistic withdrawal.

```sh
clementine-cli withdraw send [--network <BITCOIN_NETWORK>] <SIGNER_ADDRESS> <DESTINATION_ADDRESS> <WITHDRAWAL_UTXO> <OPTIMISTIC_SIGNATURE>
```

**Example:**

```sh
clementine-cli withdraw send wittb1pf... tb1qg... abc123def456:0 807c42770...
# For testnet4
clementine-cli withdraw send --network testnet wittb1pf... tb1qg... abc123def456:0 807c42770...
```

## Step 7: Check Withdrawal Status

Monitor the status of your withdrawal:

```sh
clementine-cli withdraw status [--network <BITCOIN_NETWORK>] <WITHDRAWAL_UTXO>
```

**Parameters:**

- `WITHDRAWAL_UTXO`: Withdrawal UTXO in `txid:vout` format

**Example:**

```sh
clementine-cli withdraw status 4f38192dba8b52fd4327d5c67a3fc2c61fc407a556ee19258026f83dde84798a:0
# For testnet4
clementine-cli withdraw status --network testnet 4f38192dba8b52fd4327d5c67a3fc2c61fc407a556ee19258026f83dde84798a:0
```

The status will show the response from the backend.

## Step 8: Send the Signature to the Operators

If the optimistic withdrawal does not complete within 12 hours, submit the generated `operator-paid` withdrawal signature to bridge operators for `operator-paid` withdrawal processing:

```sh
clementine-cli withdraw send-withdrawal-signature-to-operators [--network <BITCOIN_NETWORK>] <SIGNER_ADDRESS> <DESTINATION_ADDRESS> <WITHDRAWAL_UTXO> <OPERATOR_PAID_SIGNATURE>
```

**Example:**

```sh
clementine-cli withdraw send-withdrawal-signature-to-operators  wittb1pf... tb1qg... abc123def456:0 807c42770...
# For testnet4
clementine-cli withdraw send-withdrawal-signature-to-operators --network testnet wittb1pf... tb1qg... abc123def456:0 807c42770...
```

> [!NOTE]
> After sending the `operator-paid` signature, Clementine Operators will validate and process the withdrawal. Use Step 6 to monitor the status.

## Troubleshooting

- **Address rejected**: `DESTINATION_ADDRESS` must be a normal Bitcoin address without the `wit`/`dep` prefix and not one of your Clementine wallet addresses.
- **No valid UTXOs found in `withdraw scan`**: the dust UTXO must be confirmed and match `dust_utxo_amount`.
- **Signature rejected**: regenerate signatures with the exact `WITHDRAWAL_UTXO` from scan and use the optimistic signature for `withdraw send` or the operator-paid signature for `send-withdrawal-signature-to-operators`.
