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
- `precision`: The decrement step for the withdrawal amount.

The program will start publishing withdrawal intents, beginning with 10 BTC and decreasing in equal steps defined by the precision parameter. If there is no response from the Citrea node, the program will cancel the operation at the specified minimum withdrawal amount.

### Example:
A user runs the CLI with `min-withdrawal-amount 9.995` and `precision 0.001`. The CLI will publish withdrawal intents as follows:
1. Request **10 BTC**, if no response:
2. Request **9.999 BTC**, if no response:
3. Request **9.998 BTC**, if no response:
4. Request **9.997 BTC**, if no response:
5. Request **9.996 BTC**, if no response:
6. Request **9.995 BTC**, if no response:
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

Before running the CLI, you need to configure the `config.json`:

```bash
cp config.example.json config.json
```

Then, run the CLI with the following command:

```bash
./clementine-cli.js withdraw \
  --withdrawal-address YOUR_BTC_ADDRESS
```

This will use the default parameters as min-withdrawal-amount 9.99 and precision 0.001.

To customize the withdrawal parameters, use the following command:

```bash
./clementine-cli.js withdraw \
  --withdrawal-address YOUR_BTC_ADDRESS \
  --min-withdrawal-amount 9.995 \
  --precision 0.001
```

#### Important Notes
- This CLI interacts with your **local Bitcoin Core wallet**. The wallet will receive the funds after the withdrawal is processed.
- Use a **local and secure Bitcoin Core node** for the withdrawal process to ensure the safety of your funds.
- While you can use any Citrea RPC endpoint, the funds are controlled locally by the provided private key.
