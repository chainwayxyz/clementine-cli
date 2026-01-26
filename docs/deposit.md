# Deposit Operations

This guide covers the complete process for depositing 10 BTC to Citrea using Clementine CLI. The deposit process involves creating a wallet, generating a deposit address, sending 10 BTC, and monitoring the bridging process.

View all deposit commands:

```sh
clementine-cli deposit --help
```

> [!CAUTION]
> Don't forget to specify the `--network` flag if you plan to use a different Bitcoin
> network other than mainnet. Mainnet is selected implicitly for every
> command.

## Prerequisites

Before starting a deposit, ensure you have:

- A Citrea address
- A Clementine CLI wallet with `deposit` purpose ("dep" prefix address) which will be used as the `recovery taproot address`
- Access to a Bitcoin wallet for sending 10 BTC to the deposit address

## Deposit Process Overview

The deposit process consists of several stages:

1. [**Create Wallet for Deposits**](#step-1-create-a-wallet-for-deposits) - Create a recovery taproot address
2. [**Start Deposit**](#step-2-start-deposit-generate-deposit-address) - Create a unique deposit address using your Citrea (EVM) address and recovery taproot address
3. [**Send 10 BTC**](#step-3-send-10-btc-to-the-deposit-address) - Transfer 10 BTC to the deposit address and rest will be automatically handled by the Clementine, and you will receive your 10 cBTC on your Citrea address.
4. [**Monitor Status**](#step-4-monitor-deposit-status) - Track the bridging process
5. [**Recovery (if needed)**](#step-5-recovery-if-needed) - Recover funds if bridging fails

> [!IMPORTANT]
> The `RECOVERY_TAPROOT_ADDRESS` and the `DEPOSIT_ADDRESS` are different. The `RECOVERY_TAPROOT_ADDRESS` will belong to your Clementine CLI wallet to be able to sign the recovery transaction in case the deposit fails, whereas the `DEPOSIT_ADDRESS` is the address you send Bitcoin to in order to perform the deposit operation. Your `RECOVERY_TAPROOT_ADDRESS` is used alongside the `N_of_N_ADDRESS` when creating the `DEPOSIT_ADDRESS` to make sure if the deposit fails, you can recover your funds back to your `DESTINATION_ADDRESS`.

## Step 1: Start Deposit (Generate Deposit Address)

### Start Deposit

Create a deposit address using your Citrea (EVM) address and recovery taproot address:

```sh
clementine-cli deposit start [--network <BITCOIN_NETWORK>] <RECOVERY_TAPROOT_ADDRESS> <CITREA_ADDRESS>
```

**Example:**

```sh
clementine-cli deposit start depbc1p... 0x742d35...e4C837Be
# For testnet4
clementine-cli deposit start --network testnet depbc1p... 0x742d35...e4C837Be
```

> [!IMPORTANT]
> **About the "dep" prefix:** The recovery taproot address should belong to Clementine wallet with `deposit` purpose and should be prefixed with "dep" to indicate it's being used for deposit operations. This prefix helps distinguish deposit-specific addresses from regular wallet addresses and ensures proper address derivation in the Clementine bridge system.

## Step 2: Send Bitcoin to Deposit Address

Send your Bitcoin to the generated deposit address. You can use any Bitcoin wallet or client. The deposit amount is fixed to `10 BTC`. After sending the funds, rest will be automatically handled by the Clementine, and you will receive your 10 cBTC on your Citrea address.

> [!CAUTION]
> Save your deposit transaction ID immediately. This is required for recovery if the Move to Vault transaction fails after 200 blocks.

## Step 3: Monitor Deposit Status

Track the progress of your deposit throughout the bridging process:

```sh
clementine-cli deposit status [--network <BITCOIN_NETWORK>] <DEPOSIT_ADDRESS>
```

**Example:**

```sh
clementine-cli deposit status tb1pd...
# For testnet4
clementine-cli deposit status --network testnet tb1pd...
```

## Step 4: Fund Recovery (If Needed)

If 200 blocks have passed and the deposit status shows the Move to Vault transaction has not been broadcasted, you can recover your funds using the recovery mechanism. Recovery is done by signing a recovery transaction with your Clementine CLI wallet and broadcasting it to the Bitcoin network.

### Prepare Recovery Data

Gather all necessary information:

- `RECOVERY_TAPROOT_ADDRESS`: Your wallet's recovery address (`depbc1p...`)
- `CITREA_ADDRESS`: Your Citrea address used for the deposit (`0x...`)
- `DEPOSIT_UTXO_OUTPOINT`: Your deposit Outpoint (`txid:vout`)
- `DESTINATION_ADDRESS`: Bitcoin address where recovered funds will be sent

### Create Recovery Transaction

Generate signed recovery transaction:

```sh
clementine-cli deposit create-signed-recovery-tx [--network <BITCOIN_NETWORK>] <RECOVERY_TAPROOT_ADDRESS> <CITREA_ADDRESS> <DEPOSIT_OUTPOINT> <DESTINATION_ADDRESS> <FEE_RATE> <AMOUNT> <CLEMENTINE_AGGREGATED_KEY>
```

**Example:**

```sh
clementine-cli deposit create-signed-recovery-tx deptb1pd... 0x742d35... abc123def456...:0 tb1qe... 1 10 1e0f48f81dfa14...6bf7
# For testnet4
clementine-cli deposit create-signed-recovery-tx --network testnet deptb1pd... 0x742d35... abc123def456...:0 tb1qe... 1 10 1e0f48f81dfa14...6bf7
```

> [!IMPORTANT]
> Please make sure that `AMOUNT` you enter is the exact amount of BTC you sent to the deposit address. This is necessary to recover your funds if you accidentally send any amount other than `10 BTC`.

### Verify Recovery Transaction (Optional)

Verify the recovery transaction details before broadcasting:

```sh
clementine-cli deposit verify-recovery-tx [--network <BITCOIN_NETWORK>] <RECOVERY_TX> <RECOVERY_TAPROOT_ADDRESS> <CITREA_ADDRESS> [AMOUNT] <CLEMENTINE_AGGREGATED_KEY>
```

### Broadcast Recovery Transaction

Send the recovery transaction to the Bitcoin network:

```sh
clementine-cli deposit broadcast-recovery-tx <RECOVERY_TX>
```


### Common Issues

**Deposit address generation fails:**

- Verify Citrea (EVM) address format (`0x...`)
- Check recovery taproot address format (`depbc1p...`)
- Ensure network parameter matches your Bitcoin network

**Status check returns no results:**

- Confirm deposit address is correct
- Wait for Bitcoin network confirmation (usually 1-6 blocks)
- Check if deposit transaction was actually broadcasted

**Recovery transaction creation fails:**

- Verify all parameters match original deposit exactly
- Check that 200 blocks have actually passed
- Ensure recovery taproot address is from the correct wallet
