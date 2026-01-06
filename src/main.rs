use bitcoin::{Network, OutPoint, Txid, taproot::Signature};
use clap::{Parser, Subcommand};
use clementine_cli::cli::{
    cli_backup_wallet, cli_create_wallet, cli_generate_withdrawal_signatures,
    cli_get_deposit_address, cli_get_deposit_address_details, cli_import_wallet_from_file,
    cli_import_wallet_from_mnemonic, cli_import_wallet_from_private_key,
    cli_list_all_deposit_addresses, cli_scan_withdrawals, cli_show_mnemonic, cli_show_private_key,
    cli_start_withdrawal, cli_verify_recovery_tx_with_validation, cli_verify_wallet_integrity,
    deposit_create_signed_recovery_tx, deposit_status, send_withdrawal_signature,
    withdrawal_status,
};
use clementine_cli::cli_network::{CliNetwork, NETWORK_HELP_MESSAGE, NetworkParser};

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
use clementine_cli::{handle_simple_call, parse_transaction_hex};
use colored::Colorize;
use std::str::FromStr;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt};

const TERMS_OF_SERVICE_URL: &str = "https://www.citrea.xyz/clementine-bridge-terms-of-service";

/// Initializes tracing to `Debug` level if verbose flag is given. If not,
/// defaults to `RUST_LOG` env variable. If neither is set, logging is turned off.
pub(crate) fn initialize_logger(is_verbose: bool) {
    let filter = if is_verbose {
        EnvFilter::builder()
            .with_default_directive(LevelFilter::DEBUG.into())
            .from_env_lossy()
    } else if std::env::var("RUST_LOG").is_ok() {
        EnvFilter::from_default_env()
    } else {
        EnvFilter::new("off")
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

fn print_terms_notice() {
    println!(
        "By continuing to interact with the Clementine CLI, you are confirming that you have reviewed and have agreed to the terms of service for the Clementine bridge presented here: {}",
        TERMS_OF_SERVICE_URL.underline()
    );
    println!();
}

#[derive(Parser)]
#[command(name = "clementine")]
#[command(about = "Clementine CLI - wallet-agnostic Citrea bridge CLI", long_about = None, version)]
struct Cli {
    /// Turns verbose logging on
    #[arg(long, action = clap::ArgAction::SetTrue)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    // Init
    Init {},
    // Update config
    UpdateConfig {
        #[arg(long, default_value_t = CliNetwork::Bitcoin, help = NETWORK_HELP_MESSAGE, value_parser = NetworkParser)]
        network: CliNetwork,
        /// Assume yes to all prompts, changes will be applied without confirmation
        #[arg(short = 'y', long = "yes", action = clap::ArgAction::SetTrue)]
        yes: bool,
        /// Key=value pairs to update, e.g. bridge_amount=123456 esplora_rest_api=https://...
        #[arg(required = true)]
        kv: Vec<String>,
    },
    /// Show configuration for a given network
    ShowConfig {
        #[arg(long, default_value_t = CliNetwork::Bitcoin, help = NETWORK_HELP_MESSAGE, value_parser = NetworkParser)]
        network: CliNetwork,
    },
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
        #[arg(long, default_value_t = CliNetwork::Bitcoin, help = NETWORK_HELP_MESSAGE, value_parser = NetworkParser)]
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
        #[arg(long, default_value_t = CliNetwork::Bitcoin, help = NETWORK_HELP_MESSAGE, value_parser = NetworkParser)]
        network: CliNetwork,
        /// Label for the imported wallet
        label: String,
        /// Purpose for the imported wallet
        purpose: Purpose,
    },
    /// Import wallet using secure private key input.
    ImportPrivateKey {
        #[arg(long, default_value_t = CliNetwork::Bitcoin, help = NETWORK_HELP_MESSAGE, value_parser = NetworkParser)]
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
        #[arg(long, default_value_t = CliNetwork::Bitcoin, help = NETWORK_HELP_MESSAGE, value_parser = NetworkParser)]
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
        /// Clementine aggregated public key
        clementine_aggregated_key: String,
        #[arg(long, default_value_t = CliNetwork::Bitcoin, help = NETWORK_HELP_MESSAGE, value_parser = NetworkParser)]
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
        amount: Option<f64>,
        /// Clementine aggregated public key
        clementine_aggregated_key: String,
        #[arg(long, default_value_t = CliNetwork::Bitcoin, help = NETWORK_HELP_MESSAGE, value_parser = NetworkParser)]
        network: CliNetwork,
    },
    /// Check the status of a deposit.
    Status {
        /// Deposit address (taproot address funds were sent to)
        deposit_address: String,
        #[arg(long, default_value_t = CliNetwork::Bitcoin, help = NETWORK_HELP_MESSAGE, value_parser = NetworkParser)]
        network: CliNetwork,
    },
    /// Broadcasts raw recovery transaction to Bitcoin network either by Bitcoin Esplora API or Bitcoin RPC.
    BroadcastRecoveryTx {
        /// Raw transaction to broadcast (hex-encoded)
        raw_tx: String,
        #[arg(long, default_value_t = CliNetwork::Bitcoin, help = NETWORK_HELP_MESSAGE, value_parser = NetworkParser)]
        network: CliNetwork,
    },
    /// Get deposit parameters for a move-to-vault transaction.
    GetDepositParams {
        /// Move-to-vault transaction ID (txid)
        move_to_vault_txid: String,
        #[arg(long, default_value_t = CliNetwork::Bitcoin, help = NETWORK_HELP_MESSAGE, value_parser = NetworkParser)]
        network: CliNetwork,
    },
    /// List all stored deposit addresses.
    ListDepositAddresses,
    /// Show stored details for a deposit address.
    GetDepositAddressDetails {
        /// Deposit address (taproot address funds were sent to)
        deposit_address: String,
    },
}

#[derive(Subcommand)]
enum WithdrawCommands {
    /// Start a withdrawal process and get instructions for sending funds.
    Start {
        #[arg(long, default_value_t = CliNetwork::Bitcoin, help = NETWORK_HELP_MESSAGE, value_parser = NetworkParser)]
        network: CliNetwork,
        /// Clementine wallet address for signing withdrawals (wit-prefixed, taproot)
        signer_address: String,
        /// Destination address for withdrawn BTC
        destination_address: String,
    },
    /// Scan for UTXOs to use in withdrawal.
    Scan {
        #[arg(long, default_value_t = CliNetwork::Bitcoin, help = NETWORK_HELP_MESSAGE, value_parser = NetworkParser)]
        network: CliNetwork,
        /// Clementine wallet address for signing withdrawals (wit-prefixed, taproot)
        signer_address: String,
        /// Destination address for withdrawn BTC
        destination_address: String,
    },
    /// Generate a withdrawal signature (for air-gapped use).
    GenerateWithdrawalSignatures {
        #[arg(long, default_value_t = CliNetwork::Bitcoin, help = NETWORK_HELP_MESSAGE, value_parser = NetworkParser)]
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
        #[arg(long, default_value_t = CliNetwork::Bitcoin, help = NETWORK_HELP_MESSAGE, value_parser = NetworkParser)]
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
    #[command(hide = true)]
    SendSafeWithdraw {
        #[arg(long, default_value_t = CliNetwork::Bitcoin, help = NETWORK_HELP_MESSAGE, value_parser = NetworkParser)]
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
        #[arg(long, default_value_t = CliNetwork::Bitcoin, help = NETWORK_HELP_MESSAGE, value_parser = NetworkParser)]
        network: CliNetwork,
        /// Withdrawal UTXO outpoint (format: <txid>:<vout>)
        withdrawal_utxo: String,
    },
    /// Send withdrawal signatures to operators.
    SendWithdrawalSignatureToOperators {
        #[arg(long, default_value_t = CliNetwork::Bitcoin, help = NETWORK_HELP_MESSAGE, value_parser = NetworkParser)]
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
        Commands::Init {} => {
            handle_cli_command!(clementine_cli::cli::cli_init());
        }
        Commands::UpdateConfig { network, yes, kv } => {
            let kv: Vec<(String, String)> = kv
                .into_iter()
                .map(|s| {
                    let mut split = s.splitn(2, '=');
                    let key = split.next().expect("Key always exists");
                    let value = split.next().unwrap_or("");
                    (key.to_string(), value.to_string())
                })
                .collect();
            handle_cli_command!(clementine_cli::cli::update_config_with_confirm(
                network.into(),
                kv,
                yes
            ));
        }
        Commands::ShowConfig { network } => {
            handle_cli_command!(clementine_cli::cli::cli_show_config(network.into()));
        }
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
                let address = handle_simple_call!(
                    TaprootAddressWithPrefix::from_string_with_prefix_unchecked(&address)
                );
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
                let address = handle_simple_call!(
                    TaprootAddressWithPrefix::from_string_with_prefix_unchecked(&address)
                );
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
                let config = handle_simple_call!(BridgeCliConfig::try_parse_config(network.into()));
                let citrea_address = handle_simple_call!(parse_citrea_address(&citrea_address));
                let recovery_taproot_address =
                    handle_simple_call!(TaprootAddressWithPrefix::from_string_with_prefix(
                        &recovery_taproot_address,
                        config.network,
                    ));
                handle_cli_command!(async
                    cli_get_deposit_address(&citrea_address, &recovery_taproot_address, &config),
                    deposit_address => {
                        println!("Deposit address: {}", deposit_address.to_string ().bold());
                        println!("{} Send exactly {} BTC to the address above to initiate the deposit.", "INFO".bold(), config.bridge_amount.to_btc());
                        println!("For Bitcoin Core users, you can send your deposit using the following command (add any parameters as needed): ");
                        println!("$ {} sendtoaddress {} {}", get_bitcoin_cli_command(&config), deposit_address.to_string(), config.bridge_amount.to_btc());
                        println!();

                        print_terms_notice();
                        println!("After sending the funds, you can monitor the deposit status using:");
                        println!("clementine-cli deposit status --network {} {}", config.network, deposit_address.to_string());
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
                clementine_aggregated_key,
                network,
            } => {
                let config = handle_simple_call!(BridgeCliConfig::try_parse_config(network.into()));
                let recovery_taproot_address =
                    handle_simple_call!(TaprootAddressWithPrefix::from_string_with_prefix(
                        &recovery_taproot_address,
                        config.network,
                    ));

                let citrea_address = handle_simple_call!(parse_citrea_address(&citrea_address));
                let deposit_utxo_outpoint =
                    handle_simple_call!(OutPoint::from_str(&deposit_utxo_outpoint));
                let bitcoin_address =
                    handle_simple_call!(BitcoinAddress::from_str(&destination_address));
                let destination_address =
                    handle_simple_call!(bitcoin_address.require_network(config.network));

                handle_cli_command!(
                    deposit_create_signed_recovery_tx(
                        &citrea_address,
                        &recovery_taproot_address,
                        &deposit_utxo_outpoint,
                        &destination_address,
                        fee_rate,
                        amount,
                        &config,
                        clementine_aggregated_key,
                    )
                    .await
                );
            }
            DepositCommands::VerifyRecoveryTx {
                recovery_tx,
                recovery_taproot_address,
                evm_address,
                amount,
                clementine_aggregated_key,
                network,
            } => {
                let config = handle_simple_call!(BridgeCliConfig::try_parse_config(network.into()));
                let recovery_tx = handle_simple_call!(parse_transaction_hex(&recovery_tx));
                let citrea_address = handle_simple_call!(parse_citrea_address(&evm_address));
                let recovery_taproot_address =
                    handle_simple_call!(TaprootAddressWithPrefix::from_string_with_prefix(
                        &recovery_taproot_address,
                        config.network,
                    ));
                handle_cli_command!(
                    async
                    cli_verify_recovery_tx_with_validation(
                        deposit::VerifyRecoveryTxParams {
                            recovery_tx,
                            citrea_address,
                            recovery_taproot_address,
                            amount,
                        },
                        &config,
                        clementine_aggregated_key,
                    ),
                    (txid, address, amount) => {
                        println!("Recovery transaction verified!");
                        println!(
                            "This transaction may be broadcast only after the transaction {} \
                             has been confirmed on-chain for at least {} blocks.",
                            txid, config.user_takes_after
                        );
                        println!(
                            "Once this condition has been satisfied and the transaction is broadcast, \
                             an amount of {} BTC ({} sats) will be sent to the address {}.",
                            amount, amount.to_sat(), address
                        );
                    }
                );
            }
            DepositCommands::Status {
                deposit_address,
                network,
            } => {
                let config = handle_simple_call!(BridgeCliConfig::try_parse_config(network.into()));
                let deposit_address =
                    handle_simple_call!(parse_taproot_address(&deposit_address, config.network));
                handle_cli_command!(deposit_status(deposit_address, &config).await);
            }
            DepositCommands::BroadcastRecoveryTx { raw_tx, network } => {
                let config = handle_simple_call!(BridgeCliConfig::try_parse_config(network.into()));

                handle_cli_command!(async broadcast_recovery_tx(&config, raw_tx), txid => {
                    println!("{}", txid);
                });
            }
            DepositCommands::GetDepositParams {
                move_to_vault_txid,
                network,
            } => {
                let config = handle_simple_call!(BridgeCliConfig::try_parse_config(network.into()));
                let move_to_vault_txid = handle_simple_call!(Txid::from_str(&move_to_vault_txid));
                handle_cli_command!(async
                    get_deposit_params(&move_to_vault_txid, &config),
                    params => {
                        println!("Deposit parameters hex: {}", hex::encode(params));
                    }
                );
            }
            DepositCommands::ListDepositAddresses => {
                handle_cli_command!(cli_list_all_deposit_addresses());
            }
            DepositCommands::GetDepositAddressDetails { deposit_address } => {
                handle_cli_command!(cli_get_deposit_address_details(&deposit_address));
            }
        },
        Commands::Withdraw { command } => match command {
            WithdrawCommands::Start {
                signer_address,
                destination_address,
                network,
            } => {
                let config = handle_simple_call!(BridgeCliConfig::try_parse_config(network.into()));

                let signer_address =
                    handle_simple_call!(TaprootAddressWithPrefix::from_string_with_prefix(
                        &signer_address,
                        config.network,
                    ));

                // Use wrap_err to preserve inner error location and context

                handle_simple_call!(should_not_have_purpose(&destination_address));

                let destination_address =
                    handle_simple_call!(parse_address(&destination_address, config.network));

                handle_cli_command!(async
                    cli_start_withdrawal(&signer_address, &destination_address, &config),
                    _result => {
                        println!("Send exactly {} sats to {}", config.dust_utxo_amount.to_sat(), signer_address.address_without_prefix());
                        println!("You can use:");
                        println!("$ {} sendtoaddress {} {}",
                            get_bitcoin_cli_command(&config),
                            signer_address.address_without_prefix(), config.dust_utxo_amount.to_btc());
                        println!("or a similar command from a wallet you are using");
                        println!("Then run:");
                        println!("$ clementine-cli withdraw scan --network {} {} {}",
                            config.network, signer_address.address_with_prefix(), destination_address);
                        println!("to scan UTXOs that can be used for the withdrawal operation");
                        println!();
                        print_terms_notice();
                        println!("{} If your wallet cannot send exactly {} sats, you may send a higher supported amount. Be sure to update the config to match the amount you actually sent before proceeding.", "WARNING".bold(), config.dust_utxo_amount.to_sat());
                    }
                );
            }
            WithdrawCommands::Scan {
                signer_address,
                destination_address,
                network,
            } => {
                let config = handle_simple_call!(BridgeCliConfig::try_parse_config(network.into()));
                let signer_address =
                    handle_simple_call!(TaprootAddressWithPrefix::from_string_with_prefix(
                        &signer_address,
                        config.network,
                    ));

                handle_simple_call!(should_not_have_purpose(&destination_address));

                let destination_address =
                    handle_simple_call!(parse_address(&destination_address, config.network));
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
                let config = handle_simple_call!(BridgeCliConfig::try_parse_config(network.into()));
                let signer_address =
                    handle_simple_call!(TaprootAddressWithPrefix::from_string_with_prefix(
                        &signer_address,
                        network.into(),
                    ));

                handle_simple_call!(should_not_have_purpose(&destination_address));

                let destination_address =
                    handle_simple_call!(parse_address(&destination_address, network.into()));
                let withdrawal_outpoint =
                    handle_simple_call!(OutPoint::from_str(&withdrawal_utxo_outpoint));
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
                        println!("$ clementine-cli withdraw safe-withdraw --network {} {} {} {} {}",
                            network, &signer_address.address_with_prefix(), destination_address, withdrawal_utxo_outpoint, serialize_and_encode(optimistic_signature));
                        println!();
                        println!("on your online device to initiate optimistic withdrawal process on the Citrea network");
                        println!("Then monitor withdrawal status for 12 hours using:");
                        println!();
                        println!("$ clementine-cli withdraw status --network {} {}", network, &withdrawal_outpoint);
                        println!();
                        println!("If the optimistic withdrawal does not complete in 12 hours, run:");
                        println!();
                        println!("$ clementine-cli withdraw send-withdrawal-signature-to-operators --network {} {} {} {} {}",
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
                let config = handle_simple_call!(BridgeCliConfig::try_parse_config(network.into()));

                let signer_address =
                    handle_simple_call!(TaprootAddressWithPrefix::from_string_with_prefix(
                        &signer_address,
                        config.network,
                    ));

                handle_simple_call!(should_not_have_purpose(&destination_address));

                let destination_address =
                    handle_simple_call!(parse_address(&destination_address, config.network));
                let withdrawal_outpoint =
                    handle_simple_call!(OutPoint::from_str(&withdrawal_utxo_outpoint));
                let signature_bytes = handle_simple_call!(hex::decode(signature));
                let sig =
                    handle_simple_call!(bitcoin::taproot::Signature::from_slice(&signature_bytes));
                handle_cli_command!(async
                    withdraw::safe_withdraw(
                        &signer_address,
                        &destination_address,
                        &withdrawal_outpoint,
                        &config.optimistic_withdrawal_amount,
                        &sig,
                        &config,
                    ),
                    (withdrawal_ui_url, tx_json, params) => {
                        println!(
                            "\n{} Opening withdrawal page {} in your default browser...",
                            "INFO".bold(),
                            withdrawal_ui_url.0
                        );

                        println!("\nPress a key to continue...");
                        std::io::stdin().read_line(&mut String::new()).map_err(|e| {
                            tracing::error!("Failed to read input: {}", e);
                            eyre::eyre!("Failed to read input.")
                        })?;

                        println!("\nPlease review the transaction details below:\n");


                        let pretty_json = serde_json::from_str::<serde_json::Value>(&tx_json.0)
                            .ok()
                            .and_then(|json| serde_json::to_string_pretty(&json).ok())
                            .unwrap_or_else(|| tx_json.0.clone());

                        println!("Transaction JSON:\n{}", pretty_json);

                        println!("\nDestination Address: {}\n", destination_address.to_string());

                        println!("{:#?}\n", params);

                        println!("Please double check the transaction details before proceeding in the browser.\n");

                        println!("Press a key to continue...");
                        let mut input = String::new();

                        std::io::stdin().read_line(&mut input).map_err(|e| {
                            tracing::error!("Failed to read input: {}", e);
                            eyre::eyre!("Failed to read input.")
                        })?;

                        if let Err(e) = open::that(&withdrawal_ui_url.0) {
                            return Err(eyre::eyre!(
                            "Failed to open browser: {}. Please visit the following URL manually: {}",
                            e,
                            withdrawal_ui_url.0
                            )
                            .into());
                        }
                    }
                );
            }
            WithdrawCommands::SendSafeWithdraw {
                signer_address,
                destination_address,
                withdrawal_utxo_outpoint,
                signature,
                network,
            } => {
                let config = handle_simple_call!(BridgeCliConfig::try_parse_config(network.into()));
                let signer_address =
                    handle_simple_call!(TaprootAddressWithPrefix::from_string_with_prefix(
                        &signer_address,
                        config.network,
                    ));

                handle_simple_call!(should_not_have_purpose(&destination_address));

                let destination_address =
                    handle_simple_call!(parse_address(&destination_address, config.network));
                let withdrawal_outpoint =
                    handle_simple_call!(OutPoint::from_str(&withdrawal_utxo_outpoint));
                let signature_bytes = handle_simple_call!(hex::decode(signature));
                let sig =
                    handle_simple_call!(bitcoin::taproot::Signature::from_slice(&signature_bytes));
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
                        println!("Transaction Hash: {:#?}", result.transaction_hash);
                    }
                );
            }
            WithdrawCommands::Status {
                withdrawal_utxo,
                network,
            } => {
                let withdrawal_outpoint = handle_simple_call!(OutPoint::from_str(&withdrawal_utxo));

                let config = handle_simple_call!(BridgeCliConfig::try_parse_config(network.into()));

                handle_cli_command!(async withdrawal_status(withdrawal_outpoint, &config));
            }
            WithdrawCommands::SendWithdrawalSignatureToOperators {
                signer_address,
                destination_address,
                withdrawal_utxo_outpoint,
                signature,
                network,
            } => {
                let config = handle_simple_call!(BridgeCliConfig::try_parse_config(network.into()));
                handle_cli_command!(
                    async send_withdrawal_signature(
                        &signer_address,
                        &destination_address,
                        &withdrawal_utxo_outpoint,
                        config.operator_withdrawal_amount.to_sat(),
                        &signature,
                        &config,
                    ),
                    _ => {
                        println!("Withdrawal signature sent successfully to Clementine Operators");
                    }
                );
            }
        },
    }
    Ok(())
}
