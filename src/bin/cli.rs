use bitcoin::Network;
use clap::{Parser, Subcommand};
use clementine_cli::{deposit, withdrawal};

#[derive(Parser)]
#[command(name = "clementine")]
#[command(about = "Clementine CLI - wallet-agnostic Citrea bridge CLI", long_about = None)]
struct Cli {
    /// Bitcoin network to use (bitcoin, testnet, testnet4)
    #[arg(long, default_value = "bitcoin")]
    network: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Deposit {
        #[command(subcommand)]
        command: DepositCommands,
    },
    Withdrawal {
        #[command(subcommand)]
        command: WithdrawalCommands,
    },
}

#[derive(Subcommand)]
enum DepositCommands {
    GenerateRecoveryKey {
        #[arg(short, long)]
        y: bool,
        #[arg(long)]
        private_key: Option<String>,
    },
    GetDepositAddress {
        citrea_address: String,
        recovery_taproot_address: String,
    },
    SignRecoveryTx {
        evm_address: String,
        recovery_taproot_address: String,
        deposit_txid: String,
        deposit_vout: u32,
        claim_address: String,
        #[arg(long)]
        fee_rate: Option<u64>,
        /// Amount in BTC (e.g., 0.1 for 0.1 BTC)
        #[arg(long)]
        amount: Option<f64>,
    },
    VerifyRecoveryTx {
        recovery_tx: String,
        evm_address: String,
        recovery_taproot_address: String,
        /// Amount in BTC (e.g., 0.1 for 0.1 BTC)
        #[arg(long)]
        amount: Option<f64>,
    },
    DepositStatus {
        deposit_address: String,
    },
}

#[derive(Subcommand)]
enum WithdrawalCommands {
    GenerateSignerAddress {
        #[arg(short, long)]
        y: bool,
    },
    GenerateWithdrawalSignature {
        signer_address: String,
        withdrawal_address: String,
        withdrawal_utxo: String,
        amount: f64,
    },
    SafeWithdraw {
        signer_address: String,
        withdrawal_address: String,
        withdrawal_utxo: String,
        amount: f64,
        signature: String,
        #[arg(long)]
        bitcoind_rpc_url: Option<String>,
        #[arg(long)]
        bitcoind_rpc_user: Option<String>,
        #[arg(long)]
        bitcoind_rpc_password: Option<String>,
    },
    Status {
        withdrawal_index: u32,
    },
    GenerateOperatorWithdrawalSignatures {
        withdrawal_address: String,
        signer_address: String,
        withdrawal_utxo_txid: String,
        withdrawal_utxo_vout: u32,
        withdrawal_amount: u64,
    },
    SendWithdrawalSignaturesToOperators {
        withdrawal_address: String,
        signer_address: String,
        withdrawal_utxo_txid: String,
        withdrawal_utxo_vout: u32,
        withdrawal_index: u32,
        signature: String,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Parse network string using bitcoin crate's parsing
    let network = match cli.network.parse::<Network>() {
        Ok(network) => network,
        Err(_) => {
            eprintln!(
                "Error: Invalid network '{}'. Use: bitcoin, testnet, signet, regtest or testnet4",
                cli.network
            );
            std::process::exit(1);
        }
    };

    match cli.command {
        Commands::Deposit { command } => match command {
            DepositCommands::GenerateRecoveryKey { y, private_key } => {
                if let Err(e) = deposit::generate_recovery_key(y, private_key, network) {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
            DepositCommands::GetDepositAddress {
                citrea_address,
                recovery_taproot_address,
            } => {
                if let Err(e) = deposit::get_deposit_address(
                    &citrea_address,
                    &recovery_taproot_address,
                    network,
                ) {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
            DepositCommands::SignRecoveryTx {
                evm_address,
                recovery_taproot_address,
                deposit_txid,
                deposit_vout,
                claim_address,
                fee_rate,
                amount,
            } => {
                if let Err(e) = deposit::sign_recovery_tx(
                    &evm_address,
                    &recovery_taproot_address,
                    &deposit_txid,
                    deposit_vout,
                    &claim_address,
                    fee_rate,
                    amount,
                    network,
                ) {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
            DepositCommands::VerifyRecoveryTx {
                recovery_tx,
                evm_address,
                recovery_taproot_address,
                amount,
            } => {
                if let Err(e) = deposit::verify_recovery_tx(
                    &recovery_tx,
                    &evm_address,
                    &recovery_taproot_address,
                    amount,
                    network,
                ) {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
            DepositCommands::DepositStatus { deposit_address } => {
                println!("TODO: deposit.deposit_status: {}", deposit_address);
            }
        },
        Commands::Withdrawal { command } => match command {
            WithdrawalCommands::GenerateSignerAddress { y } => {
                if let Err(e) = withdrawal::generate_signer_address(y, network) {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
            WithdrawalCommands::GenerateWithdrawalSignature {
                withdrawal_address,
                signer_address,
                withdrawal_utxo,
                amount,
            } => {
                if let Err(e) = withdrawal::generate_withdrawal_signature(
                    &signer_address,
                    &withdrawal_address,
                    &withdrawal_utxo,
                    amount,
                    network,
                ) {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
            WithdrawalCommands::SafeWithdraw {
                signer_address,
                withdrawal_address,
                withdrawal_utxo,
                amount,
                signature,
                bitcoind_rpc_url,
                bitcoind_rpc_user,
                bitcoind_rpc_password,
            } => {
                if let Err(e) = withdrawal::safe_withdraw(
                    &signer_address,
                    &withdrawal_address,
                    &withdrawal_utxo,
                    amount,
                    &signature,
                    bitcoind_rpc_url.as_deref(),
                    bitcoind_rpc_user.as_deref(),
                    bitcoind_rpc_password.as_deref(),
                    network,
                )
                .await {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
            WithdrawalCommands::Status { withdrawal_index } => {
                println!("TODO: withdrawal.status: {}", withdrawal_index);
            }
            WithdrawalCommands::GenerateOperatorWithdrawalSignatures {
                withdrawal_address,
                signer_address,
                withdrawal_utxo_txid,
                withdrawal_utxo_vout,
                withdrawal_amount,
            } => {
                println!(
                    "TODO: withdrawal.generate_operator_withdrawal_signatures: {} {} {} {} {}",
                    withdrawal_address,
                    signer_address,
                    withdrawal_utxo_txid,
                    withdrawal_utxo_vout,
                    withdrawal_amount
                );
            }
            WithdrawalCommands::SendWithdrawalSignaturesToOperators {
                withdrawal_address,
                signer_address,
                withdrawal_utxo_txid,
                withdrawal_utxo_vout,
                withdrawal_index,
                signature,
            } => {
                println!(
                    "TODO: withdrawal.send_withdrawal_signatures_to_operators: {} {} {} {} {} {}",
                    withdrawal_address,
                    signer_address,
                    withdrawal_utxo_txid,
                    withdrawal_utxo_vout,
                    withdrawal_index,
                    signature
                );
            }
        },
    }
}
