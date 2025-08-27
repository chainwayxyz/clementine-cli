use bitcoin::{Network, taproot::Signature};
use clap::{Parser, Subcommand};
use clementine_cli::{
    backup_wallet,
    config::BridgeCliConfig,
    deposit, get_all_wallets_with_addresses, get_deposit_params, show_mnemonic,
    wallet::{
        self, Purpose, create_encrypted_wallet_with_address, delete_wallet,
        import_wallet_from_file, import_wallet_from_mnemonic, verify_wallet_integrity,
    },
    withdrawal,
};
use colored::Colorize;
use std::path::PathBuf;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt};

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
    /// Path to config file. If not given, ~/.clementine/bridge_cli_config.toml or $PWD/bridge_cli_config.toml files will be used in that order.
    #[arg(long)]
    config_file: Option<PathBuf>,

    /// Bitcoin network.
    #[arg(long)]
    network: Network,

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
        /// Label for the wallet file
        label: String,
        purpose: Purpose,
    },
    /// Backup wallet to specified destination.
    Backup {
        /// Destination path for wallet backup
        destination: String,
        /// Address of the wallet to backup
        address: String,
    },
    /// Delete a wallet by address.
    Delete {
        /// Address of the wallet to delete
        address: String,
    },
    /// Show mnemonic with interactive terminal.
    ShowMnemonic {
        /// Wallet address to show mnemonic for
        address: String,
    },
    ShowPrivateKey {
        /// Wallet address to show private key for.
        address: String,
    },
    /// Import wallet using secure mnemonic input.
    ImportMnemonic {
        /// Label for the imported wallet
        label: String,
        /// Purpose for the imported wallet
        purpose: Purpose,
    },
    ImportPrivateKey {
        /// Label for the imported wallet
        label: String,
        /// Purpose for the imported wallet
        purpose: Purpose,
    },
    ImportFile {
        /// Filename to import wallet from
        filename: String,
        /// Label for the imported wallet
        label: Option<String>,
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
        recovery_taproot_address: String,
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
        signer_address: String,
        withdrawal_address: String,
        withdrawal_utxo_txid: String,
        withdrawal_utxo_vout: u32,
        withdrawal_amount: u64,
    },
    SendWithdrawalSignaturesToOperators {
        signer_address: String,
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

    let config = BridgeCliConfig::try_parse_config(cli.config_file, cli.network).unwrap();

    match cli.command {
        Commands::Wallet { command } => match command {
            WalletCommands::Create { label, purpose } => {
                let address =
                    match create_encrypted_wallet_with_address(config.network, label, purpose) {
                        Ok(addr) => addr,
                        Err(e) => {
                            eprintln!("{} {e}", "Error:".red().bold());
                            std::process::exit(1);
                        }
                    };
                println!(
                    "Wallet created with address: {}",
                    address.address_with_prefix()
                );
            }
            WalletCommands::Backup {
                destination,
                address,
            } => {
                match backup_wallet(&address, &destination) {
                    Ok((addr, dest)) => {
                        println!(
                            "Backup completed for address {} to destination {}",
                            addr.address_with_prefix(),
                            dest.display()
                        );
                    }
                    Err(e) => {
                        eprintln!("{} {e}", "Error:".red().bold());
                        std::process::exit(1);
                    }
                };
            }
            WalletCommands::ShowMnemonic { address } => {
                match show_mnemonic(&address) {
                    Ok(_) => println!("Mnemonic display completed"),
                    Err(e) => {
                        eprintln!("{} {e}", "Error:".red().bold());
                        std::process::exit(1);
                    }
                };
            }
            WalletCommands::ImportMnemonic { label, purpose } => {
                let address = match import_wallet_from_mnemonic(config.network, &label, purpose) {
                    Ok(addr) => addr,
                    Err(e) => {
                        eprintln!("{} {e}", "Error:".red().bold());
                        std::process::exit(1);
                    }
                };
                println!(
                    "Import completed for address: {}",
                    address.address_with_prefix()
                );
            }
            WalletCommands::ImportFile { filename, label } => {
                let address = match import_wallet_from_file(&filename, label.as_deref()) {
                    Ok(addr) => addr,
                    Err(e) => {
                        eprintln!("{} {e}", "Error:".red().bold());
                        std::process::exit(1);
                    }
                };
                println!(
                    "Import from file completed for address: {}",
                    address.address_with_prefix()
                );
            }
            WalletCommands::ImportPrivateKey { label, purpose } => {
                let address =
                    match wallet::import_wallet_from_private_key(config.network, &label, purpose) {
                        Ok(addr) => addr,
                        Err(e) => {
                            eprintln!("{} {e}", "Error:".red().bold());
                            std::process::exit(1);
                        }
                    };
                println!(
                    "Import from private key completed for address: {}",
                    address.address_with_prefix()
                );
                println!(
                    "{} Note: This wallet was imported from a private key, so no mnemonic phrase is available.",
                    "INFO".yellow()
                );
            }
            WalletCommands::Delete { address } => {
                match delete_wallet(&address) {
                    Ok(res) => res,
                    Err(e) => {
                        eprintln!("{} {e}", "Error:".red().bold());
                        std::process::exit(1);
                    }
                };
                println!("Wallet deleted");
            }
            WalletCommands::VerifyIntegrity => {
                match verify_wallet_integrity() {
                    Ok(res) => res,
                    Err(e) => {
                        eprintln!("{} {e}", "Error:".red().bold());
                        std::process::exit(1);
                    }
                };
                println!("Wallet integrity verification completed");
            }
            WalletCommands::List => {
                match get_all_wallets_with_addresses() {
                    Ok(w) => w,
                    Err(e) => {
                        eprintln!("{} {e}", "Error:".red().bold());
                        std::process::exit(1);
                    }
                };
            }
            WalletCommands::ShowPrivateKey { address } => {
                match wallet::show_private_key(&address) {
                    Ok(_) => println!("Private key display completed"),
                    Err(e) => {
                        eprintln!("{} {e}", "Error:".red().bold());
                        std::process::exit(1);
                    }
                };
            }
        },
        Commands::Deposit { command } => match command {
            DepositCommands::GetDepositAddress {
                citrea_address,
                recovery_taproot_address,
            } => {
                let deposit_address = match deposit::get_deposit_address(
                    &citrea_address,
                    &recovery_taproot_address,
                    &config,
                )
                .await
                {
                    Ok(addr) => addr,
                    Err(e) => {
                        eprintln!("{} {e}", "Error:".red().bold());
                        std::process::exit(1);
                    }
                };
                println!("Deposit address: {}", deposit_address);
            }
            DepositCommands::SignRecoveryTx {
                recovery_taproot_address,
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

                match deposit::sign_recovery_tx(
                    &evm_address,
                    &recovery_taproot_address,
                    &deposit_txid,
                    deposit_vout,
                    &claim_address,
                    fee_rate,
                    amount,
                    &config,
                ) {
                    Ok(tx) => {
                        println!("Recovery transaction hex: {}", serialize_and_encode(tx));
                    }
                    Err(e) => {
                        eprintln!("{} {e}", "Error:".red().bold());
                        std::process::exit(1);
                    }
                };
            }
            DepositCommands::VerifyRecoveryTx {
                recovery_tx,
                evm_address,
                recovery_taproot_address,
                amount,
            } => {
                let result: (bitcoin::Txid, bitcoin::Address, bitcoin::Amount) =
                    match deposit::verify_recovery_tx(
                        &recovery_tx,
                        &evm_address,
                        &recovery_taproot_address,
                        amount,
                        &config,
                    ) {
                        Ok(res) => res,
                        Err(e) => {
                            eprintln!("{} {e}", "Error:".red().bold());
                            std::process::exit(1);
                        }
                    };
                println!("Recovery transaction verification completed!");
                println!("Txid: {}", result.0);
                println!("Address: {}", result.1);
                println!("Amount: {}", result.2);
            }
            DepositCommands::DepositStatus { deposit_address } => {
                unimplemented!("deposit.deposit_status: {}", deposit_address);
            }
            DepositCommands::GetDepositParams { move_to_vault_txid } => {
                match get_deposit_params(&move_to_vault_txid, &config).await {
                    Ok(params) => {
                        println!("Deposit parameters hex: {}", hex::encode(params));
                    }
                    Err(e) => {
                        eprintln!("{} {e}", "Error:".red().bold());
                        std::process::exit(1);
                    }
                };
            }
        },
        Commands::Withdrawal { command } => match command {
            WithdrawalCommands::GenerateWithdrawalSignature {
                signer_address,
                withdrawal_address,
                withdrawal_utxo,
                amount,
            } => {
                fn serialize_and_encode(signature: Signature) -> String {
                    hex::encode(signature.serialize())
                }

                match withdrawal::generate_withdrawal_signature(
                    &signer_address,
                    &withdrawal_address,
                    &withdrawal_utxo,
                    amount,
                    config.network,
                ) {
                    Ok(signature) => {
                        println!(
                            "Withdrawal signature hex: {}",
                            serialize_and_encode(signature)
                        );
                    }
                    Err(e) => {
                        eprintln!("{} {e}", "Error:".red().bold());
                        std::process::exit(1);
                    }
                };
            }
            WithdrawalCommands::SafeWithdraw {
                signer_address,
                withdrawal_address,
                withdrawal_utxo,
                amount,
                signature,
            } => {
                let withdrawal_ui_url = match withdrawal::safe_withdraw(
                    &signer_address,
                    &withdrawal_address,
                    &withdrawal_utxo,
                    amount,
                    &signature,
                    &config,
                )
                .await
                {
                    Ok(url) => url,
                    Err(e) => {
                        eprintln!("{} {e}", "Error:".red().bold());
                        std::process::exit(1);
                    }
                };
                println!(
                    "\n{} Opening withdrawal page {withdrawal_ui_url} in your default browser...",
                    "INFO".green().bold()
                );
            }
            WithdrawalCommands::SendSafeWithdrawal {
                signer_address,
                withdrawal_address,
                withdrawal_utxo,
                amount,
                signature,
            } => {
                let result = match withdrawal::send_safe_withdrawal(
                    &signer_address,
                    &withdrawal_address,
                    &withdrawal_utxo,
                    amount,
                    &signature,
                    &config,
                )
                .await
                {
                    Ok(res) => res,
                    Err(e) => {
                        eprintln!("{} {e}", "Error:".red().bold());
                        std::process::exit(1);
                    }
                };
                println!("Safe withdrawal transaction sent!");
                println!("Transaction Receipt: {:#?}", result);
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
