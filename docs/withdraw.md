# Withdrawing from Citrea

This guide covers the withdrawal process from Citrea back to Bitcoin using Clementine CLI. Withdrawals follow a specific sequential process using two devices for maximum security.

All available withdrawal commands can be viewed using `clementine-cli withdraw --help` command and `clementine-cli withdraw <subcommand> --help`.

## Prerequisites

Before starting a withdrawal, ensure you have:

- A Clementine wallet with `withdrawal` purpose ("wit" prefix address)
- A Bitcoin address where funds will be sent (destination address)
- Access to both airgapped and online devices
- Sufficient balance on Citrea to withdraw

> [!CAUTION]
> Don't forget to specify the `--network` flag if you plan to use a different Bitcoin
> network other than mainnet. Mainnet is selected implicitly for every
> command.

## Withdrawal Process Overview

The withdrawal process follows these sequential steps:

1. [**Create Withdrawal Wallet**](#step-1-create-a-wallet-for-withdrawal) - Create a withdrawal wallet for the signer address
2. [**Start Withdrawal**](#step-2-start-withdrawal) - Initiate withdrawal process (prompts for Bitcoin transaction)
3. [**Send Bitcoin Transaction**](#step-3-send-required-bitcoin-transaction) - Send required transaction to signer address to create withdrawal UTXO
4. [**Scan for Withdrawals**](#step-4-scan-for-withdrawals) - Find available withdrawal UTXOs
5. [**Generate Withdrawal Signatures**](#step-5-generate-withdrawal-signatures) - Create signatures on airgapped device
6. [**Safe Withdraw**](#step-6-safe-withdraw) - Submit withdrawal request with signature to Citrea for optimistic withdrawal
7. [**Check Status**](#step-7-check-withdrawal-status) - Monitor withdrawal progress for optimistic withdrawal for 12 hours
8. [**(Optional) Send Signature to Operators**](#step-8-send-the-signature-to-the-operators) - If Step 6 fails, submit signature to Clementine operators for processing
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
clementine-cli wallet create --network testnet4 my_withdrawal_wallet withdrawal
```

For more detailed wallet usage, please check the [wallet documentation](wallet.md).

## Step 2: Start Withdrawal

**ONLINE DEVICE OPERATION:** Start the withdrawal process:

```sh
clementine-cli withdraw start [--network <BITCOIN_NETWORK>] <SIGNER_ADDRESS> <DESTINATION_ADDRESS>
```

**Parameters:**

- `SIGNER_ADDRESS`: Withdrawal wallet address with "wit" prefix (from airgapped device)
- `DESTINATION_ADDRESS`: Bitcoin address where funds will be sent

> [!TIP]
> After you start a withdrawal with the withdrawal start command, it will prompt
> you with the next steps and the correct values. However, you may want to
> return to this document, as it contains details and security suggestions for
> the following steps.

**Example:**

```sh
clementine-cli withdraw start wittb1pf... tb1qg...
# For testnet4
clementine-cli withdraw start --network testnet4 wittb1pf... tb1qg...
```

This command will prompt the user to send a Bitcoin transaction that will create the 0-value UTXO needed for the withdrawal operation.

> [!IMPORTANT]
> **About the "wit" prefix:** The signer address must belong to a Clementine wallet with `withdrawal` purpose and should be prefixed with "wit" to indicate it's being used for withdrawal operations. This ensures proper cryptographic derivation for withdrawal bridge operations.

**Important:** After running this command, you'll need to send the prompted Bitcoin transaction to the signer address before proceeding.

## Step 3: Send Required Bitcoin Transaction

Send the Bitcoin transaction as prompted by the start command:

```sh
bitcoin-cli -testnet4 sendtoaddress <SIGNER_ADDRESS> <AMOUNT>
```

**Example:**

```sh
bitcoin-cli -testnet4 sendtoaddress wittb1pf... 0.00000330
```

> [!IMPORTANT]
> If your wallet cannot send exactly 330 sats, you may send a higher supported amount. Be sure to update the config to match the amount you actually sent before proceeding. 

This creates the 0-value UTXO needed for the withdrawal operation.

## Step 4: Scan for Withdrawals

**ONLINE DEVICE OPERATION:** Scan for available withdrawal UTXOs that can be used for the withdrawal operation:

```sh
clementine-cli withdraw scan [--network <BITCOIN_NETWORK>] <SIGNER_ADDRESS> <DESTINATION_ADDRESS>
```

**Example:**

```sh
clementine-cli withdraw scan wittb1pf... tb1qg...
# For testnet4
clementine-cli withdraw scan --network testnet4 wittb1pf... tb1qg...
```

This command will scan Bitcoin network and return possible withdrawal scenarios for appropriate UTXOs, with corresponding prompt to generate the necessary signature.

## Step 5: Generate Withdrawal Signatures

**AIRGAPPED DEVICE ONLY:** Generate the signatures for optimistic withdrawal and operator-paid withdrawal:

### Prepare Data Transfer

**Transfer from online device to airgapped device:**

- Signer address for signing withdrawals (wit-prefixed, taproot)
- Destination address where withdrawn BTC will be sent
- Withdrawal UTXO details (from scan command)

### Generate Signature

> [!IMPORTANT]
> This command will generate two signatures: one for `optimistic` withdrawal (which has an exact amount of 999999760 satoshis, or 9.9999976 BTC), and one for `operator-paid` withdrawal (which hash an exact amount of 997000000 satoshis, or 9.97 BTC).

```sh
clementine-cli withdraw generate-withdrawal-signatures [--network <BITCOIN_NETWORK>] <SIGNER_ADDRESS> <DESTINATION_ADDRESS> <WITHDRAWAL_UTXO>
```

**Example:**

```sh
clementine-cli withdraw generate-withdrawal-signatures wittb1pf... tb1qg... abc123def456...:0
# For testnet4
clementine-cli withdraw generate-withdrawal-signatures --network testnet4 wittb1pf... tb1qg... abc123def456...:0
```

> [!CAUTION]
> Save the generated signatures since they will be used to authorize the operations that will be done later.

## Step 6: Safe Withdraw

**ONLINE DEVICE OPERATION:** Execute the `optimistic` withdrawal with signature verification and submit to Citrea:

```sh
clementine-cli withdraw safe-withdraw [--network <BITCOIN_NETWORK>] <SIGNER_ADDRESS> <DESTINATION_ADDRESS> <WITHDRAWAL_UTXO> <OPTIMISTIC_SIGNATURE>
```

**Example:**

```sh
clementine-cli withdraw safe-withdraw wittb1pf... tb1qg... abc123def456:0 807c42770...
# For testnet4
clementine-cli withdraw safe-withdraw --network testnet4 wittb1pf... tb1qg... abc123def456:0 807c42770...
```

**What this does:**

- Verifies the signature against the withdrawal parameters
- Provides final transaction confirmation
- Prompts to Ethereum wallet to submit the withdrawal transaction to Citrea

In case `safe-withdraw` fails, you can send your withdrawal transaction directly to the bridge contract by using `send-safe-withdrawal`:

```sh
clementine-cli withdraw send-safe-withdrawal [--network <BITCOIN_NETWORK>] <SIGNER_ADDRESS> <DESTINATION_ADDRESS> <WITHDRAWAL_UTXO> <OPTIMISTIC_SIGNATURE>
```

> [!IMPORTANT]
> This command will submit the `optimistic` withdrawal signature to the Bridge contract. For 12 hours, the backend will wait for Clementine Signers to provide `optimistic` withdrawal transaction. If this fails, you will need to use `operator-paid` withdrawal with its signature that is generated in Step 4.

## Step 7: Check Withdrawal Status

**ONLINE DEVICE OPERATION:** Monitor the status of your withdrawal:

```sh
clementine-cli withdraw status [--network <BITCOIN_NETWORK>] <WITHDRAWAL_UTXO>
```

**Parameters:**

- `WITHDRAWAL_UTXO`: Withdrawal UTXO in `txid:vout` format

**Example:**

```sh
clementine-cli withdraw status 4f38192dba8b52fd4327d5c67a3fc2c61fc407a556ee19258026f83dde84798a:0
# For testnet4
clementine-cli withdraw status --network testnet4 4f38192dba8b52fd4327d5c67a3fc2c61fc407a556ee19258026f83dde84798a:0
```

The status will show the response from the backend.

## Step 8: Send the Signature to the Operators

**ONLINE DEVICE OPERATION:** If Step 5 fails (Clementine Signers fail to provide `optimistic` withdrawal in 12 hours), submit the generated `operator-paid` withdrawal signature to bridge operators for `operator-paid` withdrawal processing:

```sh
clementine-cli withdraw send-withdrawal-signature-to-operators [--network <BITCOIN_NETWORK>] <SIGNER_ADDRESS> <DESTINATION_ADDRESS> <WITHDRAWAL_UTXO> <OPERATOR_PAID_SIGNATURE> <WITHDRAWAL_INDEX>
```

**Example:**

```sh
clementine-cli withdraw send-withdrawal-signature-to-operators  wittb1pf... tb1qg... abc123def456:0 807c42770... 1
# For testnet4
clementine-cli withdraw send-withdrawal-signature-to-operators --network testnet4  wittb1pf... tb1qg... abc123def456:0 807c42770... 1
```

> [!NOTE]
> After sending the `operator-paid` signature, Clementine Operators will validate and process the withdrawal. Use Step 6 to monitor the status.

**Critical Security Note:** ALL cryptographic operations (Step 4) must be performed on the airgapped device. Network operations and transaction submission occur on the online device.

## Security Considerations

**Critical Security Requirements:**

- **Airgapped Signing**: ALL signature generation must occur on airgapped device
- **Address Verification**: Always verify withdrawal and destination addresses before signing
- **Signature Protection**: Never share or expose withdrawal signatures
- **Data Verification**: Cross-check all parameters between devices
- **Status Monitoring**: Regularly monitor withdrawal progress

**Best Practices:**

1. **Device Isolation**: Keep airgapped device permanently offline
2. **Secure Transfer**: Use formatted USB drives or QR codes for data transfer
3. **Parameter Verification**: Double-check all withdrawal parameters
4. **Backup Strategy**: Save all transaction details and signatures
5. **Network Consistency**: Use same `--network` parameter on both devices

## Troubleshooting

### Common Issues

- **Withdrawal not found**: Ensure you're using the correct withdrawal index
- **Signature validation failed**: Verify all parameters match exactly
- **UTXO not available**: The withdrawal UTXO may have been spent or is not yet confirmed
- **Network connectivity issues**: Check your connection to the Bitcoin network and Citrea bridge

### Getting Help

For additional help with withdraw commands:

```sh
clementine-cli withdraw --help
clementine-cli withdraw <command> --help
```
