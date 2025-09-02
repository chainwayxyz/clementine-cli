use bitcoin::{
    Amount, Network, OutPoint, Transaction, Txid, consensus::deserialize, taproot::Signature,
};
use clap::{Parser, Subcommand};
use clementine_cli::errors::PrintErr;
use clementine_cli::{
    BitcoinAddress,
    cli::{
        cli_backup_wallet, cli_create_wallet, cli_import_wallet_from_file,
        cli_import_wallet_from_mnemonic, cli_import_wallet_from_private_key, cli_show_mnemonic,
        cli_show_private_key, cli_verify_wallet_integrity, deposit_status,
        send_withdrawal_signatures, withdrawal_status,
    },
    config::BridgeCliConfig,
    deposit, get_deposit_params, handle_cli_command, parse_citrea_address,
    print_all_wallets_with_addresses,
    structs::TaprootAddressWithPrefix,
    wallet::{Purpose, parse_address, parse_taproot_address},
    withdrawal,
};
use colored::Colorize;
use std::path::PathBuf;
use std::str::FromStr;
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
        /// Bitcoin network (required for this subcommand)
        #[arg(long)]
        network: Network,
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
        /// Bitcoin network (required for this subcommand)
        #[arg(long)]
        network: Network,
        /// Label for the imported wallet
        label: String,
        /// Purpose for the imported wallet
        purpose: Purpose,
    },
    ImportPrivateKey {
        /// Bitcoin network (required for this subcommand)
        #[arg(long)]
        network: Network,
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
        /// Bitcoin network (required for this subcommand)
        #[arg(long)]
        network: Network,
        recovery_taproot_address: String,
        citrea_address: String,
    },
    SignRecoveryTx {
        recovery_taproot_address: String,
        evm_address: String,
        deposit_txid: String,
        deposit_vout: u32,
        claim_address: String,
        fee_rate: u64,
        /// Amount in BTC (e.g., 0.1 for 0.1 BTC)
        amount: f64,
        /// Bitcoin network (required for this subcommand)
        #[arg(long)]
        network: Network,
    },
    VerifyRecoveryTx {
        recovery_tx: String,
        recovery_taproot_address: String,
        evm_address: String,
        /// Amount in BTC (e.g., 0.1 for 0.1 BTC)
        #[arg(long)]
        amount: Option<f64>,
        #[arg(long)]
        network: Network,
    },
    Status {
        deposit_address: String,
        #[arg(long)]
        network: Network,
    },
    GetDepositParams {
        move_to_vault_txid: String,
        #[arg(long)]
        network: Network,
    },
}

#[derive(Subcommand)]
enum WithdrawalCommands {
    Start {
        #[arg(long)]
        network: Network,
        signer_address: String,
        claim_address: String,
    },
    Scan {
        #[arg(long)]
        network: Network,
        signer_address: String,
        claim_address: String,
    },
    GenerateWithdrawalSignature {
        #[arg(long)]
        network: Network,
        signer_address: String,
        withdrawal_address: String,
        withdrawal_utxo: String,
        amount: f64,
    },
    SafeWithdraw {
        #[arg(long)]
        network: Network,
        signer_address: String,
        withdrawal_address: String,
        withdrawal_utxo: String,
        amount: f64,
        signature: String,
    },
    SendSafeWithdrawal {
        #[arg(long)]
        network: Network,
        signer_address: String,
        withdrawal_address: String,
        withdrawal_utxo: String,
        amount: f64,
        signature: String,
    },
    Status {
        #[arg(long)]
        network: Network,
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
        #[arg(long)]
        network: Network,
        signer_address: String,
        withdrawal_address: String,
        withdrawal_utxo_txid: String,
        withdrawal_utxo_vout: u32,
        withdrawal_index: u32,
        signature: String,
        withdrawal_amount: u64,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    color_eyre::install().expect("Failed to install color-eyre");

    let cli = Cli::parse();

    initialize_logger(cli.verbose);

    match cli.command {
        Commands::Wallet { command } => match command {
            WalletCommands::Create {
                label,
                purpose,
                network,
            } => {
                handle_cli_command!(
                    cli_create_wallet(network, label, purpose),
                    _ => {}
                );
            }
            WalletCommands::Backup {
                destination,
                address,
            } => {
                handle_cli_command!(
                    cli_backup_wallet(&address, &destination),
                    (addr, dest) => {
                        println!(
                            "Backup completed for address {} to destination {}",
                            addr.address_with_prefix(),
                            dest.display()
                        );
                    }
                );
            }
            WalletCommands::ShowMnemonic { address } => {
                let address = TaprootAddressWithPrefix::from_string_with_prefix_unchecked(&address)
                    .print_err()?;
                handle_cli_command!(cli_show_mnemonic(&address), "Mnemonic display completed");
            }
            WalletCommands::ImportMnemonic {
                label,
                purpose,
                network,
            } => {
                handle_cli_command!(
                    cli_import_wallet_from_mnemonic(network, &label, purpose),
                    address => {
                        println!(
                            "Import completed for address: {}",
                            address.address_with_prefix()
                        );
                    }
                );
            }
            WalletCommands::ImportFile { filename, label } => {
                handle_cli_command!(
                    cli_import_wallet_from_file(&filename, label.as_deref()),
                    address => {
                        println!(
                            "Import from file completed for address: {}",
                            address.address_with_prefix()
                        );
                    }
                );
            }
            WalletCommands::ImportPrivateKey {
                label,
                purpose,
                network,
            } => {
                handle_cli_command!(
                    cli_import_wallet_from_private_key(network, &label, purpose),
                    address => {
                        println!(
                            "Import from private key completed for address: {}",
                            address.address_with_prefix()
                        );
                        println!(
                            "{} Note: This wallet was imported from a private key, so no mnemonic phrase is available.",
                            "INFO".yellow()
                        );
                    }
                );
            }
            WalletCommands::VerifyIntegrity => {
                handle_cli_command!(
                    cli_verify_wallet_integrity(),
                    "Wallet integrity verification completed"
                );
            }
            WalletCommands::List => {
                handle_cli_command!(print_all_wallets_with_addresses());
            }
            WalletCommands::ShowPrivateKey { address } => {
                let address = TaprootAddressWithPrefix::from_string_with_prefix_unchecked(&address)
                    .print_err()?;
                handle_cli_command!(
                    cli_show_private_key(&address),
                    "Private key display completed"
                );
            }
        },
        Commands::Deposit { command } => match command {
            DepositCommands::GetDepositAddress {
                recovery_taproot_address,
                citrea_address,
                network,
            } => {
                let config = BridgeCliConfig::try_parse_config(cli.config_file, network).unwrap();
                let citrea_address = parse_citrea_address(&citrea_address)?;
                let recovery_taproot_address = TaprootAddressWithPrefix::from_string_with_prefix(
                    &recovery_taproot_address,
                    config.network,
                )
                .print_err()?;
                handle_cli_command!(async
                    deposit::get_deposit_address(&citrea_address, &recovery_taproot_address, &config),
                    deposit_address => {
                        println!("Deposit address: {}", deposit_address);
                    }
                );
            }
            DepositCommands::SignRecoveryTx {
                recovery_taproot_address,
                evm_address,
                deposit_txid,
                deposit_vout,
                claim_address,
                fee_rate,
                amount,
                network,
            } => {
                let config = BridgeCliConfig::try_parse_config(cli.config_file, network).unwrap();

                let citrea_address = parse_citrea_address(&evm_address)?;
                let recovery_taproot_address = TaprootAddressWithPrefix::from_string_with_prefix(
                    &recovery_taproot_address,
                    config.network,
                )
                .print_err()?;
                let txid = Txid::from_str(&deposit_txid)?;
                let outpoint = OutPoint {
                    txid,
                    vout: deposit_vout,
                };
                let claim_address =
                    BitcoinAddress::from_str(&claim_address)?.require_network(config.network)?;

                fn serialize_and_encode(tx: bitcoin::Transaction) -> String {
                    hex::encode(bitcoin::consensus::serialize(&tx))
                }

                handle_cli_command!(
                    deposit::sign_recovery_tx(
                        &citrea_address,
                        &recovery_taproot_address,
                        &outpoint,
                        &claim_address,
                        fee_rate,
                        amount,
                        &config,
                    ),
                    tx => {
                        println!("Recovery transaction hex: {}", serialize_and_encode(tx));
                    }
                );
            }
            DepositCommands::VerifyRecoveryTx {
                recovery_tx,
                recovery_taproot_address,
                evm_address,
                amount,
                network,
            } => {
                let config = BridgeCliConfig::try_parse_config(cli.config_file, network).unwrap();
                let recovery_tx: Transaction = deserialize(&hex::decode(recovery_tx)?)?;
                let citrea_address = parse_citrea_address(&evm_address)?;
                let recovery_taproot_address = TaprootAddressWithPrefix::from_string_with_prefix(
                    &recovery_taproot_address,
                    config.network,
                )
                .print_err()?;
                handle_cli_command!(
                    deposit::verify_recovery_tx(
                        &recovery_tx,
                        &citrea_address,
                        &recovery_taproot_address,
                        amount,
                        &config,
                    ),
                    (txid, address, amount) => {
                        println!("Recovery transaction verification completed!");
                        println!("Txid: {}", txid);
                        println!("Address: {}", address);
                        println!("Amount: {}", amount);
                    }
                );
            }
            DepositCommands::Status {
                deposit_address,
                network,
            } => {
                let config = BridgeCliConfig::try_parse_config(cli.config_file, network).unwrap();
                let deposit_address = parse_taproot_address(&deposit_address, config.network)?;
                deposit_status(deposit_address, &config).await?;
            }
            DepositCommands::GetDepositParams {
                move_to_vault_txid,
                network,
            } => {
                let config = BridgeCliConfig::try_parse_config(cli.config_file, network).unwrap();
                let move_to_vault_txid = Txid::from_str(&move_to_vault_txid)?;
                handle_cli_command!(async
                    get_deposit_params(&move_to_vault_txid, &config),
                    params => {
                        println!("Deposit parameters hex: {}", hex::encode(params));
                    }
                );
            }
        },
        Commands::Withdrawal { command } => match command {
            WithdrawalCommands::Start {
                signer_address,
                claim_address,
                network,
            } => {
                let config = BridgeCliConfig::try_parse_config(cli.config_file, network).unwrap();

                let signer_address = TaprootAddressWithPrefix::from_string_with_prefix(
                    &signer_address,
                    config.network,
                )
                .print_err()?;

                // Use wrap_err to preserve inner error location and context

                Purpose::should_not_have_purpose(&claim_address).inspect_err(|_| {
                    eprintln!("Invalid claim address: {}", claim_address.bold());
                })?;

                let claim_address =
                    parse_address(&claim_address, config.network).inspect_err(|_| {
                        eprintln!("Invalid claim address: {}", claim_address.bold());
                    })?;

                handle_cli_command!(
                    withdrawal::start_withdrawal(&signer_address, &claim_address, &config),
                    () => {
                        println!("Send exactly 0.00000330 bitcoins to {}", signer_address.address_without_prefix());
                        println!("Then run: withdrawal scan {} {} to scan for UTXOs",
                            signer_address.address_with_prefix(), claim_address);
                    }
                );
            }
            WithdrawalCommands::Scan {
                signer_address,
                claim_address,
                network,
            } => {
                let config = BridgeCliConfig::try_parse_config(cli.config_file, network).unwrap();
                let signer_address = TaprootAddressWithPrefix::from_string_with_prefix(
                    &signer_address,
                    config.network,
                )
                .print_err()?;

                Purpose::should_not_have_purpose(&claim_address).inspect_err(|_| {
                    eprintln!("Invalid claim address: {}", claim_address.bold());
                })?;

                let claim_address = parse_address(&claim_address, config.network)?;
                handle_cli_command!(async
                    withdrawal::scan_withdrawal(&signer_address, &claim_address, &config),
                    mut utxos => {
                        utxos.sort_by_key(|(outpoint, _)| outpoint.txid);

                        let utxos_with_wrong_amount: Vec<_> = utxos.iter().filter(|(_, amount)| *amount != Amount::from_sat(330)).collect();

                        if !utxos_with_wrong_amount.is_empty() {
                            eprintln!("{} The following UTXOs have amounts different than 330 sats. They will be ignored for withdrawal operations.", "WARNING".bold());
                            for (outpoint, amount) in utxos_with_wrong_amount {
                                eprintln!(" - OutPoint: {}, Amount: {}", outpoint, amount);
                            }
                            eprintln!("Please ensure you send exactly 330 sats to the signer address for each withdrawal operation.");

                            // sleep for 2 seconds to ensure user sees the warning
                            std::thread::sleep(std::time::Duration::from_secs(2));

                            println!();
                        }

                        utxos.retain(|(_, amount)| *amount == Amount::from_sat(330));

                        if utxos.is_empty() {
                            eprintln!("No UTXOs found. Please send 330 sats first using 'withdrawal start' command");
                        } else if utxos.len() == 1 {
                            let (outpoint, _) = &utxos[0];
                            println!("run \nwithdrawal generate-withdrawal-signature {} {} {} {}",
                                &signer_address.address_with_prefix(), claim_address, outpoint, config.optimistic_withdrawal_amount.to_btc());
                            println!("inside your airgapped pc");
                        } else {
                            println!("{} Multiple UTXOs found, we advise to use one UTXO for one withdrawal operation", "WARNING".bold());
                            println!(
                                "{} For your security: Use a unique signer address for each withdrawal.",
                                "IMPORTANT NOTICE!".bold()
                            );
                            for (outpoint, _) in utxos.iter() {
                                println!("\nRun: \nwithdrawal generate-withdrawal-signature {} {} {} {}",
                                    &signer_address.address_with_prefix(), claim_address, outpoint, config.optimistic_withdrawal_amount.to_btc());
                            }
                        }
                    }
                );
            }
            WithdrawalCommands::GenerateWithdrawalSignature {
                signer_address,
                withdrawal_address,
                withdrawal_utxo,
                amount,
                network,
            } => {
                let signer_address =
                    TaprootAddressWithPrefix::from_string_with_prefix(&signer_address, network)
                        .print_err()?;

                Purpose::should_not_have_purpose(&withdrawal_address).inspect_err(|_| {
                    eprintln!("Invalid withdrawal address: {}", withdrawal_address.bold());
                })?;

                let claim_address = parse_address(&withdrawal_address, network)?;
                let withdrawal_outpoint = OutPoint::from_str(&withdrawal_utxo)?;
                let amount = Amount::from_btc(amount)?;
                fn serialize_and_encode(signature: Signature) -> String {
                    hex::encode(signature.serialize())
                }

                handle_cli_command!(
                    withdrawal::generate_withdrawal_signature(
                        &signer_address,
                        &claim_address,
                        &withdrawal_outpoint,
                        &amount,
                        network,
                    ),
                    signature => {
                        println!(
                            "Withdrawal signature hex: {}",
                            serialize_and_encode(signature)
                        );
                    }
                );
            }
            WithdrawalCommands::SafeWithdraw {
                signer_address,
                withdrawal_address,
                withdrawal_utxo,
                amount,
                signature,
                network,
            } => {
                let config = BridgeCliConfig::try_parse_config(cli.config_file, network).unwrap();
                let signer_address = TaprootAddressWithPrefix::from_string_with_prefix(
                    &signer_address,
                    config.network,
                )
                .print_err()?;

                Purpose::should_not_have_purpose(&withdrawal_address).inspect_err(|_| {
                    eprintln!("Invalid withdrawal address: {}", withdrawal_address.bold());
                })?;

                let withdrawal_address = parse_address(&withdrawal_address, config.network)?;
                let withdrawal_outpoint = OutPoint::from_str(&withdrawal_utxo)?;
                let withdrawal_amount = Amount::from_btc(amount)?;
                let sig = bitcoin::taproot::Signature::from_slice(&hex::decode(signature)?)?;
                handle_cli_command!(async
                    withdrawal::safe_withdraw(
                        &signer_address,
                        &withdrawal_address,
                        &withdrawal_outpoint,
                        &withdrawal_amount,
                        &sig,
                        &config,
                    ),
                    withdrawal_ui_url => {
                        println!(
                            "\n{} Opening withdrawal page {withdrawal_ui_url} in your default browser...",
                            "INFO".green().bold()
                        );
                    }
                );
            }
            WithdrawalCommands::SendSafeWithdrawal {
                signer_address,
                withdrawal_address,
                withdrawal_utxo,
                amount,
                signature,
                network,
            } => {
                let config = BridgeCliConfig::try_parse_config(cli.config_file, network).unwrap();
                let signer_address = TaprootAddressWithPrefix::from_string_with_prefix(
                    &signer_address,
                    config.network,
                )
                .print_err()?;

                Purpose::should_not_have_purpose(&withdrawal_address).inspect_err(|_| {
                    eprintln!("Invalid withdrawal address: {}", withdrawal_address.bold());
                })?;

                let withdrawal_address = parse_address(&withdrawal_address, config.network)?;
                let withdrawal_outpoint = OutPoint::from_str(&withdrawal_utxo)?;
                let withdrawal_amount = Amount::from_btc(amount)?;
                let sig = bitcoin::taproot::Signature::from_slice(&hex::decode(signature)?)?;
                handle_cli_command!(async
                    withdrawal::send_safe_withdrawal(
                        &signer_address,
                        &withdrawal_address,
                        &withdrawal_outpoint,
                        &withdrawal_amount,
                        &sig,
                        &config,
                    ),
                    result => {
                        println!("Safe withdrawal transaction sent!");
                        println!("Transaction Receipt: {:#?}", result);
                    }
                );
            }
            WithdrawalCommands::Status {
                withdrawal_index,
                network,
            } => {
                let config = BridgeCliConfig::try_parse_config(cli.config_file, network).unwrap();
                withdrawal_status(withdrawal_index, &config).await?;
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
                withdrawal_amount,
                network,
            } => {
                let config = BridgeCliConfig::try_parse_config(cli.config_file, network).unwrap();
                send_withdrawal_signatures(
                    &signer_address,
                    &withdrawal_address,
                    &withdrawal_utxo_txid,
                    withdrawal_utxo_vout,
                    withdrawal_index,
                    &signature,
                    &config,
                    withdrawal_amount,
                )
                .await?;
                println!("Withdrawal signatures sent successfully to operators");
            }
        },
    }
    Ok(())
}
