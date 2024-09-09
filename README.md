# Clementine CLI

[Clementine](https://github.com/chainwayxyz/clementine) is Citrea's BitVM-based, trust-minimized two-way peg program. This repository includes a CLI tool to interact with Clementine. For detailed usage and documentation, refer to this [guide](https://docs.citrea.xyz).

The current functionality of the CLI is divided into two main operations:
- **Deposit** (work in progress)
- **Withdrawal** (fully functional)

<details>
  <summary><strong>Deposit</strong></summary>

Currently, the deposit operation is provided through a [UI](https://citrea.xyz/bridge). The primary function is to communicate with a backend that generates a deposit address tied to the user's EVM address and initiates backend tasks for BitVM pre-signature collection.

In the future, we plan to migrate the deposit functionality into this CLI. The implementation is still **work in progress**.

</details>

<details>
  <summary><strong>Withdrawal</strong></summary>

The withdrawal operation is fully functional via this CLI. Users can initiate a withdrawal from Clementine using two main parameters:
- `min-withdrawal-amount`: The minimum BTC amount the user wants to withdraw.
- `num-rounds`: The number of rounds to gradually decrease the withdrawal request from 10 BTC down to the `min-withdrawal-amount`.

The program will start publishing withdrawal intents, beginning with 10 BTC and decreasing in equal steps over the specified number of rounds.

### Example:
A user runs the CLI with `min-withdrawal-amount 9.5` and `num-rounds 5`. The CLI will publish withdrawal intents as follows:
1. Request **10 BTC**, if no response:
2. Request **9.9 BTC**, if no response:
3. Request **9.8 BTC**, if no response:
4. Request **9.7 BTC**, if no response:
5. Request **9.6 BTC**, if no response:
6. Request **9.5 BTC**, if no response:
7. Cancel the operation if there is no response at 9.5 BTC.

</details>

---

### Installation

#### 1. Install Node.js & npm
To install Node.js and npm, follow the instructions specific to your operating system:

##### For Linux (Ubuntu/Debian):
```bash
sudo apt update
sudo apt install nodejs npm
```

##### For macOS:
```bash
brew install node
```

##### For Windows:
Download and install Node.js from the [official website](https://nodejs.org/).

#### 2. Install Required Packages
After cloning the repository, install the necessary npm packages:
```bash
npm install
```

#### 3. Run the CLI
To initiate a withdrawal, run the following command:

```bash
./clementine-cli.js withdraw \
  --bitcoin-rpc-url YOUR_BTC_RPC_URL \
  --rpcuser YOUR_BTC_RPC_USER \
  --rpcpassword YOUR_BTC_RPC_PASSWORD \
  --citrea-rpc-url YOUR_CITREA_RPC_URL \
  --citrea-private-key YOUR_CITREA_PRIVATE_KEY \
  --withdrawal-sig-endpoints http://127.0.0.1:8080/withdrawals \
  --min-withdrawal-amount AMOUNT \
  --num-rounds ROUNDS
```

- `YOUR_BTC_RPC_URL`: The URL of your Bitcoin Core node's RPC interface.
- `YOUR_BTC_RPC_USER`: Your Bitcoin Core node's RPC username.
- `YOUR_BTC_RPC_PASSWORD`: Your Bitcoin Core node's RPC password.
- `YOUR_CITREA_RPC_URL`: The URL of the Citrea node you are interacting with.
- `YOUR_CITREA_PRIVATE_KEY`: The private key that holds the funds to be withdrawn.
- `AMOUNT`: The minimum BTC amount you wish to withdraw.
- `ROUNDS`: The number of rounds to decrement the withdrawal amount.

#### Important Notes
- This CLI interacts with your **local Bitcoin Core wallet**. The wallet will receive the funds after the withdrawal is processed.
- Use a **local and secure Bitcoin Core node** for the withdrawal process to ensure the safety of your funds.
- While you can use any Citrea RPC endpoint, the funds are controlled locally by the provided private key.
