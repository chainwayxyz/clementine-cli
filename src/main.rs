use bitcoin::{
    Amount, Network, OutPoint, Transaction, Txid, consensus::deserialize, taproot::Signature,
};
use clap::{Parser, Subcommand, ValueEnum};
use clementine_cli::cli::cli_scan_withdrawals;
use clementine_cli::errors::PrintErr;
use clementine_cli::wallet::should_not_have_purpose;
use clementine_cli::{
    BitcoinAddress, broadcast_recovery_tx,
    cli::{
        cli_backup_wallet, cli_create_wallet, cli_get_deposit_address, cli_import_wallet_from_file,
        cli_import_wallet_from_mnemonic, cli_import_wallet_from_private_key, cli_show_mnemonic,
        cli_show_private_key, cli_start_withdrawal, cli_verify_wallet_integrity,
        deposit_create_signed_recovery_tx, deposit_status, send_withdrawal_signatures,
        withdrawal_status,
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

/// Local copy of [`bitcoin::Network`], just to implement [`ValueEnum`].
#[derive(Copy, Clone, Debug, ValueEnum)]
enum CliNetwork {
    Bitcoin,
    Testnet,
    Testnet4,
    Signet,
    Regtest,
}

impl From<CliNetwork> for Network {
    fn from(value: CliNetwork) -> Self {
        match value {
            CliNetwork::Bitcoin => Network::Bitcoin,
            CliNetwork::Testnet => Network::Testnet,
            CliNetwork::Testnet4 => Network::Testnet4,
            CliNetwork::Signet => Network::Signet,
            CliNetwork::Regtest => Network::Regtest,
        }
    }
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
    /// Manage a deposit process.
    Deposit {
        #[command(subcommand)]
        command: DepositCommands,
    },
    /// Manage a withdrawal process.
    Withdraw {
        #[command(subcommand)]
        command: WithdrawCommands,
    },
}

#[derive(Subcommand)]
enum WalletCommands {
    /// Create a new wallet with mnemonic display.
    Create {
        /// Bitcoin network (required for this subcommand)
        #[arg(long, default_value_t = CliNetwork::Bitcoin, value_enum)]
        network: CliNetwork,
        /// Label for the wallet file
        label: String,
        /// Purpose for the wallet.
        purpose: Purpose,
    },
    /// Backup a wallet to specified destination.
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
    /// Show private key with interactive terminal.
    ShowPrivateKey {
        /// Wallet address to show private key for
        address: String,
    },
    /// Import wallet using secure mnemonic input.
    ImportMnemonic {
        /// Bitcoin network (required for this subcommand)
        #[arg(long, default_value_t = CliNetwork::Bitcoin, value_enum)]
        network: CliNetwork,
        /// Label for the imported wallet
        label: String,
        /// Purpose for the imported wallet
        purpose: Purpose,
    },
    /// Import wallet using secure private key input.
    ImportPrivateKey {
        /// Bitcoin network (required for this subcommand)
        #[arg(long, default_value_t = CliNetwork::Bitcoin, value_enum)]
        network: CliNetwork,
        /// Label for the imported wallet
        label: String,
        /// Purpose for the imported wallet
        purpose: Purpose,
    },
    /// Import wallet from a backup file.
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
    /// Generate a deposit address for the given Citrea and recovery addresses.
    GetDepositAddress {
        /// Bitcoin network (required for this subcommand)
        #[arg(long, default_value_t = CliNetwork::Bitcoin, value_enum)]
        network: CliNetwork,
        recovery_taproot_address: String,
        citrea_address: String,
    },
    /// Creates a raw Bitcoin transaction that can collect funds back to the
    /// given address
    CreateSignedRecoveryTx {
        /// Recovery taproot address, which deposit has been made
        recovery_taproot_address: String,
        /// Your Citrea address, which deposit has been made
        citrea_address: String,
        /// UTXO outpoint of the deposit transaction
        deposit_utxo_outpoint: String,
        /// Your Bitcoin address, which will collect the 10 BTC (- fees)
        claim_address: String,
        /// Fee rate to be used when creating the recovery tx
        fee_rate: u64,
        /// Amount in BTC (e.g., 0.1 for 0.1 BTC)
        amount: f64,
        /// Bitcoin network (required for this subcommand)
        #[arg(long, default_value_t = CliNetwork::Bitcoin, value_enum)]
        network: CliNetwork,
    },
    /// Verify a recovery transaction before broadcasting.
    VerifyRecoveryTx {
        recovery_tx: String,
        recovery_taproot_address: String,
        evm_address: String,
        /// Amount in BTC (e.g., 0.1 for 0.1 BTC)
        #[arg(long)]
        amount: Option<f64>,
        #[arg(long, default_value_t = CliNetwork::Bitcoin, value_enum)]
        network: CliNetwork,
    },
    // Check the status of a deposit.
    Status {
        deposit_address: String,
        #[arg(long, default_value_t = CliNetwork::Bitcoin, value_enum)]
        network: CliNetwork,
    },
    /// Broadcasts raw recovery transaction to Bitcoin network either by Mempool API or Bitcoin RPC
    BroadcastRecoveryTx {
        /// Hex encoded raw transaction.
        raw_tx: String,
        #[arg(long, default_value_t = CliNetwork::Bitcoin, value_enum)]
        network: CliNetwork,
    },
    /// Get deposit parameters for a move-to-vault transaction.
    GetDepositParams {
        move_to_vault_txid: String,
        #[arg(long, default_value_t = CliNetwork::Bitcoin, value_enum)]
        network: CliNetwork,
    },
}

#[derive(Subcommand)]
enum WithdrawCommands {
    /// Start a withdrawal process and get instructions for sending funds.
    Start {
        #[arg(long, default_value_t = CliNetwork::Bitcoin, value_enum)]
        network: CliNetwork,
        signer_address: String,
        claim_address: String,
    },
    /// Scan for UTXOs to use in withdrawal.
    Scan {
        #[arg(long, default_value_t = CliNetwork::Bitcoin, value_enum)]
        network: CliNetwork,
        signer_address: String,
        claim_address: String,
    },
    /// Generate a withdrawal signature (for air-gapped use).
    GenerateWithdrawalSignature {
        #[arg(long, default_value_t = CliNetwork::Bitcoin, value_enum)]
        network: CliNetwork,
        signer_address: String,
        withdrawal_address: String,
        withdrawal_utxo_outpoint: String,
        amount: f64,
    },
    /// Initiate a safe withdrawal by opening browser interface.
    SafeWithdraw {
        #[arg(long, default_value_t = CliNetwork::Bitcoin, value_enum)]
        network: CliNetwork,
        signer_address: String,
        withdrawal_address: String,
        withdrawal_utxo_outpoint: String,
        amount: f64,
        signature: String,
    },
    /// Send a safe withdrawal transaction.
    SendSafeWithdrawal {
        #[arg(long, default_value_t = CliNetwork::Bitcoin, value_enum)]
        network: CliNetwork,
        signer_address: String,
        withdrawal_address: String,
        withdrawal_utxo_outpoint: String,
        amount: f64,
        signature: String,
    },
    /// Check the status of a withdrawal.
    Status {
        #[arg(long, default_value_t = CliNetwork::Bitcoin, value_enum)]
        network: CliNetwork,
        withdrawal_index: u32,
    },
    /// Generate operator withdrawal signatures.
    GenerateOperatorWithdrawalSignatures {
        signer_address: String,
        withdrawal_address: String,
        withdrawal_utxo_outpoint: String,
        withdrawal_amount: u64,
    },
    /// Send withdrawal signatures to operators.
    SendWithdrawalSignaturesToOperators {
        #[arg(long, default_value_t = CliNetwork::Bitcoin, value_enum)]
        network: CliNetwork,
        signer_address: String,
        withdrawal_address: String,
        withdrawal_utxo_outpoint: String,
        withdrawal_amount: f64,
        signature: String,
        withdrawal_index: u32,
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
                    cli_create_wallet(network.into(), label, purpose),
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
                    cli_import_wallet_from_mnemonic(network.into(), &label, purpose),
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
                    cli_import_wallet_from_private_key(network.into(), &label, purpose),
                    address => {
                        println!(
                            "Import from private key completed for address: {}",
                            address.address_with_prefix()
                        );
                        println!(
                            "{} Note: This wallet was imported from a private key, so no mnemonic phrase is available.",
                            "INFO".bold()
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
                let config =
                    BridgeCliConfig::try_parse_config(cli.config_file, network.into()).unwrap();
                let citrea_address = parse_citrea_address(&citrea_address)?;
                let recovery_taproot_address = TaprootAddressWithPrefix::from_string_with_prefix(
                    &recovery_taproot_address,
                    config.network,
                )
                .print_err()?;
                handle_cli_command!(async
                                    cli_get_deposit_address(&citrea_address, &recovery_taproot_address, &config),
                                    deposit_address => {
                                        println! ("Deposit address: {}", deposit_address.to_string ().bold());
                println!("{} Send exactly 10 BTC to the address above to initiate the deposit.", "INFO".bold());
                println! ("For Bitcoin Core users, you can send your deposit using the following command (add any parameters as needed): ");
                println! ("bitcoin-cli sendtoaddress \"{}\" 10", deposit_address.to_string());
                                    }
                                );
            }
            DepositCommands::CreateSignedRecoveryTx {
                recovery_taproot_address,
                citrea_address,
                deposit_utxo_outpoint,
                claim_address,
                fee_rate,
                amount,
                network,
            } => {
                let config =
                    BridgeCliConfig::try_parse_config(cli.config_file, network.into()).unwrap();
                let recovery_taproot_address = TaprootAddressWithPrefix::from_string_with_prefix(
                    &recovery_taproot_address,
                    config.network,
                )
                .print_err()?;

                let citrea_address = parse_citrea_address(&citrea_address)?;
                let deposit_utxo_outpoint = OutPoint::from_str(&deposit_utxo_outpoint)?;
                let claim_address =
                    BitcoinAddress::from_str(&claim_address)?.require_network(config.network)?;

                handle_cli_command!(
                    deposit_create_signed_recovery_tx(
                        &citrea_address,
                        &recovery_taproot_address,
                        &deposit_utxo_outpoint,
                        &claim_address,
                        fee_rate,
                        amount,
                        &config,
                    )
                    .await
                );
            }
            DepositCommands::VerifyRecoveryTx {
                recovery_tx,
                recovery_taproot_address,
                evm_address,
                amount,
                network,
            } => {
                let config =
                    BridgeCliConfig::try_parse_config(cli.config_file, network.into()).unwrap();
                let recovery_tx: Transaction = deserialize(&hex::decode(recovery_tx)?)?;
                let citrea_address = parse_citrea_address(&evm_address)?;
                let recovery_taproot_address = TaprootAddressWithPrefix::from_string_with_prefix(
                    &recovery_taproot_address,
                    config.network,
                )
                .print_err()?;
                handle_cli_command!(
                    deposit::verify_recovery_tx(
                        deposit::VerifyRecoveryTxParams {
                            recovery_tx,
                            citrea_address,
                            recovery_taproot_address,
                            amount,
                        },
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
                let config =
                    BridgeCliConfig::try_parse_config(cli.config_file, network.into()).unwrap();
                let deposit_address = parse_taproot_address(&deposit_address, config.network)?;
                deposit_status(deposit_address, &config).await?;
            }
            DepositCommands::BroadcastRecoveryTx { raw_tx, network } => {
                let config =
                    BridgeCliConfig::try_parse_config(cli.config_file, network.into()).unwrap();

                handle_cli_command!(async broadcast_recovery_tx(&config, raw_tx), txid => {
                    println!("{}", txid);
                });
            }
            DepositCommands::GetDepositParams {
                move_to_vault_txid,
                network,
            } => {
                let config =
                    BridgeCliConfig::try_parse_config(cli.config_file, network.into()).unwrap();
                let move_to_vault_txid = Txid::from_str(&move_to_vault_txid)?;
                handle_cli_command!(async
                    get_deposit_params(&move_to_vault_txid, &config),
                    params => {
                        println!("Deposit parameters hex: {}", hex::encode(params));
                    }
                );
            }
        },
        Commands::Withdraw { command } => match command {
            WithdrawCommands::Start {
                signer_address,
                claim_address,
                network,
            } => {
                let config =
                    BridgeCliConfig::try_parse_config(cli.config_file, network.into()).unwrap();

                let signer_address = TaprootAddressWithPrefix::from_string_with_prefix(
                    &signer_address,
                    config.network,
                )
                .print_err()?;

                // Use wrap_err to preserve inner error location and context

                should_not_have_purpose(&claim_address).inspect_err(|_| {
                    eprintln!("Invalid claim address: {}", claim_address.bold());
                })?;

                let claim_address =
                    parse_address(&claim_address, config.network).inspect_err(|_| {
                        eprintln!("Invalid claim address: {}", claim_address.bold());
                    })?;

                handle_cli_command!(async
                    cli_start_withdrawal(&signer_address, &claim_address, &config),
                    _result => {
                        println!("Send exactly {} sats to {}", clementine_cli::WITHDRAWAL_UTXO_AMOUNT, signer_address.address_without_prefix());
                        println!("You can use:");
                        println!("bitcoin-cli sendtoaddress \"{}\" 0.00000{}",
                            signer_address.address_without_prefix(), clementine_cli::WITHDRAWAL_UTXO_AMOUNT.to_sat());
                        println!("or a similar command from a wallet you are using");
                        println!("Then run:");
                        println!("clementine-cli withdraw scan --network {} {} {}",
                            config.network, signer_address.address_with_prefix(), claim_address);
                        println!("to scan UTXOs that can be used for the withdrawal operation");
                    }
                );
            }
            WithdrawCommands::Scan {
                signer_address,
                claim_address,
                network,
            } => {
                let config =
                    BridgeCliConfig::try_parse_config(cli.config_file, network.into()).unwrap();
                let signer_address = TaprootAddressWithPrefix::from_string_with_prefix(
                    &signer_address,
                    config.network,
                )
                .print_err()?;

                should_not_have_purpose(&claim_address).inspect_err(|_| {
                    eprintln!("Invalid claim address: {}", claim_address.bold());
                })?;

                let claim_address = parse_address(&claim_address, config.network)?;
                handle_cli_command!(
                    cli_scan_withdrawals(&signer_address, &claim_address, &config).await,
                    _ => { }
                )
            }
            WithdrawCommands::GenerateWithdrawalSignature {
                signer_address,
                withdrawal_address,
                withdrawal_utxo_outpoint,
                amount,
                network,
            } => {
                let signer_address = TaprootAddressWithPrefix::from_string_with_prefix(
                    &signer_address,
                    network.into(),
                )
                .print_err()?;

                should_not_have_purpose(&withdrawal_address).inspect_err(|_| {
                    eprintln!("Invalid withdrawal address: {}", withdrawal_address.bold());
                })?;

                let claim_address = parse_address(&withdrawal_address, network.into())?;
                let withdrawal_outpoint = OutPoint::from_str(&withdrawal_utxo_outpoint)?;
                let amount = Amount::from_btc(amount)?;
                fn serialize_and_encode(signature: Signature) -> String {
                    hex::encode(signature.serialize())
                }
                let network: Network = network.into();

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
                        println!("Now run:");
                        println!("clementine-cli withdraw safe-withdraw --network {} {} {} {} {} {}",
                            network, &signer_address.address_with_prefix(), withdrawal_address, withdrawal_utxo_outpoint, amount.to_btc(), serialize_and_encode(signature));
                        println!("on your online device to initiate withdrawal process on the Citrea network");
                    }
                );
            }
            WithdrawCommands::SafeWithdraw {
                signer_address,
                withdrawal_address,
                withdrawal_utxo_outpoint,
                amount,
                signature,
                network,
            } => {
                let config =
                    BridgeCliConfig::try_parse_config(cli.config_file, network.into()).unwrap();
                let signer_address = TaprootAddressWithPrefix::from_string_with_prefix(
                    &signer_address,
                    config.network,
                )
                .print_err()?;

                should_not_have_purpose(&withdrawal_address).inspect_err(|_| {
                    eprintln!("Invalid withdrawal address: {}", withdrawal_address.bold());
                })?;

                let withdrawal_address = parse_address(&withdrawal_address, config.network)?;
                let withdrawal_outpoint = OutPoint::from_str(&withdrawal_utxo_outpoint)?;
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
                            "INFO".bold()
                        );
                    }
                );
            }
            WithdrawCommands::SendSafeWithdrawal {
                signer_address,
                withdrawal_address,
                withdrawal_utxo_outpoint,
                amount,
                signature,
                network,
            } => {
                let config =
                    BridgeCliConfig::try_parse_config(cli.config_file, network.into()).unwrap();
                let signer_address = TaprootAddressWithPrefix::from_string_with_prefix(
                    &signer_address,
                    config.network,
                )
                .print_err()?;

                should_not_have_purpose(&withdrawal_address).inspect_err(|_| {
                    eprintln!("Invalid withdrawal address: {}", withdrawal_address.bold());
                })?;

                let withdrawal_address = parse_address(&withdrawal_address, config.network)?;
                let withdrawal_outpoint = OutPoint::from_str(&withdrawal_utxo_outpoint)?;
                let withdrawal_amount = Amount::from_btc(amount)?;
                let sig = bitcoin::taproot::Signature::from_slice(&hex::decode(signature)?)?;
                handle_cli_command!(async
                    withdrawal::send_safe_withdrawal(
                        withdrawal::SafeWithdrawalParams {
                            signer_address,
                            withdrawal_address,
                            withdrawal_outpoint,
                            withdrawal_amount,
                            signature: sig,
                        },
                        &config,
                    ),
                    result => {
                        println!("Safe withdrawal transaction sent!");
                        println!("Transaction Receipt: {:#?}", result);
                    }
                );
            }
            WithdrawCommands::Status {
                withdrawal_index,
                network,
            } => {
                let config =
                    BridgeCliConfig::try_parse_config(cli.config_file, network.into()).unwrap();
                withdrawal_status(withdrawal_index, &config).await?;
            }
            WithdrawCommands::GenerateOperatorWithdrawalSignatures {
                withdrawal_address,
                signer_address,
                withdrawal_utxo_outpoint,
                withdrawal_amount,
            } => {
                unimplemented!(
                    "withdrawal.generate_operator_withdrawal_signatures: {} {} {} {}",
                    withdrawal_address,
                    signer_address,
                    withdrawal_utxo_outpoint,
                    withdrawal_amount
                );
            }
            WithdrawCommands::SendWithdrawalSignaturesToOperators {
                signer_address,
                withdrawal_address,
                withdrawal_utxo_outpoint,
                withdrawal_index,
                signature,
                withdrawal_amount,
                network,
            } => {
                let config =
                    BridgeCliConfig::try_parse_config(cli.config_file, network.into()).unwrap();
                send_withdrawal_signatures(
                    &signer_address,
                    &withdrawal_address,
                    &withdrawal_utxo_outpoint,
                    withdrawal_amount,
                    &signature,
                    &config,
                    withdrawal_index,
                )
                .await?;
                println!("Withdrawal signatures sent successfully to operators");
            }
        },
    }
    Ok(())
}
