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
  operatorEndpoints: ["https://api.testnet.citrea.xyz/withdrawals"],
  citreaExplorerUrl: "https://explorer.testnet.citrea.xyz/",
  bitcoinExplorerUrl: "https://mempool.space/testnet4/",
};

const configFilePath = path.join(__dirname, "config.json");
if (fs.existsSync(configFilePath)) {
  const fileConfig = JSON.parse(fs.readFileSync(configFilePath, "utf-8"));
  config = { ...config, ...fileConfig };
}

// Command-line options for configuration
program
  .option("--bitcoin-rpc-url <url>", "Bitcoin RPC URL")
  .option("--bitcoin-rpc-user <user>", "Bitcoin RPC User")
  .option("--bitcoin-rpc-password <password>", "Bitcoin RPC Password")
  .option("--citrea-rpc-url <url>", "Citrea RPC URL")
  .option("--citrea-private-key <key>", "Citrea Private Key");

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
    throw error;
  }
};

const createDustUtxoFromWallet = async (options, address) => {
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

  await makeBitcoinRpcCall(options, "testmempoolaccept", [[signedtx]]);

  await makeBitcoinRpcCall(options, "sendrawtransaction", [signedtx]);

  return { txid, vout };
};

const createBurnTx = (wallet, { txid, vout }) => {
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

const sendAnyoneCanPaySignature = async (
  options,
  { dustUtxoDetails, withdrawalAddress, amount }
) => {
  const operatorEndpoints = options.operatorEndpoints;

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
    await makeBitcoinRpcCall(options, "getaddressinfo", [
      dustUtxoDetails.address,
    ])
  ).scriptPubKey;
  const withdrawalAddressScriptPubKey = (
    await makeBitcoinRpcCall(options, "getaddressinfo", [withdrawalAddress])
  ).scriptPubKey;

  const payload = {
    idx: dustUtxoDetails.withdrawalIdx,
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

  // console.log("Payload:", payload);
  // make that at least one of the requests is successful
  let success = false;
  let paymentTxid = null;
  await Promise.all(
    operatorEndpoints.map((endpoint) => {
      return axios
        .post(endpoint, payload)
        .then((response) => {
          // console.log("Response:", response);
          if (response.status === 200) {
            success = true;
            paymentTxid = response.data;
          }
        })
        .catch((error) => {
          const errMsg =
            error.response.data?.error || error?.message || "Unknown error";
          throw new Error(errMsg);
        });
    })
  );
  if (success) {
    return paymentTxid;
  }
};

const withdraw = async (
  config,
  backupFilePath,
  withdrawalAddress,
  minWithdrawalAmount,
  precision
) => {
  let backupData;
  try {
    backupData = JSON.parse(fs.readFileSync(backupFilePath, "utf-8"));
  } catch (error) {
    throw new Error(`Error reading backup data from file: ${error.message}`);
  }
  // console.log("Backup data:", backupData);

  if (!backupData.descriptor || !backupData.address) {
    throw new Error("Invalid backup data.");
  }

  if (!backupData.txid) {
    try {
      console.log("Creating dust UTXO...");
      const { txid, vout } = await createDustUtxoFromWallet(
        config,
        backupData.address
      );
      backupData = { ...backupData, txid, vout };
      fs.writeFileSync(backupFilePath, JSON.stringify(backupData));
    } catch (error) {
      console.error(`\nError creating dust UTXO`);
      console.error(`You need to have some funds in your wallet.`);
      try {
        const balance = await makeBitcoinRpcCall(config, "getbalance");
        console.log(`\nYour wallet balance: ${balance}\n`);
      } catch (error) {
        console.error(`\nError getting wallet balance: ${error.message}\n`);
      }
      throw new Error(`Error creating dust UTXO: ${error.message}`);
    }
  }

  let outspend;
  try {
    outspend = await axios.get(
      `${config.bitcoinExplorerUrl}api/tx/${backupData.txid}/outspend/${backupData.vout}`
    );
    outspend = outspend.data;
  } catch (error) {
    throw new Error(`Error getting outspend data: ${error.message}`);
  }

  if (outspend.txid) {
    console.log("Withdrawal completed.");
    console.log(
      `You can view the transaction on Bitcoin Explorer: ${config.bitcoinExplorerUrl}tx/${outspend.txid}`
    );
    return;
  }

  if (!backupData.burnTxHash) {
    console.log("Withdrawing 10 cBTC...");

    let wallet;
    let provider;
    try {
      provider = new ethers.JsonRpcProvider(config.citreaRpcUrl);
      wallet = new ethers.Wallet(config.citreaPrivateKey, provider);
    } catch (error) {
      throw new Error(`Error connecting to Citrea: ${error.message}`);
    }

    let tx;
    try {
      tx = await createBurnTx(wallet, backupData);
    } catch (error) {
      if (error.message.startsWith("insufficient funds")) {
        console.error(
          "\nError creating withdraw transaction: Insufficient funds."
        );
        console.error(
          "You need to have at least 10 cBTC in your Citrea wallet.\n"
        );
        try {
          const balance = await provider.getBalance(wallet.address);
          console.log(`Your citrea balance: ${balance} cBTC\n`);
        } catch (error) {
          console.error(`Error getting wallet balance: ${error.message}\n`);
        }
      }
      throw new Error(`Error creating withdraw transaction: ${error.message}`);
    }
    let txhash;
    try {
      const signature = await wallet.signTransaction(tx);
      txhash = ethers.keccak256(signature);
    } catch (error) {
      throw new Error(`Error signing transaction: ${error.message}`);
    }

    backupData = { ...backupData, burnTxHash: txhash };
    // write the hash to the file
    fs.writeFileSync(backupFilePath, JSON.stringify(backupData));

    let signedTx;
    try {
      // Sign and send the transaction
      signedTx = await wallet.sendTransaction(tx);
    } catch (error) {
      fs.writeFileSync(
        backupData,
        JSON.stringify({ ...backupData, burnTxHash: null })
      );
      throw new Error(`Error sending transaction: ${error.message}`);
    }
    try {
      const receipt = await signedTx.wait();

      const logs = receipt.logs;
      const logData = logs[0].data;

      const withdrawalIdx = parseInt(
        logData.slice(logData.length - 128, logData.length - 64),
        16
      );

      backupData = { ...backupData, withdrawalIdx, receipt };
      fs.writeFileSync(backupFilePath, JSON.stringify(backupData));
    } catch (error) {
      throw new Error(`Error getting receipt: ${error.message}`);
    }
    // sleep 1 second
    await new Promise((resolve) => setTimeout(resolve, 1000));

    console.log(
      `\nWithdrawal tx on Citrea completed, see the tx on ${config.citreaExplorerUrl}tx/${txhash}\n`
    );

  }

  if (!backupData.withdrawalIdx) {
    if (!backupData.burnTxHash) {
      throw new Error("Burn transaction hash is required.");
    }
    const provider = new ethers.JsonRpcProvider(config.citreaRpcUrl);

    const receipt = provider.getTransactionReceipt(backupData.burnTxHash);
    const logs = receipt.logs;
    const logData = logs[0].data;
    const withdrawalIdx = parseInt(
      logData.slice(logData.length - 128, logData.length - 64),
      16
    );
    backupData = { ...backupData, withdrawalIdx };
    fs.writeFileSync(backupFilePath, JSON.stringify(backupData));
  }

  if (!withdrawalAddress) {
    throw new Error("Withdrawal address is required.");
  }

  if (!minWithdrawalAmount) {
    throw new Error("Minimum withdrawal amount is required.");
  }
  try {
    minWithdrawalAmount = parseFloat(minWithdrawalAmount);
  } catch (error) {
    throw new Error("Invalid minimum withdrawal amount.");
  }

  if (!precision) {
    throw new Error("Precision is required.");
  }
  try {
    precision = parseFloat(precision);
  } catch (error) {
    throw new Error("Invalid precision.");
  }

  // amounts is from 10 to minWithdrawalAmount in steps of precision

  for (let amount = 10; amount >= minWithdrawalAmount; amount -= precision) {
    amount = amount.toFixed(8);
    console.log(`Trying to withdraw ${amount} BTC...`);
    try {
      const paymentTxid = await sendAnyoneCanPaySignature(config, {
        dustUtxoDetails: backupData,
        withdrawalAddress,
        amount,
      });
      console.log(
        `Withdrawal successful. You will receive ${amount} BTC soon.`
      );
      console.log(`Here are the payment txids:`);
      // iterate over paymentTxid.withdrawal_operator_payments
      for (const payment of paymentTxid.withdrawal_operator_payments) {
        console.log(`${config.bitcoinExplorerUrl}tx/${payment.txid}`);
      }
      // console.log(paymentTxid);
      // console.log(
      //   `Payment txid for ${amount} cBTC: ${JSON.stringify(paymentTxid)}`
      // );
      return paymentTxid;
    } catch (error) {
      console.log(`Error sending withdrawal signature: ${error.message}`);
      // sleep for 1 second
      await new Promise((resolve) => setTimeout(resolve, 1000));
    }
  }
  throw new Error("Withdrawal failed.");
};

program
  .command("withdraw")
  .description("Withdraw funds from Citrea to Bitcoin")
  .option(
    "-a, --withdrawal-address <address>",
    "Specify the withdrawal address on Bitcoin",
  )
  .option(
    "-m, --min-withdrawal-amount <amount>",
    "Specify the minimum withdrawal amount",
    "9.99"
  )
  .option("-p, --precision <number>", "Specify the precision", "0.001")
  .option(
    "-b, --backup-folder-path <path>",
    "Specify the backup folder path",
    "backups/"
  )
  .action(async (options) => {
    const {
      withdrawalAddress,
      minWithdrawalAmount,
      precision,
      backupFolderPath,
    } = options;
    config = { ...config, ...program.opts() };
    // Ensure required options are provided
    if (
      !config.bitcoinRpcUrl ||
      !config.bitcoinRpcUser ||
      !config.bitcoinRpcPassword ||
      !config.citreaRpcUrl ||
      !config.citreaPrivateKey ||
      !withdrawalAddress
    ) {
      console.error(
        "Error: --bitcoin-rpc-url, --bitcoin-rpc-user, --bitcoin-rpc-password, --citrea-rpc-url, and --citrea-private-key are required for this command."
      );
      process.exit(1);
    }
    // Step 1: Generate a new private key
    const privateKey = createRandomPrivKey();
    // Convert the private key to WIF format
    const WIF = privateKeyToWIF(privateKey, true, true);
    // Call getdescriptorinfo to get the address
    const descriptor = `tr(${WIF})`;

    const descriptor_results = await makeBitcoinRpcCall(
      config,
      "getdescriptorinfo",
      [descriptor]
    );
    const addressArray = await makeBitcoinRpcCall(config, "deriveaddresses", [
      descriptor_results.descriptor,
    ]);
    const address = addressArray[0];

    const backupFilePath = path.join(backupFolderPath, `${address}.json`);

    console.log(
      `\nTo be able to resume your withdrawal process, use this command:\n./clementine-cli.js resumewithdraw --backup-file-path ${backupFilePath}\n./clementine-cli.js resumewithdraw --help\nfor more information.\n\n`
    );

    fs.writeFileSync(
      backupFilePath,
      JSON.stringify({ descriptor, address, withdrawalAddress })
    );

    try {
      await withdraw(
        config,
        backupFilePath,
        withdrawalAddress,
        minWithdrawalAmount,
        precision
      );
    } catch (error) {
      console.log(`Error in withdrawal process: ${error.message}`);
      console.log(
        "\Resume the withdrawal process using the following command:"
      );
      console.log(
        `./clementine-cli.js resumewithdraw --backup-file-path ${backupFilePath}\n`
      );
    }
  });

program
  .command("resumewithdraw")
  .description("Resume withdrawal process from a backup file")
  .option("-b, --backup-file-path <path>", "Backup file path", "")
  .option(
    "-m, --min-withdrawal-amount <amount>",
    "Specify the minimum withdrawal amount",
    "9.99"
  )
  .option("-p, --precision <number>", "Specify the precision", "0.001")
  .action(async (options) => {
    const { minWithdrawalAmount, precision, backupFilePath } = options;
    config = { ...config, ...program.opts() };
    // Ensure required options are provided
    if (
      !config.bitcoinRpcUrl ||
      !config.bitcoinRpcUser ||
      !config.bitcoinRpcPassword ||
      !config.citreaRpcUrl ||
      !config.citreaPrivateKey
    ) {
      console.error(
        "Error: --bitcoin-rpc-url, --bitcoin-rpc-user, --bitcoin-rpc-password, --citrea-rpc-url, and --citrea-private-key are required for this command."
      );
      process.exit(1);
    }
    // read the backup file
    const backupData = JSON.parse(fs.readFileSync(backupFilePath, "utf-8"));
    if (!backupData.withdrawalAddress) {
      throw new Error("Withdrawal address is not set.");
    }
    try {
      await withdraw(
        config,
        backupFilePath,
        backupData.withdrawalAddress,
        minWithdrawalAmount,
        precision
      );
    } catch (error) {
      console.log(`Error in withdrawal process: ${error.message}`);
    }
  });

program.parse(process.argv);
