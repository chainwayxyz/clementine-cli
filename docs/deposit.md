# Depositing to Citrea

All the available deposit commands can be viewed using `clementine-cli deposit --help`
command and `clementine-cli deposit <sub-command> --help`.

1. Get your deposit address, using your EVM address and recovery taproot address (with dep prefix)

   ```sh
   # Save printed out address
   clementine-cli --network <BITCOIN NETWORK> deposit get-deposit-address <CITREA EVM ADDRESS> <RECOVERY TAPROOT ADDRESS>
   ```

2. Send 10 BTC to the deposit address, using previously generated deposit address

3. Wait for Move TX to appear on Bitcoin

   ```sh
   clementine-cli --network <BITCOIN NETWORK> deposit deposit-status <DEPOSIT ADDRESS>
   ```

## FAQ

- Why do I need to provide my recovery taproot address?

  User's configuration can be wrong or local Clementine CLI version might be old.
  Both of these affects the end result of generated recovery taproot address. So,
  user is expected to generate that address from latest tools and provide the
  correct value.
