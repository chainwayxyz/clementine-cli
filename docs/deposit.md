# Depositing to Citrea

All the available deposit commands can be viewed using `clementine deposit --help`
command and `clementine deposit <sub-command> --help`.

1. Get your deposit address, using your EVM address and recovery taproot address

   ```sh
   # Save printed out address
   clementine --config-file <CONFIG FILE> deposit get-deposit-address <CITREA EVM ADDRESS> <RECOVERY TAPROOT ADDRESS>
   ```

2. Send 10 BTC to the deposit address, using previously generated deposit address

   ```sh
   # Save deposit tx's TxId and vout 
   clementine --config-file <CONFIG FILE> deposit send-deposit-transaction <DEPOSIT ADDRESS>
   ```

3. Wait for Move TX to appear on Bitcoin -- TODO: havent checked it yet

   ```sh
   clementine --config-file <CONFIG FILE> deposit deposit-status <DEPOSIT ADDRESS>
   ```

4. TODO: Should we add steps if things go wrong (whitepaper page 25 section peg in)?

## FAQ

- Why do I need to provide my recovery taproot address?

  User's configuration can be wrong or local Clementine CLI version might be old.
  Both of these affects the end result of generated recovery taproot address. So,
  user is expected to generate that address from latest tools and provide the
  correct value.
