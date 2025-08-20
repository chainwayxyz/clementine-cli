use clap::{Parser, Subcommand};
use clementine_cli::{config::BridgeCliConfig, deposit, utils::initialize_logger, withdrawal};
use std::path::PathBuf;
use tracing::level_filters::LevelFilter;

#[derive(Parser)]
#[command(name = "clementine")]
#[command(about = "Clementine CLI - wallet-agnostic Citrea bridge CLI", long_about = None)]
struct Cli {
    /// Path to config file. If not given, current directory will be searched for the bridge_bridge_cli_config.toml file
    #[arg(long)]
    config_file: Option<PathBuf>,

    /// Turns verbose logging on
    #[arg(long, action = clap::ArgAction::SetTrue)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Deposit related operations.
    Deposit {
        #[command(subcommand)]
        command: DepositCommands,
    },
    /// Withdrawal related operations.
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
    GetDepositParams {
        move_to_vault_txid: String,
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
    },
    SendSafeWithdrawal {
        signer_address: String,
        withdrawal_address: String,
        withdrawal_utxo: String,
        amount: f64,
        signature: String,
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

    let level_filter = if cli.verbose {
        Some(LevelFilter::DEBUG)
    } else {
        None
    };
    initialize_logger(level_filter).unwrap();

    let config = if let Some(config_file_path) = cli.config_file {
        tracing::debug!("Config file {config_file_path:?} is going to be used...");
        BridgeCliConfig::try_parse_file(config_file_path).unwrap()
    } else {
        let mut current_dir = std::env::current_dir().unwrap();
        current_dir.push("bridge_cli_config.toml");
        tracing::debug!(
            "No config file given, looking for the current directory: {current_dir:?}..."
        );
        BridgeCliConfig::try_parse_file(current_dir).unwrap()
    };

    match cli.command {
        Commands::Deposit { command } => match command {
            DepositCommands::GenerateRecoveryKey { y, private_key } => {
                if let Err(e) = deposit::generate_recovery_key(y, private_key, config.network) {
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
                    &config,
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
                    &config,
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
                    &config,
                ) {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
            DepositCommands::DepositStatus { deposit_address } => {
                unimplemented!("deposit.deposit_status: {}", deposit_address);
            }
            DepositCommands::GetDepositParams { move_to_vault_txid } => {
                if let Err(e) = deposit::get_deposit_params(&move_to_vault_txid, &config).await {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        },
        Commands::Withdrawal { command } => match command {
            WithdrawalCommands::GenerateSignerAddress { y } => {
                match withdrawal::generate_signer_address(y, config.network) {
                    Ok(address) => println!(
                        "Address for {} is {}\n
                        Please send 0.0000033 BTC (330 sats) to this address.",
                        config.network, address
                    ),
                    Err(e) => {
                        eprintln!("Error while generating signer address: {}", e);
                        std::process::exit(1);
                    }
                }
            }
            WithdrawalCommands::GenerateWithdrawalSignature {
                withdrawal_address,
                signer_address,
                withdrawal_utxo,
                amount,
            } => {
                match withdrawal::generate_withdrawal_signature(
                    &signer_address,
                    &withdrawal_address,
                    &withdrawal_utxo,
                    amount,
                    config.network,
                ) {
                    Ok(signature) => println!("Signature: {}", hex::encode(signature.serialize())),
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                }
            }
            WithdrawalCommands::SafeWithdraw {
                signer_address,
                withdrawal_address,
                withdrawal_utxo,
                amount,
                signature,
            } => {
                if let Err(e) = withdrawal::safe_withdraw(
                    &signer_address,
                    &withdrawal_address,
                    &withdrawal_utxo,
                    amount,
                    &signature,
                    &config,
                )
                .await
                {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
            WithdrawalCommands::SendSafeWithdrawal {
                signer_address,
                withdrawal_address,
                withdrawal_utxo,
                amount,
                signature,
            } => {
                if let Err(e) = withdrawal::send_safe_withdrawal(
                    &signer_address,
                    &withdrawal_address,
                    &withdrawal_utxo,
                    amount,
                    &signature,
                    &config,
                )
                .await
                {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
            WithdrawalCommands::Status { withdrawal_index } => {
                unimplemented!("withdrawal.status: {}", withdrawal_index);
            }
            WithdrawalCommands::GenerateOperatorWithdrawalSignatures {
                withdrawal_address,
                signer_address,
                withdrawal_utxo_txid,
                withdrawal_utxo_vout,
                withdrawal_amount,
            } => {
                unimplemented!(
                    "withdrawal.generate_operator_withdrawal_signatures: {} {} {} {} {}",
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
                unimplemented!(
                    "withdrawal.send_withdrawal_signatures_to_operators: {} {} {} {} {} {}",
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
