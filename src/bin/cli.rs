use std::path::PathBuf;

use clap::{Parser, Subcommand};

macro_rules! handle_or_exit {
    ($expr:expr) => {
        if let Err(e) = $expr {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    };
}
use clementine_cli::{
    config::BridgeCliConfig,
    debug, deposit,
    mnemonic::show_mnemonic_secure,
    wallet::{
        self, create_encrypted_wallet_with_address, delete_wallet, import_wallet_from_file,
        import_wallet_from_mnemonic, verify_wallet_integrity,
    },
    withdrawal,
};

#[derive(Parser)]
#[command(name = "clementine")]
#[command(about = "Clementine CLI - wallet-agnostic Citrea bridge CLI", long_about = None)]
struct Cli {
    /// Path to config file. If not given, current directory will be searched for the bridge_cli_config.toml file
    #[arg(long)]
    config_file: Option<PathBuf>,

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
    /// Create a new wallet with optional mnemonic display.
    CreateWallet {
        /// Name for the wallet file
        name: String,
    },
    /// Backup wallet to specified destination.
    BackupWallet {
        /// Destination path for wallet backup
        destination: String,
        /// Address of the wallet to backup
        address: String,
    },
    /// Show mnemonic with interactive terminal.
    ShowMnemonic {
        /// Bitcoin address to show mnemonic for
        address: String,
    },
    /// Import wallet using secure mnemonic input.
    ImportFromMnemonic {
        // No file parameter - uses secure step-by-step mnemonic input
    },
    ImportFromFile {
        /// Filename to import wallet from
        filename: String,
    },
    ImportFromPrivateKey {},
    /// Delete a wallet by name.
    DeleteWallet {
        /// Name of the wallet to delete
        name: String,
    },
    /// Verify integrity of wallet registry and files.
    VerifyIntegrity,
    /// List all wallets with their addresses.
    ListWalletsWithAddresses,
    ShowPrivateKey {
        /// Address to export private key for
        address: String,
    },
}

#[derive(Subcommand)]
enum DepositCommands {
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

    let config = if let Some(config_file_path) = cli.config_file {
        debug!("Config file {config_file_path:?} is going to be used...");
        BridgeCliConfig::try_parse_file(config_file_path).unwrap()
    } else {
        let mut current_dir = std::env::current_dir().unwrap();
        current_dir.push("bridge_cli_config.toml");
        debug!("No config file given, looking for the current directory: {current_dir:?}...");
        BridgeCliConfig::try_parse_file(current_dir).unwrap()
    };

    match cli.command {
        Commands::Wallet { command } => match command {
            WalletCommands::CreateWallet { name } => {
                handle_or_exit!(create_encrypted_wallet_with_address(config.network, name));
            }
            WalletCommands::BackupWallet {
                destination,
                address,
            } => {
                handle_or_exit!(wallet::backup_wallet(&address, &destination));
            }
            WalletCommands::ShowMnemonic { address } => {
                handle_or_exit!(show_mnemonic_secure(&address));
            }
            WalletCommands::ImportFromMnemonic {} => {
                handle_or_exit!(import_wallet_from_mnemonic(config.network));
            }
            WalletCommands::ImportFromFile { filename } => {
                handle_or_exit!(import_wallet_from_file(&filename));
            }
            WalletCommands::ImportFromPrivateKey {} => {
                handle_or_exit!(wallet::import_wallet_from_private_key(config.network));
            }
            WalletCommands::DeleteWallet { name } => {
                handle_or_exit!(delete_wallet(&name));
            }
            WalletCommands::VerifyIntegrity => {
                handle_or_exit!(verify_wallet_integrity());
            }
            WalletCommands::ListWalletsWithAddresses => {
                handle_or_exit!(clementine_cli::address::get_all_wallets_with_addresses());
            }
            WalletCommands::ShowPrivateKey { address } => {
                handle_or_exit!(wallet::show_private_key(&address, config.network));
            }
        },
        Commands::Deposit { command } => match command {
            DepositCommands::GetDepositAddress {
                citrea_address,
                recovery_taproot_address,
            } => {
                handle_or_exit!(deposit::get_deposit_address(
                    &citrea_address,
                    &recovery_taproot_address,
                    &config,
                ));
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
                handle_or_exit!(deposit::sign_recovery_tx(
                    &evm_address,
                    &recovery_taproot_address,
                    &deposit_txid,
                    deposit_vout,
                    &claim_address,
                    fee_rate,
                    amount,
                    &config,
                ));
            }
            DepositCommands::VerifyRecoveryTx {
                recovery_tx,
                evm_address,
                recovery_taproot_address,
                amount,
            } => {
                handle_or_exit!(deposit::verify_recovery_tx(
                    &recovery_tx,
                    &evm_address,
                    &recovery_taproot_address,
                    amount,
                    &config,
                ));
            }
            DepositCommands::DepositStatus { deposit_address } => {
                unimplemented!("deposit.deposit_status: {}", deposit_address);
            }
            DepositCommands::GetDepositParams { move_to_vault_txid } => {
                handle_or_exit!(deposit::get_deposit_params(&move_to_vault_txid, &config).await);
            }
        },
        Commands::Withdrawal { command } => match command {
            WithdrawalCommands::GenerateWithdrawalSignature {
                withdrawal_address,
                signer_address,
                withdrawal_utxo,
                amount,
            } => {
                handle_or_exit!(withdrawal::generate_withdrawal_signature(
                    &signer_address,
                    &withdrawal_address,
                    &withdrawal_utxo,
                    amount,
                    config.network,
                ));
            }
            WithdrawalCommands::SafeWithdraw {
                signer_address,
                withdrawal_address,
                withdrawal_utxo,
                amount,
                signature,
            } => {
                handle_or_exit!(
                    withdrawal::safe_withdraw(
                        &signer_address,
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
                signer_address,
                withdrawal_address,
                withdrawal_utxo,
                amount,
                signature,
            } => {
                handle_or_exit!(
                    withdrawal::send_safe_withdrawal(
                        &signer_address,
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
