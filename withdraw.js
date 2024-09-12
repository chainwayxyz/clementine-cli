#!/usr/bin/env node

const { Command } = require("commander");
const axios = require("axios");
const { ethers } = require("ethers");
const fs = require("fs");

const program = new Command();

program
  .requiredOption("--bitcoin-rpc-url <url>", "Bitcoin RPC URL")
  .requiredOption("--rpcuser <user>", "Bitcoin RPC user")
  .requiredOption("--rpcpassword <password>", "Bitcoin RPC password")
  .requiredOption(
    "--citrea-rpc-url <url>",
    "Citrea RPC URL",
    "http://127.0.0.1:12345/"
  ) // Set default value
  .option("--citrea-private-key <key>", "Citrea private key")
  .option("--withdrawal-idx <idx>", "Withdrawal index")
  .requiredOption(
    "--withdrawal-sig-endpoints <endpoints>",
    "Withdrawal signature endpoints"
  )
  .requiredOption(
    "--min-withdrawal-amount <amount>",
    "Minimum withdrawal amount"
  )
  .requiredOption("--num-rounds <rounds>", "Number of rounds");

program.parse(process.argv);

program.parse(process.argv);

const options = program.opts();

// Initialize ethers provider and wallet
const provider = new ethers.JsonRpcProvider(options.citreaRpcUrl);
const wallet = new ethers.Wallet(options.citreaPrivateKey, provider);

// Function to make Bitcoin RPC calls
const makeBitcoinRpcCall = async (method, params = []) => {
  try {
    const response = await axios.post(
      options.bitcoinRpcUrl,
      {
        jsonrpc: "2.0",
        id: 1,
        method: method,
        params: params,
      },
      {
        auth: {
          username: options.rpcuser,
          password: options.rpcpassword,
        },
      }
    );

    return response.data.result;
  } catch (error) {
    console.error(`Error in Bitcoin RPC call: ${error.message}`);
    throw error;
  }
};

const printBalances = async () => {
  // Fetch Ethereum balance using the provider and wallet address
  const ethBalance = await provider.getBalance(wallet.address);
  console.log(`ETH Balance: ${ethers.formatEther(ethBalance)} ETH`);

  // Fetch Bitcoin balance
  const btcBalance = await makeBitcoinRpcCall("getbalance");
  console.log(`BTC Balance: ${btcBalance} BTC`);
};

const calculateAndLockUtxo = async (withdrawalAddress = null) => {
  // Step 1: Generate a new address and a withdrawal address
  const address = await makeBitcoinRpcCall("getnewaddress", ["", "bech32m"]);
  if (!withdrawalAddress) {
    withdrawalAddress = await makeBitcoinRpcCall("getnewaddress", [
      "",
      "bech32m",
    ]);
  }

  // Step 2: Create a raw transaction
  const rawtx = await makeBitcoinRpcCall("createrawtransaction", [
    [],
    { [address]: 0.00000546 },
  ]);
  const fundedtx = (
    await makeBitcoinRpcCall("fundrawtransaction", [
      rawtx,
      { changePosition: 1 },
    ])
  ).hex;
  const signedtx = (
    await makeBitcoinRpcCall("signrawtransactionwithwallet", [fundedtx])
  ).hex;

  const txid = (await makeBitcoinRpcCall("decoderawtransaction", [signedtx]))
    .txid;

  const vout = 0;

  // testmempoolaccept

  await makeBitcoinRpcCall("testmempoolaccept", [[signedtx]]);

  // Lock the UTXO
  // await makeBitcoinRpcCall("lockunspent", [false, [{ txid: txid, vout: 0 }]]);

  return { address, withdrawalAddress, txid, vout, signedtx };
};

const createBurnTx = ({ txid, vout }) => {
  console.log("Creating burn transaction...", txid, vout);
  if (!txid) {
    throw new Error("txid is required to create a burn transaction");
  }
  if (vout !== 0) {
    throw new Error("vout must be 0 for the burn transaction");
  }
  const reversed_txid = txid.match(/.{2}/g).reverse().join("");

  const data = new ethers.Interface([
    "function withdraw(bytes32 txId, bytes4 outputId)",
  ]).encodeFunctionData("withdraw", ["0x" + reversed_txid, "0x00000000"]);

  const tx = {
    to: "0x3100000000000000000000000000000000000002",
    value: ethers.parseEther("10"),
    data: data,
  };
  const populatedtx = wallet.populateTransaction(tx);

  return populatedtx;
};

const handleWithdrawal = async () => {
  if (options.withdrawalIdx) {
    // read the withdrawal data from the file
    const withdrawalData = JSON.parse(
      fs.readFileSync(`withdrawal_data_${options.withdrawalIdx}.json`)
    );

    await sendAnyoneCanPaySignatures(withdrawalData);
    return;
  }
  await printBalances();

  // Initial UTXO creation and locking
  const {
    address,
    withdrawalAddress,
    txid,
    vout,
    signedtx: signedRawTx,
  } = await calculateAndLockUtxo();

  const burn_tx = await createBurnTx({ txid, vout });

  console.log("Burn transaction created.");
  console.log("Burn transaction:", burn_tx);
  console.log("Signed raw transaction:", signedRawTx);
  console.log("Withdrawal address:", withdrawalAddress);
  console.log("UTXO locked.");

  // Ask for user approval
  const readline = require("readline").createInterface({
    input: process.stdin,
    output: process.stdout,
  });

  readline.question(
    `This operation will create a dust utxo of 546 sats and burn 10 cBTC and start the auction, you withdrawal details will be saved and you can use it later to continue sending intents. Do you want to proceed? (y/n) `,
    async (answer) => {
      if (answer.toLowerCase() === "y") {
        console.log("Proceeding with the transaction...");

        // Step 3: Send the transaction to the bitcoin network

        await makeBitcoinRpcCall("sendrawtransaction", [signedRawTx]);

        // lock the utxo

        await makeBitcoinRpcCall("lockunspent", [
          false,
          [{ txid: txid, vout: vout }],
        ]);

        // Sign and send the transaction
        const signedTx = await wallet.sendTransaction({
          ...burn_tx,
        });
        const receipt = await signedTx.wait();

        console.log(
          `EVM transaction sent with hash: ${JSON.stringify(receipt)}`
        );

        // get the withdrawal index from logs
        const logs = receipt.logs;
        const logData = logs[0].data;

        console.log("Log data:", logData);

        // extract the 5
        const withdrawal_idx = parseInt(
          logData.slice(logData.length - 128, logData.length - 64),
          16
        );

        const withdrawalData = {
          address,
          withdrawalAddress,
          txid,
          vout,
          withdrawal_idx,
        };

        console.log("Withdrawal data:", withdrawalData);

        // save the withdrawal data to a file withdrawal_data_{withdrawal_idx}.json
        fs.writeFileSync(
          `withdrawal_data_${withdrawal_idx}.json`,
          JSON.stringify(withdrawalData)
        );

        // Proceed with withdrawal rounds
        await sendAnyoneCanPaySignatures(withdrawalData);
      } else {
        console.log("Operation cancelled.");
      }
      readline.close();
    }
  );
};

const sendAnyoneCanPaySignatures = async ({
  address,
  withdrawalAddress,
  txid,
  vout,
  withdrawal_idx,
}) => {
  const operatorsEndpoints = options.withdrawalSigEndpoints.split(",");
  const minWithdrawalAmount = parseFloat(options.minWithdrawalAmount);
  const numRounds = parseInt(options.numRounds, 10);
  let amount = 10;

  for (let i = 0; i < numRounds && amount >= minWithdrawalAmount; i++) {
    try {
      // Step 4: Create a raw transaction to withdraw
      const rawtx = await makeBitcoinRpcCall("createrawtransaction", [
        [{ txid, vout: 0 }],
        { [withdrawalAddress]: amount },
      ]);
      const signedtxrequest = (
        await makeBitcoinRpcCall("signrawtransactionwithwallet", [
          rawtx,
          [],
          "SINGLE|ANYONECANPAY",
        ])
      );
      if (!signedtxrequest.complete) {
        if (signedtxrequest.errors[0].error === "Input not found or already spent") {
          console.log("Input not found or already spent, Withdrawal completed. Exiting...");
          return;
        }
        throw new Error("Bitcoin Transaction signing failed");
      }
      const signedtx = signedtxrequest.hex;
      // Extract the txinwitness
      const txinwitness = (
        await makeBitcoinRpcCall("decoderawtransaction", [signedtx])
      ).vin[0].txinwitness[0].slice(0, -2);
      const inputAddressScriptPubKey = (
        await makeBitcoinRpcCall("getaddressinfo", [address])
      ).scriptPubKey;
      const withdrawalAddressScriptPubKey = (
        await makeBitcoinRpcCall("getaddressinfo", [withdrawalAddress])
      ).scriptPubKey;

      const payload = {
        idx: withdrawal_idx,
        user_sig: txinwitness,
        input_utxo: {
          outpoint: `${txid}:0`,
          txout: {
            script_pubkey: inputAddressScriptPubKey,
            value: 546,
          },
        },
        output_txout: {
          script_pubkey: withdrawalAddressScriptPubKey,
          value:  parseInt(amount * 1e8),
        },
      };

      // // Send payload to withdrawal signature endpoints
      // for (const endpoint of options.withdrawalSigEndpoints.split(",")) {
      //   console.log(`Sending payload to endpoint: ${endpoint}`);
      //   console.log("Payload:", payload);
      //   await axios.post(endpoint, payload);
      // }
      // make all the requests in parallel
      // await Promise.all(
      //   operatorsEndpoints.map((endpoint) => {
      //     console.log(`Sending payload to endpoint: ${endpoint}`);
      //     return axios.post(endpoint, payload);
      //   })
      // );

      // make that at least one of the requests is successful
      let success = false;
      let paymentTxid = null;
      await Promise.all(
        operatorsEndpoints.map((endpoint) => {
          console.log(`Sending payload to endpoint: ${endpoint}, payload:`, payload);
          return axios
            .post(endpoint, payload)
            .then((response) => {
              console.log("Response:", response);
              if (response.status === 200) {
                success = true;
                paymentTxid = response.data;
              }
            })
            .catch((error) => {
              console.error(`Error in endpoint ${endpoint}:`, error.message);
            });
        })
      );
      if (success) {
        console.log(
          `Round ${
            i + 1
          }: Withdrawal of ${amount} BTC processed. Payment txid: ${JSON.stringify(paymentTxid)}`
        );
        return;
      }

      const newAmount = amount - (10 - minWithdrawalAmount) / numRounds;
      console.log(
        `Round ${
          i + 1
        }: Withdrawal of ${amount} BTC failed, retrying with amount ${newAmount} BTC`
      );
      amount = newAmount;
    } catch (error) {
      console.error(`Error in round ${i + 1}:`, error.message);
    }
  }
};

handleWithdrawal();
