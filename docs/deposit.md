# Deposit Operations

This guide covers the complete process for depositing Bitcoin to Citrea using Clementine CLI. The deposit process involves generating a deposit address, sending Bitcoin, and monitoring the bridging process.

View all deposit commands:

```sh
clementine-cli deposit --help
```

## Prerequisites

Before starting a deposit, ensure you have:

- A Citrea address (Ethereum format)
- A Clementine wallet with `deposit` purpose ("dep" prefix address) which will be used as the recovery taproot address
- Access to a Bitcoin wallet or node for sending funds
- Sufficient Bitcoin for the deposit

## Deposit Process Overview

The deposit process consists of several stages:

1. **Generate deposit address** - Create a unique deposit address
2. **Send Bitcoin** - Transfer funds to the deposit address
3. **Monitor status** - Track the bridging process
4. **Recovery (if needed)** - Recover funds if bridging fails

> [!IMPORTANT]
> The `RECOVERY_TAPROOT_ADDRESS` and the `DEPOSIT_ADDRESS` are different. The `RECOVERY_TAPROOT_ADDRESS` will belong to your Clementine wallet to be able to perform deposit specific signing operations in case the deposit fails, whereas `DEPOSIT_ADDRESS` is the address that the deposited BTC funds are sent to. Your `RECOVERY_TAPROOT_ADDRESS` is used when creating the `DEPOSIT_ADDRESS` to make sure if the deposit fails, you can recover your funds back to your `DESTIONATION_ADDRESS`.

## Step 1: Generate Deposit Address

**TWO-DEVICE PROCESS:**

### Online Device: Generate Deposit Address

Create a deposit address using your Citrea (EVM) address and recovery taproot address:

```sh
clementine-cli deposit get-deposit-address --network <BITCOIN_NETWORK> <RECOVERY_TAPROOT_ADDRESS> <CITREA_ADDRESS>
```

**Example:**

```sh
clementine-cli deposit get-deposit-address --network testnet4 depbc1p... 0x742d35Cc6631C0532925a3b8D0dE4E8de4C837Be
```

> [!IMPORTANT]
> **About the "dep" prefix:** The recovery taproot address should belong to Clementine wallet with `deposit` purpose and should be prefixed with "dep" to indicate it's being used for deposit operations. This prefix helps distinguish deposit-specific addresses from regular wallet addresses and ensures proper address derivation in the Clementine bridge system.

### Airgapped Device: Verify Deposit Address

**CRITICAL VERIFICATION STEP:** Transfer the generated deposit address to your airgapped device and verify it:

```sh
# Run this on airgapped device to verify the deposit address matches
clementine-cli deposit get-deposit-address --network <BITCOIN_NETWORK> <RECOVERY_TAPROOT_ADDRESS> <EVM_ADDRESS>
```

## Step 2: Send Bitcoin to Deposit Address

Send your Bitcoin to the generated deposit address. You can use any Bitcoin wallet or client. The deposit amount is fixed to `10 BTC`.

**Using bitcoin-cli:**

```sh
bitcoin-cli -<NETWORK> sendtoaddress <DEPOSIT_ADDRESS> 10
```

**Example:**

```sh
bitcoin-cli -testnet4 sendtoaddress "tb1pd..." 10
```

> [!CAUTION]
> Save your deposit transaction ID immediately. This is required for recovery if the Move to Vault transaction fails after 200 blocks.

## Step 3: Monitor Deposit Status

**ONLINE DEVICE OPERATION:** Track the progress of your deposit through the bridging process:

```sh
clementine-cli deposit status --network <BITCOIN_NETWORK> <DEPOSIT_ADDRESS>
```

**Example:**

```sh
clementine-cli --network testnet4 deposit status tb1pd...
```

The status will show the response from the backend.

**Two-Device Monitoring Protocol:**

1. **Online Device**: Run status checks regularly
2. **Document Status**: Record all status changes with timestamps
3. **Critical Threshold**: Monitor closely around 200-block threshold
4. **Transfer Info**: If recovery needed, prepare data for airgapped device

## Step 4: Fund Recovery (If Needed)

**TWO-DEVICE RECOVERY PROCESS:** If 200 blocks have passed and the deposit status shows the Move to Vault transaction has not been broadcasted, you can recover your funds using the recovery mechanism.

### Prepare Recovery Data (Online Device)

Gather all necessary information on your online device:

- `RECOVERY_TAPROOT_ADDRESS`: Your wallet's recovery address (from airgapped device)
- `EVM_ADDRESS`: Your Citrea address used for the deposit
- `DEPOSIT_UTXO_OUTPOINT`: Your deposit Outpoint (`txid:vout`)
- `DESTIONATION_ADDRESS`: Bitcoin address where recovered funds will be sent

### Create Recovery Transaction (Airgapped Device)

**AIRGAPPED DEVICE ONLY:** Transfer recovery data to airgapped device and generate signed recovery transaction:

```sh
clementine-cli deposit create-signed-recovery-tx --network <BITCOIN_NETWORK> <RECOVERY_TAPROOT_ADDRESS> <CITREA_ADDRESS> <DEPOSIT_OUTPOINT> <DESTINATION_ADDRESS> <FEE_RATE> <AMOUNT>
```

**Example:**

```sh
clementine-cli deposit create-signed-recovery-tx --network testnet4 deptb1pd... 0x742d35... abc123def456...:0 tb1qe... 1 10
```

> [!IMPORTANT]
> Please make sure that `amount` you enter is the exact amount of BTC you sent to the deposit address. This is necessary to recover your funds if you accidentally send any amount other than `10 BTC`.

**Airgapped Recovery Protocol:**

1. **Transfer Data**: Move all recovery parameters to airgapped device via secure method
2. **Generate TX**: Run create-signed-recovery-tx command on airgapped device
3. **Verify Details**: Double-check all parameters before execution
4. **Transfer Output**: Move signed recovery transaction back to online device

### Verify Recovery Transaction (Optional)

**BOTH DEVICES:** Verify the recovery transaction details before broadcasting:

**Airgapped Device (Generate Verification):**

```sh
clementine-cli deposit verify-recovery-tx --network <BITCOIN_NETWORK> <RECOVERY_TX> <RECOVERY_TAPROOT_ADDRESS> <CITREA_ADDRESS> [AMOUNT]
```

### Broadcast Recovery Transaction (Online Device)

**ONLINE DEVICE ONLY:** Send the recovery transaction to the Bitcoin network:

```sh
clementine-cli deposit broadcast-recovery-tx --network <BITCOIN_NETWORK> <RECOVERY_TX>
```

> [!TIP]
> You don't have to use `clementine-cli` if you know how to send raw transactions
> by yourself. `clementine-cli` only provides helping wrappers around Mempool
> post tx api and Bitcoin CLI.

**Broadcasting Protocol:**

1. **Receive**: Get signed recovery transaction from airgapped device
2. **Final Verification**: Optionally run verify-recovery-tx on online device
3. **Broadcast**: Submit transaction to Bitcoin network
4. **Monitor**: Track transaction confirmation

## Additional Commands

### Get Deposit Parameters

Retrieve deposit parameters for advanced operations:

```sh
clementine-cli deposit get-deposit-params --network <NETWORK> <MOVE_TO_VAULT_TXID>
```

## Troubleshooting

### Common Issues

**Deposit address generation fails:**

- Verify EVM address format (0x...)
- Check recovery taproot address format (bc1p...)
- Ensure network parameter matches your Bitcoin network

**Status check returns no results:**

- Confirm deposit address is correct
- Wait for Bitcoin network confirmation (usually 1-6 blocks)
- Check if deposit transaction was actually broadcasted

**Recovery transaction creation fails:**

- Verify all parameters match original deposit exactly
- Check that 200 blocks have actually passed
- Ensure recovery taproot address is from the correct wallet

### Getting Help

For command-specific help:

```sh
clementine-cli deposit --help
clementine-cli deposit <subcommand> --help
```

## FAQ

### Why do I need to provide my recovery taproot address?

User configurations may be incorrect or the local Clementine CLI version might be outdated. Both of these factors affect the generated recovery taproot address. Users are expected to generate this address using the latest tools and provide the correct value.

### I lost my recovery taproot address and Citrea address. How can I retrieve my funds?

Unfortunately, fund recovery is not possible without these critical pieces of information. This is why it's essential to securely backup all deposit details before sending funds.

### How long should I wait before initiating recovery?

If the deposit is processed, then there is nothing to worry about. Otherwise, wait for at least 200 Bitcoin blocks (approximately 33 hours) after your deposit transaction is confirmed. Monitor the status regularly during this period.

### Can I speed up the bridging process?

The bridging process is automated and controlled by the Clementine protocol entities. While users cannot directly accelerate it, monitoring ensures that any necessary recovery actions can be taken promptly.
