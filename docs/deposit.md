# Depositing to Citrea

All the available deposit commands can be viewed using `clementine-cli deposit --help`
command and `clementine-cli deposit <sub-command> --help`.

1. Get your deposit address, using your EVM address and recovery taproot address (with dep prefix)

   ```sh
   # Save printed out address
   clementine-cli --network <BITCOIN_NETWORK> deposit get-deposit-address <EVM_ADDRESS> <RECOVERY_TAPROOT_ADDRESS>
   ```

2. Send 10 BTC to the deposit address, using previously generated deposit address.

   > [!WARNING]
   >
   > Save your deposit transaction's TxId as it will be used to create Recovery
   > Tx if your Move To Vault Tx hasn't got broadcasted after 200 blocks!

   ```sh
   # Example using bitcoin-cli
   bitcoin-cli <YOUR-BITCOIN-CLI-FLAGS> sendtoaddress <DEPOSIT_ADDRESS> 10
   ```

3. Wait for Move TX to appear on Bitcoin:

   ```sh
   clementine-cli --network <BITCOIN_NETWORK> deposit status <DEPOSIT ADDRESS>
   ```

If 200 blocks have passed and deposit status still shows Move To Vault TX is not
yet broadcasted, you can recover your funds.

1. Using the previous values:

   ```sh
   clementine-cli --network <BITCOIN_NETWORK> deposit create-signed-recovery-tx <RECOVERY_TAPROOT_ADDRESS> <EVM_ADDRESS> <DEPOSIT_TXID> <DEPOSIT_VOUT> <CLAIM_ADDRESS>
   ```

2. Optionally, verify the transaction details:

   ```sh
   clementine-cli --network <BITCOIN_NETWORK> deposit verify-recovery-tx <RECOVERY_TX> <EVM_ADDRESS> <RECOVERY_TAPROOT_ADDRESS>
   ```

3. Broadcast the raw recovery transaction

   ```sh
   # Example using bitcoin-cli
   bitcoin-cli <YOUR-BITCOIN-CLI-FLAGS> sendrawtransaction <RECOVERY_TX>
   ```

## FAQ

- Why do I need to provide my recovery taproot address?

  User's configuration can be wrong or local Clementine CLI version might be old.
  Both of these affects the end result of generated recovery taproot address. So,
  user is expected to generate that address from latest tools and provide the
  correct value.

- I lost my recovery taproot address and my Citrea address. How can I retrieve
  my funds?

  You can't. That's why you need to be extra careful while saving the details.
