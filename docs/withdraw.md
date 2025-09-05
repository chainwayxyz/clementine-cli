# Withdrawing from Citrea

This guide covers the withdrawal process from Citrea back to Bitcoin using Clementine CLI. Withdrawals follow a specific sequential process using two devices for maximum security.

All available withdrawal commands can be viewed using `clementine-cli withdraw --help` command and `clementine-cli withdraw <subcommand> --help`.

## Prerequisites

Before starting a withdrawal, ensure you have:

- A Clementine wallet with `withdrawal` purpose ("wit" prefix address)
- A Bitcoin address where funds will be sent (claim address)
- Access to both airgapped and online devices
- Sufficient balance on Citrea to withdraw

## Withdrawal Process Overview

The withdrawal process follows these sequential steps:

1. **Start Withdrawal** - Initiate withdrawal process (prompts for Bitcoin transaction)
2. **Send Bitcoin Transaction** - Send required transaction to signer address
3. **Scan for Withdrawals** - Find available withdrawal UTXOs
4. **Generate Withdrawal Signature** - Create signature on airgapped device
5. **Safe Withdraw** - Submit withdrawal request with signature to Citrea
6. **Send Signature to Operators** - Submit signature to Clementine operators for processing
7. **Check Status** - Monitor withdrawal progress

> [!IMPORTANT]
> The `SIGNER_ADDRESS` and the `CLAIM_ADDRESS` are different. The `SIGNER_ADDRESS` will belong to your Clementine wallet to be able to perform withdrawal specific signing operations, whereas `CLAIM_ADDRESS` is the address that the withdrawn BTC funds will be sent to.

## Step 1: Start Withdrawal (Online Device)

**BOTH DEVICES:** Start the withdrawal process:

```sh
clementine-cli withdraw start --network <BITCOIN_NETWORK> <SIGNER_ADDRESS> <CLAIM_ADDRESS>
```

**Parameters:**

- `SIGNER_ADDRESS`: Withdrawal wallet address with "wit" prefix (from airgapped device)
- `CLAIM_ADDRESS`: Bitcoin address where funds will be sent

**Example:**

```sh
clementine-cli withdraw start --network testnet4 wittb1pf... tb1qg...
```

This command will prompt the user to send a Bitcoin transaction that will create the 0-value UTXO needed for the withdrawal operation.

> [!IMPORTANT]
> **About the "wit" prefix:** The signer address must belong to a Clementine wallet with `withdrawal` purpose and should be prefixed with "wit" to indicate it's being used for withdrawal operations. This ensures proper cryptographic derivation for withdrawal bridge operations.

**Important:** After running this command, you'll need to send the prompted Bitcoin transaction to the signer address before proceeding.

## Step 2: Send Required Bitcoin Transaction (Online Device)

**ONLINE DEVICE OPERATION:** Send the Bitcoin transaction as prompted by the start command:

```sh
bitcoin-cli -testnet4 sendtoaddress <SIGNER_ADDRESS> <AMOUNT>
```

**Example:**

```sh
bitcoin-cli -testnet4 sendtoaddress wittb1pf... 0.00000330
```

This creates the 0-value UTXO needed for the withdrawal operation.

## Step 3: Scan for Withdrawals (Online Device)

**ONLINE DEVICE OPERATION:** Scan for available withdrawal UTXOs that can be used for the withdrawal operation:

```sh
clementine-cli withdraw scan --network <BITCOIN_NETWORK> <SIGNER_ADDRESS> <CLAIM_ADDRESS>
```

**Example:**

```sh
clementine-cli withdraw scan --network testnet4 wittb1pf... tb1qg...
```

This command will scan Bitcoin network and return possible withdrawal scenarios for appropriate UTXOs, with corresponding prompt to generate the necessary signature.

## Step 4: Generate Withdrawal Signature (Airgapped Device)

**AIRGAPPED DEVICE ONLY:** Create a signature for the withdrawal transaction:

### Prepare Data Transfer

**Transfer from online device to airgapped device:**

- Signer address (with "wit" prefix)
- Withdrawal address (claim address)
- Withdrawal UTXO details (from scan command)
- Amount to withdraw (in BTC)

### Generate Signature

```sh
clementine-cli withdraw generate-withdrawal-signature --network <BITCOIN_NETWORK> <SIGNER_ADDRESS> <WITHDRAWAL_ADDRESS> <WITHDRAWAL_UTXO> <AMOUNT>
```

**Example:**

```sh
clementine-cli withdraw generate-withdrawal-signature --network testnet4 wittb1pf... tb1qg... abc123def456...:0 9.9 BTC
```

> [!CAUTION]
> Save the generated signature since it will be used to authorize the operations that be done later.

## Step 5: Safe Withdraw (Online Device)

**ONLINE DEVICE OPERATION:** Execute the withdrawal with signature verification and submit to Citrea:

```sh
clementine-cli withdraw safe-withdraw --network <BITCOIN_NETWORK> <SIGNER_ADDRESS> <WITHDRAWAL_ADDRESS> <WITHDRAWAL_UTXO> <AMOUNT> <SIGNATURE>
```

**Example:**

```sh
clementine-cli withdraw safe-withdraw --network testnet4 wittb1pf... tb1qg... abc123def456:0 9.9 807c42770...
```

**What this does:**

- Verifies the signature matches the withdrawal parameters
- Provides final transaction confirmation
- Prompts to Metamask to submit the withdrawal transaction to Citrea

**What happens:**

- Verifies the signature from airgapped device
- Submits the withdrawal transaction to Citrea network
- Returns transaction confirmation

## Step 6: Send the Signature to the Operators

**ONLINE DEVICE OPERATION:** Submit the generated signature to bridge operators for final withdrawal processing:

```sh
clementine-cli withdraw send-withdrawal-signatures-to-operators --network <BITCOIN_NETWORK> <SIGNER_ADDRESS> <WITHDRAWAL_ADDRESS> <WITHDRAWAL_UTXO> <AMOUNT> <SIGNATURE> <WITHDRAWAL_INDEX>
```

**Example:**

```sh
clementine-cli withdraw send-withdrawal-signatures-to-operators  --network testnet4 wittb1pf... tb1qg... abc123def456:0 9.9 807c42770... 1
```

> [!NOTE]
> After sending the signature, operators will validate and process the withdrawal. Use Step 7 to monitor the status.

## Step 7: Check Withdrawal Status (Online Device)

**ONLINE DEVICE OPERATION:** Monitor the status of your withdrawal:

```sh
clementine-cli withdraw status --network <BITCOIN_NETWORK> <WITHDRAWAL_INDEX>
```

**Parameters:**

- `WITHDRAWAL_INDEX`: Index number from withdrawal initiation or scan results

**Example:**

```sh
clementine-cli withdraw status --network testnet4 123
```

The status will show the response from the backend.

## Complete Withdrawal Workflow Summary

**Two-Device Process Overview:**

1. **[Both]** `withdraw start` - Initiate withdrawal process
2. **[Online]** `bitcoin-cli sendtoaddress` - Send necessary transaction to signer address
3. **[Online]** `withdraw scan` - Find available UTXOs
4. **[Transfer]** Move UTXO data to airgapped device
5. **[Airgapped]** `generate-withdrawal-signature` - Create signature
6. **[Transfer]** Move signature back to online device
7. **[Online]** `safe-withdraw` - Verify, prepare, and send transaction on Citrea
8. **[Online]** `send-withdrawal-signatures-to-operators` - Submit signature to bridge operators
9. **[Online]** `withdraw status` - Monitor completion

**Critical Security Note:** ALL cryptographic operations (step 5) must be performed on the airgapped device. Network operations and transaction submission occur on the online device.

## Security Considerations

**Critical Security Requirements:**

- **Airgapped Signing**: ALL signature generation must occur on airgapped device
- **Address Verification**: Always verify withdrawal and claim addresses before signing
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
