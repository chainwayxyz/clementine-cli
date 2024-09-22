#!/usr/bin/env node

const { Command } = require("commander");
const axios = require("axios");
const { ethers } = require("ethers");
const fs = require("fs");
const path = require("path");
const crypto = require("crypto");
const { default: bs58 } = require("bs58");

const program = new Command();

let config = {
  bitcoinRpcUrl: "",
  bitcoinRpcUser: "",
  bitcoinRpcPassword: "",
  citreaRpcUrl: "https://rpc.testnet.citrea.xyz/",
  citreaPrivateKey: "",
  operatorEndpoints: [""],
};

const configFile = path.join(__dirname, "config.json");
if (fs.existsSync(configFile)) {
  const fileConfig = JSON.parse(fs.readFileSync(configFile, "utf-8"));
  config = { ...config, ...fileConfig };
}

// Command-line options for configuration
program
  .option("--bitcoin-rpc-url <url>", "Bitcoin RPC URL", config.bitcoinRpcUrl)
  .option(
    "--bitcoin-rpc-user <user>",
    "Bitcoin RPC User",
    config.bitcoinRpcUser
  )
  .option(
    "--bitcoin-rpc-password <password>",
    "Bitcoin RPC Password",
    config.bitcoinRpcPassword
  )
  .option("--citrea-rpc-url <url>", "Citrea RPC URL", config.citreaRpcUrl)
  .option(
    "--citrea-private-key <key>",
    "Citrea Private Key",
    config.citreaPrivateKey
  );

// Function to convert a private key to WIF, ref: https://en.bitcoin.it/wiki/Wallet_import_format
function privateKeyToWIF(privateKeyHex, compressed = true, testnet = true) {
  // Convert the hex string to a Buffer
  let privateKeyBuffer = Buffer.from(privateKeyHex, "hex");

  // Add the 0x80 prefix for mainnet or 0xef for testnet
  const prefix = testnet ? Buffer.from([0xef]) : Buffer.from([0x80]);
  let extendedKey = Buffer.concat([prefix, privateKeyBuffer]);

  // Add the 0x01 suffix if the key will correspond to a compressed public key
  if (compressed) {
    extendedKey = Buffer.concat([extendedKey, Buffer.from([0x01])]);
  }

  // Perform SHA-256 hash twice
  const firstSHA = crypto.createHash("sha256").update(extendedKey).digest();
  const secondSHA = crypto.createHash("sha256").update(firstSHA).digest();

  // Take the first 4 bytes of the second SHA-256 hash for the checksum
  /** @type {Buffer} */
  const checksum = secondSHA.subarray(0, 4);

  // Add the checksum to the end of the extended key
  const finalKey = Buffer.concat([extendedKey, checksum]);

  // Convert to Base58Check encoding
  const WIF = bs58.encode(finalKey);

  return WIF;
}

// Function to create a random 32-byte private key
function createRandomPrivKey() {
  // Generate a random 32-byte Buffer
  const randomKey = crypto.randomBytes(32);

  // Convert the Buffer to a hex string
  const privateKeyHex = randomKey.toString("hex");

  return privateKeyHex;
}

// Function to make Bitcoin RPC calls
const makeBitcoinRpcCall = async (options, method, params = []) => {
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
          username: options.bitcoinRpcUser,
          password: options.bitcoinRpcPassword,
        },
      }
    );

    return response.data.result;
  } catch (error) {
    console.error(`Error in Bitcoin RPC call: ${error.message}`);
    console.error("Response data:", options);
    throw error;
  }
};

const createDustUtxo = async (options) => {
  // Step 1: Generate a new private key
  const privateKey = createRandomPrivKey();
  // Convert the private key to WIF format
  const WIF = privateKeyToWIF(privateKey, true, true);
  // Call getdescriptorinfo to get the address
  const descriptor = `tr(${WIF})`;

  const descriptor_results = await makeBitcoinRpcCall(
    options,
    "getdescriptorinfo",
    [descriptor]
  );
  const addressArray = await makeBitcoinRpcCall(options, "deriveaddresses", [
    descriptor_results.descriptor,
  ]);
  const address = addressArray[0];
  // console.log("Address:", address);
  // save the private key to a file with the name of the address.json
  fs.writeFileSync(`${address}.json`, JSON.stringify({ descriptor, address }));
  // Step 2: fund the address
  const rawtx = await makeBitcoinRpcCall(options, "createrawtransaction", [
    [],
    { [address]: 0.00000546 },
  ]);
  const fundedtx = (
    await makeBitcoinRpcCall(options, "fundrawtransaction", [
      rawtx,
      { changePosition: 1 },
    ])
  ).hex;
  const signedtx = (
    await makeBitcoinRpcCall(options, "signrawtransactionwithwallet", [
      fundedtx,
    ])
  ).hex;

  const txid = (
    await makeBitcoinRpcCall(options, "decoderawtransaction", [signedtx])
  ).txid;

  const vout = 0;

  // testmempoolaccept

  await makeBitcoinRpcCall(options, "testmempoolaccept", [[signedtx]]);

  await makeBitcoinRpcCall(options, "sendrawtransaction", [signedtx]);
  // update the file with the txid and vout
  fs.writeFileSync(
    `${address}.json`,
    JSON.stringify({ descriptor, address, txid, vout })
  );

  // log the file name
  console.log(`Dust UTXO created for address: ${address}`);
  console.log(`Use the file ${address}.json to access the UTXO details`);
  return { address, txid, vout, signedtx };
};

const createBurnTx = (wallet, { txid, vout }) => {
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
  console.log("Transaction:", tx);
  const populatedtx = wallet.populateTransaction(tx);

  return populatedtx;
};

const sendAnyoneCanPaySignatures = async (
  options,
  { dustUtxoDetails, withdrawalAddress, amounts }
) => {
  amounts = JSON.parse(amounts);
  const operatorEndpoints = options.operatorEndpoints;
  // for each amount
  for (const amount of amounts) {
    try {
      // Step 4: Create a raw transaction to withdraw
      const rawtx = await makeBitcoinRpcCall(options, "createpsbt", [
        [{ txid: dustUtxoDetails.txid, vout: dustUtxoDetails.vout }],
        { [withdrawalAddress]: amount },
      ]);
      const signedtxrequest = await makeBitcoinRpcCall(
        options,
        "descriptorprocesspsbt",
        [rawtx, [dustUtxoDetails.descriptor], "SINGLE|ANYONECANPAY"]
      );
      if (!signedtxrequest.complete) {
        if (
          signedtxrequest.errors[0].error === "Input not found or already spent"
        ) {
          console.log(
            "Input not found or already spent, Withdrawal completed. Exiting..."
          );
          return;
        }
        throw new Error("Bitcoin Transaction signing failed");
      }
      const signedtx = signedtxrequest.hex;
      // Extract the txinwitness
      const txinwitness = (
        await makeBitcoinRpcCall(options, "decoderawtransaction", [signedtx])
      ).vin[0].txinwitness[0].slice(0, -2);
      const inputAddressScriptPubKey = (
        await makeBitcoinRpcCall(options, "getaddressinfo", [dustUtxoDetails.address])
      ).scriptPubKey;
      const withdrawalAddressScriptPubKey = (
        await makeBitcoinRpcCall(options, "getaddressinfo", [withdrawalAddress])
      ).scriptPubKey;

      const payload = {
        idx: dustUtxoDetails.withdrawal_idx,
        user_sig: txinwitness,
        input_utxo: {
          outpoint: `${dustUtxoDetails.txid}:0`,
          txout: {
            script_pubkey: inputAddressScriptPubKey,
            value: 546,
          },
        },
        output_txout: {
          script_pubkey: withdrawalAddressScriptPubKey,
          value: parseInt(amount * 1e8),
        },
      };

      // make that at least one of the requests is successful
      let success = false;
      let paymentTxid = null;
      await Promise.all(
        operatorEndpoints.map((endpoint) => {
          console.log(
            `Sending payload to endpoint: ${endpoint}, payload:`,
            payload
          );
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
          `Withdrawal of ${amount} BTC processed. Payment txid: ${JSON.stringify(
            paymentTxid
          )}`
        );
        return;
      }
      console.log(`Withdrawal of ${amount} BTC failed`);
    } catch (error) {
      console.error(`Error in round`, error.message);
      throw error;
    }
  }
};

program
  .command("get-citrea-balance")
  .description("Get Citrea balance")
  .action(() => {
    // Ensure required options are provided
    if (!program.opts().citreaRpcUrl || !program.opts().citreaPrivateKey) {
      console.error(
        "Error: --citrea-rpc-url and --citrea-private-key are required for this command."
      );
      process.exit(1);
    }
    // Method logic here
  });

program
  .command("get-bitcoin-balance")
  .description("Get Bitcoin balance")
  .action(() => {
    // Ensure required options are provided
    if (
      !program.opts().bitcoinRpcUrl ||
      !program.opts().bitcoinRpcUser ||
      !program.opts().bitcoinRpcPassword
    ) {
      console.error(
        "Error: --bitcoin-rpc-url, --bitcoin-rpc-user, and --bitcoin-rpc-password are required for this command."
      );
      process.exit(1);
    }
    // Method logic here
  });

program
  .command("createdustutxo <pay_from_wallet> [path_to_save]")
  .description("Create dust UTXO")
  .option("-p, --pay-from-wallet", "Pay from wallet", false)
  .option("-s, --path-to-save", "Path to save", "./")
  .action((payFromWallet, pathToSave = "./") => {
    // Ensure required options are provided
    if (
      !program.opts().bitcoinRpcUrl ||
      !program.opts().bitcoinRpcUser ||
      !program.opts().bitcoinRpcPassword
    ) {
      console.error(
        "Error: --bitcoin-rpc-url, --bitcoin-rpc-user, and --bitcoin-rpc-password are required for this command."
      );
      process.exit(1);
    }

    console.log("Creating dust UTXO...");
    console.log("Pay from wallet:", program.opts());
    createDustUtxo(program.opts());
    // Method logic here
  });

program
  .command("burn10cbtc <dust_utxo_details_file_path>")
  .description("Burn 10 cBTC")
  .action(async (dustUtxoDetailsFilePath) => {
    // Ensure required options are provided
    if (!program.opts().citreaRpcUrl || !program.opts().citreaPrivateKey) {
      console.error(
        "Error: --citrea-rpc-url and --citrea-private-key are required for this command."
      );
      process.exit(1);
    }

    // read the dust utxo details from the file
    const dustUtxoDetails = JSON.parse(
      fs.readFileSync(dustUtxoDetailsFilePath, "utf-8")
    );

    // if file has receipt, then the transaction is already sent
    if (dustUtxoDetails.receipt) {
      console.log("Transaction already sent.");
      return;
    }

    const provider = new ethers.JsonRpcProvider(program.opts().citreaRpcUrl);
    const wallet = new ethers.Wallet(program.opts().citreaPrivateKey, provider);

    console.log("Creating burn transaction...");
    console.log("Dust UTXO details:", dustUtxoDetails);
    const tx = await createBurnTx(wallet, dustUtxoDetails);

    // Sign and send the transaction
    const signedTx = await wallet.sendTransaction(tx);
    console.log("Transaction sent:", signedTx);
    const receipt = await signedTx.wait();

    const logs = receipt.logs;
    const logData = logs[0].data;

    console.log("Log data:", logData);

    // extract the 5
    const withdrawal_idx = parseInt(
      logData.slice(logData.length - 128, logData.length - 64),
      16
    );

    // update the file with receipt
    fs.writeFileSync(
      dustUtxoDetailsFilePath,
      JSON.stringify({ ...dustUtxoDetails, withdrawal_idx, receipt })
    );
  });

program
  .command(
    "sendwithdrawalsignatures <dust_utxo_details_file_path> <withdrawal_address> <amounts...>"
  )
  .description("Send withdrawal signatures")
  .action(async (dustUtxoDetailsFilePath, withdrawalAddress, amounts) => {
    // Ensure required options are provided
    if (
      !program.opts().bitcoinRpcUrl ||
      !program.opts().citreaRpcUrl ||
      !program.opts().citreaPrivateKey
    ) {
      console.error(
        "Error: --bitcoin-rpc-url, --citrea-rpc-url, and --citrea-private-key are required for this command."
      );
      process.exit(1);
    }

    console.log("options:", { ...config, ...program.opts() });

    // read the dust utxo details from the file
    const dustUtxoDetails = JSON.parse(
      fs.readFileSync(dustUtxoDetailsFilePath, "utf-8")
    );

    await sendAnyoneCanPaySignatures(
      { ...config, ...program.opts() },
      {
        dustUtxoDetails,
        withdrawalAddress,
        amounts,
      }
    );
  });

program
  .command(
    "autowithdraw <withdrawal_address> <min_amount> <num_iteration> <dust_utxo_details_file_path>"
  )
  .description("Automate withdrawal process")
  .action(
    (withdrawalAddress, amounts) => {
      // Ensure required options are provided
      if (
        !program.opts().bitcoinRpcUrl ||
        !program.opts().bitcoinRpcUser ||
        !program.opts().bitcoinRpcPassword ||
        !program.opts().citreaRpcUrl ||
        !program.opts().citreaPrivateKey
      ) {
        console.error(
          "Error: --bitcoin-rpc-url, --bitcoin-rpc-user, --bitcoin-rpc-password, --citrea-rpc-url, and --citrea-private-key are required for this command."
        );
        process.exit(1);
      }
      // Method logic here
    }
  );

program.parse(process.argv);

// const printBalances = async () => {
//   // Fetch Ethereum balance using the provider and wallet address
//   const ethBalance = await provider.getBalance(wallet.address);
//   console.log(`ETH Balance: ${ethers.formatEther(ethBalance)} ETH`);

//   // Fetch Bitcoin balance
//   const btcBalance = await makeBitcoinRpcCall("getbalance");
//   console.log(`BTC Balance: ${btcBalance} BTC`);
// };

// const calculateAndLockUtxo = async (withdrawalAddress = null) => {
//   // Step 1: Generate a new address and a withdrawal address
//   const address = await makeBitcoinRpcCall("getnewaddress", ["", "bech32m"]);
//   if (!withdrawalAddress) {
//     withdrawalAddress = await makeBitcoinRpcCall("getnewaddress", [
//       "",
//       "bech32m",
//     ]);
//   }

//   // Step 2: Create a raw transaction
//   const rawtx = await makeBitcoinRpcCall("createrawtransaction", [
//     [],
//     { [address]: 0.00000546 },
//   ]);
//   const fundedtx = (
//     await makeBitcoinRpcCall("fundrawtransaction", [
//       rawtx,
//       { changePosition: 1 },
//     ])
//   ).hex;
//   const signedtx = (
//     await makeBitcoinRpcCall("signrawtransactionwithwallet", [fundedtx])
//   ).hex;

//   const txid = (await makeBitcoinRpcCall("decoderawtransaction", [signedtx]))
//     .txid;

//   const vout = 0;

//   // testmempoolaccept

//   await makeBitcoinRpcCall("testmempoolaccept", [[signedtx]]);

//   // Lock the UTXO
//   // await makeBitcoinRpcCall("lockunspent", [false, [{ txid: txid, vout: 0 }]]);

//   return { address, withdrawalAddress, txid, vout, signedtx };
// };

// const handleWithdrawal = async () => {
//   if (options.withdrawalIdx) {
//     // read the withdrawal data from the file
//     const withdrawalData = JSON.parse(
//       fs.readFileSync(`withdrawal_data_${options.withdrawalIdx}.json`)
//     );

//     await sendAnyoneCanPaySignatures(withdrawalData);
//     return;
//   }
//   await printBalances();

//   // Initial UTXO creation and locking
//   const {
//     address,
//     withdrawalAddress,
//     txid,
//     vout,
//     signedtx: signedRawTx,
//   } = await calculateAndLockUtxo();

//   const burn_tx = await createBurnTx({ txid, vout });

//   console.log("Burn transaction created.");
//   console.log("Burn transaction:", burn_tx);
//   console.log("Signed raw transaction:", signedRawTx);
//   console.log("Withdrawal address:", withdrawalAddress);
//   console.log("UTXO locked.");

//   // Ask for user approval
//   const readline = require("readline").createInterface({
//     input: process.stdin,
//     output: process.stdout,
//   });

//   readline.question(
//     `This operation will create a dust utxo of 546 sats and burn 10 cBTC and start the auction, you withdrawal details will be saved and you can use it later to continue sending intents. Do you want to proceed? (y/n) `,
//     async (answer) => {
//       if (answer.toLowerCase() === "y") {
//         console.log("Proceeding with the transaction...");

//         // Step 3: Send the transaction to the bitcoin network

//         await makeBitcoinRpcCall("sendrawtransaction", [signedRawTx]);

//         // lock the utxo

//         await makeBitcoinRpcCall("lockunspent", [
//           false,
//           [{ txid: txid, vout: vout }],
//         ]);

//         // Sign and send the transaction
//         const signedTx = await wallet.sendTransaction({
//           ...burn_tx,
//         });
//         const receipt = await signedTx.wait();

//         console.log(
//           `EVM transaction sent with hash: ${JSON.stringify(receipt)}`
//         );

//         // get the withdrawal index from logs
//         const logs = receipt.logs;
//         const logData = logs[0].data;

//         console.log("Log data:", logData);

//         // extract the 5
//         const withdrawal_idx = parseInt(
//           logData.slice(logData.length - 128, logData.length - 64),
//           16
//         );

//         const withdrawalData = {
//           address,
//           withdrawalAddress,
//           txid,
//           vout,
//           withdrawal_idx,
//         };

//         console.log("Withdrawal data:", withdrawalData);

//         // save the withdrawal data to a file withdrawal_data_{withdrawal_idx}.json
//         fs.writeFileSync(
//           `withdrawal_data_${withdrawal_idx}.json`,
//           JSON.stringify(withdrawalData)
//         );

//         // Proceed with withdrawal rounds
//         await sendAnyoneCanPaySignatures(withdrawalData);
//       } else {
//         console.log("Operation cancelled.");
//       }
//       readline.close();
//     }
//   );
// };

// handleWithdrawal();
