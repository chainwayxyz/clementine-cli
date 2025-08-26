use bitcoin::taproot::Signature;
use clap::{Parser, Subcommand};
use clementine_cli::{
    backup_wallet, config::BridgeCliConfig, create_encrypted_wallet_with_address, delete_wallet,
    deposit, get_all_wallets_with_addresses, get_deposit_params, import_wallet_from_file,
    import_wallet_from_mnemonic, import_wallet_from_private_key, show_mnemonic, show_private_key,
    verify_wallet_integrity, withdrawal,
};
use colored::Colorize;
use std::path::PathBuf;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt};

macro_rules! handle_or_exit {
    ($expr:expr) => {
        if let Err(e) = $expr {
            eprintln!("{} {:?}", "Error:".red().bold(), eyre::Report::from(e));
            std::process::exit(1);
        }
    };
}

macro_rules! print_or_exit {
    ($expr:expr) => {
        match $expr {
            Ok(result) => println!("{result:?}"),
            Err(e) => {
                eprintln!("{} {e}", "Error:".red().bold());
                std::process::exit(1);
            }
        }
    };
    ($expr:expr, $wrapper:expr) => {
        match $expr {
            Ok(result) => println!("{:?}", $wrapper(result)),
            Err(e) => {
                eprintln!("{} {e}", "Error:".red().bold());
                std::process::exit(1);
            }
        }
    };
}

/// Initializes tracing to `Debug` level if verbose flag is given. If not,
/// defaults to `RUST_LOG` env variable.
pub(crate) fn initialize_logger(is_verbose: bool) {
    let level = if is_verbose {
        Some(LevelFilter::DEBUG)
    } else {
        None
    };

    let filter = match level {
        Some(lvl) => EnvFilter::builder()
            .with_default_directive(lvl.into())
            .from_env_lossy(),
        None => EnvFilter::from_default_env(),
    };

    let standard_layer = fmt::layer()
        .with_test_writer()
        .with_file(true)
        .with_line_number(true)
        .with_target(true);

    let _ = tracing::subscriber::set_global_default(
        tracing_subscriber::registry()
            .with(standard_layer)
            .with(filter),
    );
}

#[derive(Parser)]
#[command(name = "clementine")]
#[command(about = "Clementine CLI - wallet-agnostic Citrea bridge CLI", long_about = None, version)]
struct Cli {
    /// Path to config file. If not given, current directory will be searched for the bridge_cli_config.toml file
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
    /// Wallet related operations.
    Wallet {
        #[command(subcommand)]
        command: WalletCommands,
    },
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
enum WalletCommands {
    /// Create a new wallet with mnemonic display.
    Create {
        /// Name for the wallet file
        wallet_name: String,
    },
    /// Backup wallet to specified destination.
    Backup {
        /// Destination path for wallet backup
        destination: String,
        /// Name of the wallet to backup
        wallet_name: String,
    },
    /// Delete a wallet by name.
    Delete {
        /// Name of the wallet to delete
        wallet_name: String,
    },
    /// Show mnemonic with interactive terminal.
    ShowMnemonic {
        /// Wallet name to show mnemonic for
        wallet_name: String,
    },
    ShowPrivateKey {
        /// Wallet name to show private key for.
        wallet_name: String,
    },
    /// Import wallet using secure mnemonic input.
    ImportMnemonic {
        /// Name for the imported wallet
        wallet_name: String,
    },
    ImportPrivateKey {
        /// Name for the imported wallet
        wallet_name: String,
    },
    ImportFile {
        /// Filename to import wallet from
        filename: String,
        /// Name for the imported wallet
        wallet_name: String,
    },
    /// Verify integrity of wallet registry and files.
    VerifyIntegrity,
    /// List all wallets with their addresses.
    List,
}

#[derive(Subcommand)]
enum DepositCommands {
    GetDepositAddress {
        citrea_address: String,
        recovery_taproot_address: String,
    },
    SignRecoveryTx {
        wallet_name: String,
        evm_address: String,
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
        wallet_name: String,
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
    GenerateWithdrawalSignature {
        wallet_name: String,
        withdrawal_address: String,
        withdrawal_utxo: String,
        amount: f64,
    },
    SafeWithdraw {
        wallet_name: String,
        withdrawal_address: String,
        withdrawal_utxo: String,
        amount: f64,
        signature: String,
    },
    SendSafeWithdrawal {
        wallet_name: String,
        withdrawal_address: String,
        withdrawal_utxo: String,
        amount: f64,
        signature: String,
    },
    Status {
        withdrawal_index: u32,
    },
    GenerateOperatorWithdrawalSignatures {
        wallet_name: String,
        withdrawal_address: String,
        withdrawal_utxo_txid: String,
        withdrawal_utxo_vout: u32,
        withdrawal_amount: u64,
    },
    SendWithdrawalSignaturesToOperators {
        wallet_name: String,
        withdrawal_address: String,
        withdrawal_utxo_txid: String,
        withdrawal_utxo_vout: u32,
        withdrawal_index: u32,
        signature: String,
    },
}

#[tokio::main]
async fn main() {
    color_eyre::install().expect("Failed to install color-eyre");

    let cli = Cli::parse();

    initialize_logger(cli.verbose);

    let config = if let Some(config_file_path) = cli.config_file {
        tracing::info!("Config file {config_file_path:?} is going to be used...");
        BridgeCliConfig::try_parse_file(config_file_path.clone()).unwrap_or_else(|e| {
            panic!(
                "Failed to read config file {:?}: {:?}",
                config_file_path.display(),
                e
            )
        })
    } else {
        let mut current_dir = std::env::current_dir().unwrap();
        current_dir.push("bridge_cli_config.toml");
        tracing::info!(
            "No config file given, looking for the current directory: {current_dir:?}..."
        );
        BridgeCliConfig::try_parse_file(current_dir.clone()).unwrap_or_else(|e| {
            panic!(
                "Failed to read config file: {:?}: {e}",
                current_dir.display()
            )
        })
    };

    match cli.command {
        Commands::Wallet { command } => match command {
            WalletCommands::Create { wallet_name } => {
                print_or_exit!(create_encrypted_wallet_with_address(
                    config.network,
                    wallet_name
                ));
            }
            WalletCommands::Backup {
                destination,
                wallet_name,
            } => {
                print_or_exit!(backup_wallet(&wallet_name, &destination));
            }
            WalletCommands::ShowMnemonic { wallet_name } => {
                handle_or_exit!(show_mnemonic(&wallet_name));
            }
            WalletCommands::ImportMnemonic { wallet_name } => {
                handle_or_exit!(import_wallet_from_mnemonic(config.network, &wallet_name));
            }
            WalletCommands::ImportFile {
                filename,
                wallet_name,
            } => {
                handle_or_exit!(import_wallet_from_file(&filename, &wallet_name));
            }
            WalletCommands::ImportPrivateKey { wallet_name } => {
                handle_or_exit!(import_wallet_from_private_key(config.network, &wallet_name));
            }
            WalletCommands::Delete { wallet_name } => {
                handle_or_exit!(delete_wallet(&wallet_name));
            }
            WalletCommands::VerifyIntegrity => {
                handle_or_exit!(verify_wallet_integrity());
            }
            WalletCommands::List => {
                handle_or_exit!(get_all_wallets_with_addresses());
            }
            WalletCommands::ShowPrivateKey { wallet_name } => {
                handle_or_exit!(show_private_key(&wallet_name));
            }
        },
        Commands::Deposit { command } => match command {
            DepositCommands::GetDepositAddress {
                citrea_address,
                recovery_taproot_address,
            } => {
                print_or_exit!(
                    deposit::get_deposit_address(
                        &citrea_address,
                        &recovery_taproot_address,
                        &config,
                    )
                    .await
                );
            }
            DepositCommands::SignRecoveryTx {
                wallet_name,
                evm_address,
                deposit_txid,
                deposit_vout,
                claim_address,
                fee_rate,
                amount,
            } => {
                fn serialize_and_encode(tx: bitcoin::Transaction) -> String {
                    hex::encode(bitcoin::consensus::serialize(&tx))
                }

                print_or_exit!(
                    deposit::sign_recovery_tx(
                        &evm_address,
                        &wallet_name,
                        &deposit_txid,
                        deposit_vout,
                        &claim_address,
                        fee_rate,
                        amount,
                        &config,
                    ),
                    serialize_and_encode
                );
            }
            DepositCommands::VerifyRecoveryTx {
                recovery_tx,
                evm_address,
                wallet_name,
                amount,
            } => {
                handle_or_exit!(deposit::verify_recovery_tx(
                    &recovery_tx,
                    &evm_address,
                    &wallet_name,
                    amount,
                    &config,
                ));
            }
            DepositCommands::DepositStatus { deposit_address } => {
                unimplemented!("deposit.deposit_status: {}", deposit_address);
            }
            DepositCommands::GetDepositParams { move_to_vault_txid } => {
                print_or_exit!(
                    get_deposit_params(&move_to_vault_txid, &config).await,
                    hex::encode
                );
            }
        },
        Commands::Withdrawal { command } => match command {
            WithdrawalCommands::GenerateWithdrawalSignature {
                wallet_name,
                withdrawal_address,
                withdrawal_utxo,
                amount,
            } => {
                fn serialize_and_encode(signature: Signature) -> String {
                    hex::encode(signature.serialize())
                }

                print_or_exit!(
                    withdrawal::generate_withdrawal_signature(
                        &wallet_name,
                        &withdrawal_address,
                        &withdrawal_utxo,
                        amount,
                        config.network,
                    ),
                    serialize_and_encode
                );
            }
            WithdrawalCommands::SafeWithdraw {
                wallet_name,
                withdrawal_address,
                withdrawal_utxo,
                amount,
                signature,
            } => {
                handle_or_exit!(
                    withdrawal::safe_withdraw(
                        &wallet_name,
                        &withdrawal_address,
                        &withdrawal_utxo,
                        amount,
                        &signature,
                        &config,
                    )
                    .await
                );
            }
            WithdrawalCommands::SendSafeWithdrawal {
                wallet_name,
                withdrawal_address,
                withdrawal_utxo,
                amount,
                signature,
            } => {
                print_or_exit!(
                    withdrawal::send_safe_withdrawal(
                        &wallet_name,
                        &withdrawal_address,
                        &withdrawal_utxo,
                        amount,
                        &signature,
                        &config,
                    )
                    .await
                );
            }
            WithdrawalCommands::Status { withdrawal_index } => {
                unimplemented!("withdrawal.status: {}", withdrawal_index);
            }
            WithdrawalCommands::GenerateOperatorWithdrawalSignatures {
                withdrawal_address,
                wallet_name,
                withdrawal_utxo_txid,
                withdrawal_utxo_vout,
                withdrawal_amount,
            } => {
                unimplemented!(
                    "withdrawal.generate_operator_withdrawal_signatures: {} {} {} {} {}",
                    withdrawal_address,
                    wallet_name,
                    withdrawal_utxo_txid,
                    withdrawal_utxo_vout,
                    withdrawal_amount
                );
            }
            WithdrawalCommands::SendWithdrawalSignaturesToOperators {
                withdrawal_address,
                wallet_name,
                withdrawal_utxo_txid,
                withdrawal_utxo_vout,
                withdrawal_index,
                signature,
            } => {
                unimplemented!(
                    "withdrawal.send_withdrawal_signatures_to_operators: {} {} {} {} {} {}",
                    withdrawal_address,
                    wallet_name,
                    withdrawal_utxo_txid,
                    withdrawal_utxo_vout,
                    withdrawal_index,
                    signature
                );
            }
        },
    }
}
