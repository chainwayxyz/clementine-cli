use std::path::PathBuf;

use clap::{Parser, Subcommand};
use clementine_cli::{
    config::CliConfig,
    debug, deposit,
    mnemonic::show_mnemonic_secure,
    wallet::{
        self, create_encrypted_wallet_with_address, delete_wallet, import_wallet_from_file, import_wallet_from_mnemonic, verify_wallet_integrity
    },
    withdrawal,
};

#[derive(Parser)]
#[command(name = "clementine")]
#[command(about = "Clementine CLI - wallet-agnostic Citrea bridge CLI", long_about = None)]
struct Cli {
    /// Path to config file. If not given, current directory will be searched for the cli_config.toml file
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
    CreateWallet {},
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
    /// Delete a wallet by address.
    DeleteWallet {
        /// Bitcoin address of the wallet to delete
        address: String,
    },
    /// Verify integrity of wallet registry and files.
    VerifyIntegrity,
    /// List all wallet addresses.
    ListAddresses,
    ExportPrivateKey {
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
        CliConfig::try_parse_file(config_file_path).unwrap()
    } else {
        let mut current_dir = std::env::current_dir().unwrap();
        current_dir.push("cli_config.toml");
        debug!("No config file given, looking for the current directory: {current_dir:?}...");
        CliConfig::try_parse_file(current_dir).unwrap()
    };

    match cli.command {
        Commands::Wallet { command } => match command {
            WalletCommands::CreateWallet {} => {
                if let Err(e) = create_encrypted_wallet_with_address(config.network) {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            }
            WalletCommands::BackupWallet {
                destination,
                address,
            } => {
                if let Err(e) = wallet::backup_wallet(&address, &destination) {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            }
            WalletCommands::ShowMnemonic { address } => {
                if let Err(e) = show_mnemonic_secure(&address) {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            }
            WalletCommands::ImportFromMnemonic {} => {
                if let Err(e) = import_wallet_from_mnemonic(config.network) {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            }
            WalletCommands::ImportFromFile { filename } => {
                if let Err(e) = import_wallet_from_file(&filename) {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            }
            WalletCommands::ImportFromPrivateKey {} => {
                if let Err(e) = wallet::import_wallet_from_private_key(config.network) {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            }
            WalletCommands::DeleteWallet { address } => {
                if let Err(e) = delete_wallet(&address) {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            }
            WalletCommands::VerifyIntegrity => {
                if let Err(e) = verify_wallet_integrity() {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            }
            WalletCommands::ListAddresses => {
                if let Err(e) = clementine_cli::address::get_all_wallet_addresses() {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            }
            WalletCommands::ExportPrivateKey { address } => {
                if let Err(e) = wallet::export_private_key(&address, config.network) {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            }
        },
        Commands::Deposit { command } => match command {
            DepositCommands::GetDepositAddress {
                citrea_address,
                recovery_taproot_address,
            } => {
                if let Err(e) = deposit::get_deposit_address(
                    &citrea_address,
                    &recovery_taproot_address,
                    &config,
                ) {
                    eprintln!("Error: {e}");
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
                    eprintln!("Error: {e}");
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
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            }
            DepositCommands::DepositStatus { deposit_address } => {
                unimplemented!("deposit.deposit_status: {}", deposit_address);
            }
            DepositCommands::GetDepositParams { move_to_vault_txid } => {
                if let Err(e) = deposit::get_deposit_params(&move_to_vault_txid, &config).await {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            }
        },
        Commands::Withdrawal { command } => match command {
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
                    config.network,
                ) {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
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
                    eprintln!("Error: {e}");
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
                    eprintln!("Error: {e}");
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
