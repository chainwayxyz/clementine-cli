use bitcoin::{Network, OutPoint, Transaction, Txid, consensus::deserialize, taproot::Signature};
use clap::{Parser, Subcommand, ValueEnum};
use clementine_cli::cli::{
    cli_backup_wallet, cli_create_wallet, cli_generate_withdrawal_signatures,
    cli_get_deposit_address, cli_import_wallet_from_file, cli_import_wallet_from_mnemonic,
    cli_import_wallet_from_private_key, cli_scan_withdrawals, cli_show_mnemonic,
    cli_show_private_key, cli_start_withdrawal, cli_verify_wallet_integrity,
    deposit_create_signed_recovery_tx, deposit_status, send_withdrawal_signature,
    withdrawal_status,
};
use clementine_cli::errors::PrintErr;
use clementine_cli::wallet::should_not_have_purpose;
use clementine_cli::{
    BitcoinAddress, broadcast_recovery_tx,
    config::BridgeCliConfig,
    deposit, get_deposit_params, handle_cli_command, parse_citrea_address,
    print_all_wallets_with_addresses,
    structs::TaprootAddressWithPrefix,
    wallet::{Purpose, parse_address, parse_taproot_address},
    withdraw,
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
    Testnet4,
    Signet,
    Regtest,
}

impl From<CliNetwork> for Network {
    fn from(value: CliNetwork) -> Self {
        match value {
            CliNetwork::Bitcoin => Network::Bitcoin,
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

fn get_bitcoin_cli_command(config: &BridgeCliConfig) -> String {
    let mut command = "bitcoin-cli".to_string();

    match config.network {
        Network::Bitcoin => (),
        Network::Testnet4 => command.push_str(" -testnet4"),
        Network::Signet => command.push_str(" -signet"),
        Network::Regtest => command.push_str(" -regtest"),
        Network::Testnet => panic!("Statically not possible to get here"),
    }

    command.push_str(" -rpcport=<rpcport>");
    command.push_str(" -rpcuser=<rpcuser>");
    command.push_str(" -rpcpassword=<rpcpassword>");
    command.push_str(" -rpcwallet=<rpcwallet>");

    command
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
        /// Bitcoin network to use
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
        /// Bitcoin network to use
        #[arg(long, default_value_t = CliNetwork::Bitcoin, value_enum)]
        network: CliNetwork,
        /// Label for the imported wallet
        label: String,
        /// Purpose for the imported wallet
        purpose: Purpose,
    },
    /// Import wallet using secure private key input.
    ImportPrivateKey {
        /// Bitcoin network to use
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
        /// Bitcoin network to use
        #[arg(long, default_value_t = CliNetwork::Bitcoin, value_enum)]
        network: CliNetwork,
        /// Recovery taproot address (must be a Clementine deposit address, dep-prefixed, taproot)
        recovery_taproot_address: String,
        /// Citrea address (EVM address to receive bridged BTC)
        citrea_address: String,
    },
    /// Creates a raw Bitcoin transaction that can collect funds back to the given address.
    CreateSignedRecoveryTx {
        /// Recovery taproot address (must be a Clementine deposit address, dep-prefixed, taproot)
        recovery_taproot_address: String,
        /// Citrea address (EVM address to receive bridged BTC)
        citrea_address: String,
        /// UTXO outpoint of the deposit transaction (format: <txid>:<vout>)
        deposit_utxo_outpoint: String,
        /// Bitcoin address to collect the recovered funds
        destination_address: String,
        /// Fee rate to use for the recovery transaction (in sats/vB)
        fee_rate: u64,
        /// Deposited output amount in BTC (e.g., 0.1 for 0.1 BTC)
        amount: f64,
        /// Bitcoin network to use
        #[arg(long, default_value_t = CliNetwork::Bitcoin, value_enum)]
        network: CliNetwork,
    },
    /// Verify a recovery transaction before broadcasting.
    VerifyRecoveryTx {
        /// Raw recovery transaction (hex-encoded)
        recovery_tx: String,
        /// Recovery taproot address (must be a Clementine deposit address, dep-prefixed, taproot)
        recovery_taproot_address: String,
        /// Citrea address (EVM address to receive bridged BTC)
        evm_address: String,
        /// Deposited output amount in BTC (e.g., 0.1 for 0.1 BTC)
        #[arg(long)]
        amount: Option<f64>,
        /// Bitcoin network to use
        #[arg(long, default_value_t = CliNetwork::Bitcoin, value_enum)]
        network: CliNetwork,
    },
    /// Check the status of a deposit.
    Status {
        /// Deposit address (taproot address funds were sent to)
        deposit_address: String,
        /// Bitcoin network to use
        #[arg(long, default_value_t = CliNetwork::Bitcoin, value_enum)]
        network: CliNetwork,
    },
    /// Broadcasts raw recovery transaction to Bitcoin network either by Mempool API or Bitcoin RPC.
    BroadcastRecoveryTx {
        /// Raw transaction to broadcast (hex-encoded)
        raw_tx: String,
        /// Bitcoin network to use
        #[arg(long, default_value_t = CliNetwork::Bitcoin, value_enum)]
        network: CliNetwork,
    },
    /// Get deposit parameters for a move-to-vault transaction.
    GetDepositParams {
        /// Move-to-vault transaction ID (txid)
        move_to_vault_txid: String,
        /// Bitcoin network to use
        #[arg(long, default_value_t = CliNetwork::Bitcoin, value_enum)]
        network: CliNetwork,
    },
}

#[derive(Subcommand)]
enum WithdrawCommands {
    /// Start a withdrawal process and get instructions for sending funds.
    Start {
        /// Bitcoin network to use
        #[arg(long, default_value_t = CliNetwork::Bitcoin, value_enum)]
        network: CliNetwork,
        /// Clementine wallet address for signing withdrawals (wit-prefixed, taproot)
        signer_address: String,
        /// Destination address for withdrawn BTC
        destination_address: String,
    },
    /// Scan for UTXOs to use in withdrawal.
    Scan {
        /// Bitcoin network to use
        #[arg(long, default_value_t = CliNetwork::Bitcoin, value_enum)]
        network: CliNetwork,
        /// Clementine wallet address for signing withdrawals (wit-prefixed, taproot)
        signer_address: String,
        /// Destination address for withdrawn BTC
        destination_address: String,
    },
    /// Generate a withdrawal signature (for air-gapped use).
    GenerateWithdrawalSignatures {
        /// Bitcoin network to use
        #[arg(long, default_value_t = CliNetwork::Bitcoin, value_enum)]
        network: CliNetwork,
        /// Clementine wallet address for signing withdrawals (wit-prefixed, taproot)
        signer_address: String,
        /// Destination address for withdrawn BTC
        destination_address: String,
        /// Withdrawal UTXO outpoint (format: <txid>:<vout>)
        withdrawal_utxo_outpoint: String,
    },
    /// Initiate a safe withdrawal by opening browser interface.
    SafeWithdraw {
        /// Bitcoin network to use
        #[arg(long, default_value_t = CliNetwork::Bitcoin, value_enum)]
        network: CliNetwork,
        /// Clementine wallet address for signing withdrawals (wit-prefixed, taproot)
        signer_address: String,
        /// Destination address for withdrawn BTC
        destination_address: String,
        /// Withdrawal UTXO outpoint (format: <txid>:<vout>)
        withdrawal_utxo_outpoint: String,
        /// Withdrawal signature (hex-encoded)
        signature: String,
    },
    /// Send a safe withdrawal transaction directly to the bridge contract.
    SendSafeWithdrawal {
        /// Bitcoin network to use
        #[arg(long, default_value_t = CliNetwork::Bitcoin, value_enum)]
        network: CliNetwork,
        /// Clementine wallet address for signing withdrawals (wit-prefixed, taproot)
        signer_address: String,
        /// Destination address for withdrawn BTC
        destination_address: String,
        /// Withdrawal UTXO outpoint (format: <txid>:<vout>)
        withdrawal_utxo_outpoint: String,
        /// Withdrawal signature (hex-encoded)
        signature: String,
    },
    /// Check the status of a withdrawal.
    Status {
        /// Bitcoin network to use
        #[arg(long, default_value_t = CliNetwork::Bitcoin, value_enum)]
        network: CliNetwork,
        /// Withdrawal UTXO outpoint (format: <txid>:<vout>)
        withdrawal_utxo: String,
    },
    /// Send withdrawal signatures to operators.
    SendWithdrawalSignatureToOperators {
        /// Bitcoin network to use
        #[arg(long, default_value_t = CliNetwork::Bitcoin, value_enum)]
        network: CliNetwork,
        /// Clementine wallet address for signing withdrawals (wit-prefixed, taproot)
        signer_address: String,
        /// Destination address for withdrawn BTC
        destination_address: String,
        /// Withdrawal UTXO outpoint (format: <txid>:<vout>)
        withdrawal_utxo_outpoint: String,
        /// Withdrawal signature (hex-encoded)
        signature: String,
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
                        println!("Deposit address: {}", deposit_address.to_string ().bold());
                        println!("{} Send exactly {} BTC to the address above to initiate the deposit.", "INFO".bold(), config.bridge_amount.to_btc());
                        println!("For Bitcoin Core users, you can send your deposit using the following command (add any parameters as needed): ");
                        println!("{} sendtoaddress {} {}", get_bitcoin_cli_command(&config), deposit_address.to_string(), config.bridge_amount.to_btc());
                    }
                );
            }
            DepositCommands::CreateSignedRecoveryTx {
                recovery_taproot_address,
                citrea_address,
                deposit_utxo_outpoint,
                destination_address,
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
                let destination_address = BitcoinAddress::from_str(&destination_address)?
                    .require_network(config.network)?;

                handle_cli_command!(
                    deposit_create_signed_recovery_tx(
                        &citrea_address,
                        &recovery_taproot_address,
                        &deposit_utxo_outpoint,
                        &destination_address,
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
                destination_address,
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

                should_not_have_purpose(&destination_address).inspect_err(|_| {
                    eprintln!(
                        "Invalid destination address: {}",
                        destination_address.bold()
                    );
                })?;

                let destination_address = parse_address(&destination_address, config.network)
                    .inspect_err(|_| {
                        eprintln!(
                            "Invalid destination address: {}",
                            destination_address.bold()
                        );
                    })?;

                handle_cli_command!(async
                    cli_start_withdrawal(&signer_address, &destination_address, &config),
                    _result => {
                        println!("Send exactly {} sats to {}", config.dust_utxo_amount.to_sat(), signer_address.address_without_prefix());
                        println!("You can use:");
                        println!("{} sendtoaddress {} {}",
                            get_bitcoin_cli_command(&config),
                            signer_address.address_without_prefix(), config.dust_utxo_amount.to_btc());
                        println!("or a similar command from a wallet you are using");
                        println!("Then run:");
                        println!("clementine-cli withdraw scan --network {} {} {}",
                            config.network, signer_address.address_with_prefix(), destination_address);
                        println!("to scan UTXOs that can be used for the withdrawal operation");
                        println!();
                        println!("{} If your wallet cannot send exactly {} sats, you may send a higher supported amount. Be sure to update the config to match the amount you actually sent before proceeding.", "WARNING".bold(), config.dust_utxo_amount.to_sat());
                    }
                );
            }
            WithdrawCommands::Scan {
                signer_address,
                destination_address,
                network,
            } => {
                let config =
                    BridgeCliConfig::try_parse_config(cli.config_file, network.into()).unwrap();
                let signer_address = TaprootAddressWithPrefix::from_string_with_prefix(
                    &signer_address,
                    config.network,
                )
                .print_err()?;

                should_not_have_purpose(&destination_address).inspect_err(|_| {
                    eprintln!(
                        "Invalid destination address: {}",
                        destination_address.bold()
                    );
                })?;

                let destination_address = parse_address(&destination_address, config.network)?;
                handle_cli_command!(
                    cli_scan_withdrawals(&signer_address, &destination_address, &config).await,
                    _ => { }
                )
            }
            WithdrawCommands::GenerateWithdrawalSignatures {
                signer_address,
                destination_address,
                withdrawal_utxo_outpoint,
                network,
            } => {
                let config =
                    BridgeCliConfig::try_parse_config(cli.config_file, network.into()).unwrap();
                let signer_address = TaprootAddressWithPrefix::from_string_with_prefix(
                    &signer_address,
                    network.into(),
                )
                .print_err()?;

                should_not_have_purpose(&destination_address).inspect_err(|_| {
                    eprintln!("Invalid withdrawal address: {}", destination_address.bold());
                })?;

                let destination_address = parse_address(&destination_address, network.into())?;
                let withdrawal_outpoint = OutPoint::from_str(&withdrawal_utxo_outpoint)?;
                fn serialize_and_encode(signature: Signature) -> String {
                    hex::encode(signature.serialize())
                }
                let network: Network = network.into();

                handle_cli_command!(
                    cli_generate_withdrawal_signatures(
                        &signer_address,
                        &destination_address,
                        &withdrawal_outpoint,
                        &config.optimistic_withdrawal_amount,
                        &config.operator_withdrawal_amount,
                        &config,
                    ),
                    (optimistic_signature, operator_signature) => {
                        println!();
                        println!(
                            "Optimistic withdrawal signature hex: {}",
                            serialize_and_encode(optimistic_signature)
                        );
                        println!();
                        println!(
                            "Operator-paid withdrawal signature hex: {}",
                            serialize_and_encode(operator_signature)
                        );
                        println!("If the transaction that created your withdrawal UTXO is not yet confirmed, please wait for it to be confirmed before proceeding.");
                        println!("After its confirmation run:");
                        println!();
                        println!("clementine-cli withdraw safe-withdraw --network {} {} {} {} {}",
                            network, &signer_address.address_with_prefix(), destination_address, withdrawal_utxo_outpoint, serialize_and_encode(optimistic_signature));
                        println!();
                        println!("on your online device to initiate optimistic withdrawal process on the Citrea network");
                        println!("Then monitor withdrawal status for 12 hours using:");
                        println!();
                        println!("clementine-cli withdraw status --network {} {}", network, &withdrawal_outpoint);
                        println!();
                        println!("If the optimistic withdrawal does not complete in 12 hours, run:");
                        println!();
                        println!("clementine-cli withdraw send-withdrawal-signature-to-operators --network {} {} {} {} {}",
                            network, &signer_address.address_with_prefix(), destination_address, withdrawal_utxo_outpoint, serialize_and_encode(operator_signature));
                        println!();
                        println!("to submit the operator-paid withdrawal signature to Clementine Operators");
                    }
                );
            }
            WithdrawCommands::SafeWithdraw {
                signer_address,
                destination_address,
                withdrawal_utxo_outpoint,
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

                should_not_have_purpose(&destination_address).inspect_err(|_| {
                    eprintln!("Invalid withdrawal address: {}", destination_address.bold());
                })?;

                let destination_address = parse_address(&destination_address, config.network)?;
                let withdrawal_outpoint = OutPoint::from_str(&withdrawal_utxo_outpoint)?;
                let sig = bitcoin::taproot::Signature::from_slice(&hex::decode(signature)?)?;
                handle_cli_command!(async
                    withdraw::safe_withdraw(
                        &signer_address,
                        &destination_address,
                        &withdrawal_outpoint,
                        &config.optimistic_withdrawal_amount,
                        &sig,
                        &config,
                    ),
                    withdrawal_ui_url => {
                        println!(
                            "\n{} Opening withdrawal page {withdrawal_ui_url} in your default browser...",
                            "INFO".bold()
                        );
                        if let Err(e) = open::that(&withdrawal_ui_url) {
                            return Err(eyre::eyre!(
                            "Failed to open browser: {}. Please visit the following URL manually: {}",
                            e,
                            withdrawal_ui_url
                            )
                            .into());
                        }
                    }
                );
            }
            WithdrawCommands::SendSafeWithdrawal {
                signer_address,
                destination_address,
                withdrawal_utxo_outpoint,
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

                should_not_have_purpose(&destination_address).inspect_err(|_| {
                    eprintln!("Invalid withdrawal address: {}", destination_address.bold());
                })?;

                let destination_address = parse_address(&destination_address, config.network)?;
                let withdrawal_outpoint = OutPoint::from_str(&withdrawal_utxo_outpoint)?;
                let sig = bitcoin::taproot::Signature::from_slice(&hex::decode(signature)?)?;
                handle_cli_command!(async
                    withdraw::send_safe_withdrawal(
                        withdraw::SafeWithdrawalParams {
                            signer_address,
                            destination_address,
                            withdrawal_outpoint,
                            withdrawal_amount: config.optimistic_withdrawal_amount,
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
                withdrawal_utxo,
                network,
            } => {
                let withdrawal_outpoint = OutPoint::from_str(&withdrawal_utxo)?;

                let config =
                    BridgeCliConfig::try_parse_config(cli.config_file, network.into()).unwrap();
                withdrawal_status(withdrawal_outpoint, &config).await?;
            }
            WithdrawCommands::SendWithdrawalSignatureToOperators {
                signer_address,
                destination_address,
                withdrawal_utxo_outpoint,
                signature,
                network,
            } => {
                let config =
                    BridgeCliConfig::try_parse_config(cli.config_file, network.into()).unwrap();
                send_withdrawal_signature(
                    &signer_address,
                    &destination_address,
                    &withdrawal_utxo_outpoint,
                    config.operator_withdrawal_amount.to_sat(),
                    &signature,
                    &config,
                )
                .await?;
                println!("Withdrawal signature sent successfully to Clementine Operators");
            }
        },
    }
    Ok(())
}
