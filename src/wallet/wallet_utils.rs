use crate::bitcoin_utils::calculate_taproot_address;
use crate::errors::BridgeCliError;
use crate::structs::AddrDisplay;
use crate::structs::SecureKeypair;
use crate::structs::SecureSecretKey;
use crate::structs::SecureString;
use crate::structs::TaprootAddressWithPrefix;
use crate::wallet::address::generate_address_from_mnemonic;
use crate::wallet::encryption::aes_decrypt_secure;
use crate::wallet::mnemonic::load_mnemonic;
use crate::wallet::passphrase::prompt_unlock_passphrase;
use crate::wallet::wallet_storage::get_storage_dir;
use crate::wallet::wallet_storage::{
    GenericWalletData, get_wallets_from_registry, load_wallet_data,
};
use bip39::Mnemonic;
use bitcoin::Network;
use bitcoin::address::NetworkChecked;
use bitcoin::address::NetworkUnchecked;
use bitcoin::address::NetworkValidation;
use bitcoin::key::Keypair;
use bitcoin::secp256k1::Secp256k1;
use bitcoin::secp256k1::SecretKey;
use eyre::eyre;
use secrecy::ExposeSecret;
use std::collections::HashSet;
use std::str::FromStr;

/// Securely load a key from wallet storage and check address validity - always requires a passphrase
pub(crate) fn load_key<T>(
    address: &TaprootAddressWithPrefix<T>,
    passphrase: &SecureString,
) -> Result<SecureKeypair, BridgeCliError>
where
    T: NetworkValidation,
    bitcoin::Address<T>: AddrDisplay,
{
    if !address_exists(address)? {
        return Err(BridgeCliError::WalletNotFound(
            address.address_with_prefix(),
        ));
    }

    // Load wallet data
    let wallet_data = load_wallet_data(address)?;

    // Load the encrypted private key
    let encrypted_private_key = wallet_data
        .encrypted_private_key
        .ok_or_else(|| BridgeCliError::NoEncryptedPrivateKeyFound)?;

    let encrypted_data =
        crate::wallet::encryption::encrypted_data_from_hex(&encrypted_private_key)?;
    let decrypted_key = aes_decrypt_secure(&encrypted_data, passphrase)?;

    let secp = Secp256k1::new();
    let secret_key = SecureSecretKey::new(SecretKey::from_str(decrypted_key.expose_secret())?);

    let keypair = Keypair::from_secret_key(&secp, secret_key.as_ref());
    let secure_keypair = SecureKeypair::new(keypair);

    Ok(secure_keypair)
}

/// Helper function to validate mnemonic imports during wallet import
pub(crate) fn validate_mnemonic_import(
    decrypted_mnemonic: &SecureString,
    wallet_data: &GenericWalletData,
) -> Result<(), BridgeCliError> {
    let network = parse_network(&wallet_data.network)?;

    let wallet_address = TaprootAddressWithPrefix::from_string_with_prefix(
        &wallet_data.address_with_prefix,
        network,
    )?;

    let mnemonic = Mnemonic::parse(decrypted_mnemonic.expose_secret())
        .map_err(|e| BridgeCliError::MnemonicValidationFailed(e.to_string()))?;

    // Generate address from mnemonic to verify it matches
    match generate_address_from_mnemonic(&mnemonic, network, wallet_address.purpose) {
        Ok(derived_address) => {
            if derived_address.address != wallet_address.address {
                return Err(BridgeCliError::AddressMismatch);
            }
            println!("Passphrase verified successfully! Address confirmed.");
        }
        Err(e) => return Err(BridgeCliError::MnemonicParseError(e.to_string())),
    }

    Ok(())
}

/// Helper function to parse network string into Network enum
pub(crate) fn parse_network(network_str: &str) -> Result<Network, BridgeCliError> {
    match network_str {
        "testnet4" => Ok(Network::Testnet4),
        "testnet" => Ok(Network::Testnet),
        "regtest" => Ok(Network::Regtest),
        "signet" => Ok(Network::Signet),
        "bitcoin" => Ok(Network::Bitcoin),
        _ => Err(BridgeCliError::UnsupportedNetwork),
    }
}

/// Helper function to validate private key imports during wallet import
pub(crate) fn validate_private_key_import(
    wallet_data: &GenericWalletData,
    passphrase: &SecureString,
    wallet_address: &str,
) -> Result<(), BridgeCliError> {
    if let Some(encrypted_private_key_hex) = &wallet_data.encrypted_private_key {
        let encrypted_private_data = crate::wallet::encryption::encrypted_data_from_hex(
            encrypted_private_key_hex,
        )
        .map_err(|e| BridgeCliError::Eyre(eyre!("Failed to parse encrypted private key: {}", e)))?;

        // Decrypt and validate the private key
        match aes_decrypt_secure(&encrypted_private_data, passphrase) {
            Ok(decrypted_private_key) => {
                let network = parse_network(&wallet_data.network)?;

                // Validate the private key format and derive address to verify
                match SecretKey::from_str(decrypted_private_key.expose_secret()) {
                    Ok(private_key) => {
                        let secure_secret_key = SecureSecretKey::new(private_key);
                        let keypair = SecureKeypair::new(Keypair::from_secret_key(
                            &crate::bitcoin_utils::SECP,
                            secure_secret_key.as_ref(),
                        ));
                        let derived_address = calculate_taproot_address(&keypair, network);

                        if derived_address.to_string() != wallet_address {
                            return Err(BridgeCliError::AddressMismatch);
                        }
                        println!(
                            "Passphrase verified successfully! Private key address confirmed."
                        );
                    }
                    Err(_) => {
                        return Err(BridgeCliError::InvalidPrivateKey(
                            "Invalid private key format".to_string(),
                        ));
                    }
                }
            }
            Err(_) => {
                return Err(BridgeCliError::IncorrectPassphrase);
            }
        }
    } else {
        return Err(BridgeCliError::MissingEncryptedPrivateKeyField);
    }

    Ok(())
}

pub(crate) fn label_exists(label: &str) -> Result<bool, BridgeCliError> {
    let wallets = get_wallets_from_registry()?;

    for (_address, wallet_entry) in wallets {
        if wallet_entry.label == label {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(crate) fn address_exists<T>(
    address: &TaprootAddressWithPrefix<T>,
) -> Result<bool, BridgeCliError>
where
    T: bitcoin::address::NetworkValidation,
    bitcoin::Address<T>: AddrDisplay,
{
    let wallets = get_wallets_from_registry()?;
    let address = address.address_without_prefix();

    for (addr, _wallet_entry) in wallets {
        if addr == address {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Validation options for wallet creation and import operations
#[derive(Debug)]
pub enum WalletValidationMode {
    /// Check if wallet name already exists
    Label,
    /// Check if address already exists for the given network
    Address,
    /// Check both wallet name and address
    Both,
}

/// Combined validation function to check for conflicts during wallet operations
pub(crate) fn validate_wallet_availability(
    label: Option<&str>,
    address: Option<&TaprootAddressWithPrefix<NetworkChecked>>,
    mode: WalletValidationMode,
) -> Result<(), BridgeCliError> {
    let should_check_wallet = matches!(
        mode,
        WalletValidationMode::Label | WalletValidationMode::Both
    );
    let should_check_address = matches!(
        mode,
        WalletValidationMode::Address | WalletValidationMode::Both
    );

    if should_check_wallet {
        let label = label.ok_or_else(|| {
            BridgeCliError::Eyre(eyre::eyre!("Wallet label is required for validation"))
        })?;
        if label_exists(label)? {
            return Err(BridgeCliError::LabelAlreadyExists(label.to_string()));
        }
    }

    if should_check_address {
        let address = address.ok_or_else(|| {
            BridgeCliError::Eyre(eyre::eyre!("Address is required for validation"))
        })?;
        if address_exists(address)? {
            return Err(BridgeCliError::AddressAlreadyExists(
                address.address_with_prefix(),
            ));
        }
    }

    Ok(())
}

/// Parse and validate an imported wallet file
pub(crate) fn parse_and_validate_imported_wallet(
    file_path: &str,
    label: Option<&str>,
) -> Result<crate::wallet::wallet_storage::GenericWalletData, BridgeCliError> {
    use std::fs;
    use std::path::Path;

    let source_path = Path::new(file_path);

    if !source_path.exists() {
        return Err(BridgeCliError::WalletFileNotFound(file_path.to_string()));
    }

    if !source_path.is_file() {
        return Err(BridgeCliError::PathNotAFile(file_path.to_string()));
    }

    // Read and parse the wallet file
    let wallet_content = fs::read_to_string(source_path)?;
    let wallet_data: crate::wallet::wallet_storage::GenericWalletData =
        serde_json::from_str(&wallet_content).map_err(|e| {
            BridgeCliError::Eyre(eyre::eyre!(
                "Failed to parse wallet file '{}': {}",
                source_path.display(),
                e
            ))
        })?;

    let network = parse_network(&wallet_data.network)?;

    // Extract and validate required fields
    let wallet_address = TaprootAddressWithPrefix::from_string_with_prefix(
        &wallet_data.address_with_prefix,
        network,
    )?;

    let label = if let Some(lbl) = label {
        lbl
    } else {
        &wallet_data.label
    };

    // Validate that both wallet label and address don't already exist
    validate_wallet_availability(
        Some(label),
        Some(&wallet_address),
        WalletValidationMode::Both,
    )?;

    // Check if encrypted data exists
    if wallet_data.encrypted_mnemonic.is_none() {
        return Err(BridgeCliError::MissingEncryptedMnemonicField);
    }

    if wallet_data.encrypted_private_key.is_none() {
        return Err(BridgeCliError::Eyre(eyre::eyre!(
            "Missing encrypted private key field"
        )));
    }

    Ok(wallet_data)
}

pub(crate) fn report_integrity_results(
    registry_wallets: &HashSet<TaprootAddressWithPrefix<NetworkUnchecked>>,
    file_wallets: &HashSet<TaprootAddressWithPrefix<NetworkUnchecked>>,
) {
    let registry_only: HashSet<&TaprootAddressWithPrefix<NetworkUnchecked>> =
        registry_wallets.difference(file_wallets).collect();
    let files_only: HashSet<&TaprootAddressWithPrefix<NetworkUnchecked>> =
        file_wallets.difference(registry_wallets).collect();
    let matching: HashSet<&TaprootAddressWithPrefix<NetworkUnchecked>> =
        registry_wallets.intersection(file_wallets).collect();

    // Report results
    println!("Integrity Verification Results:");
    println!("  Total registered wallets: {}", registry_wallets.len());
    println!("  Total wallet files found: {}", file_wallets.len());
    println!("  Matching entries: {}", matching.len());
    println!();

    print_successful_matches(&matching);
    let has_issues = print_integrity_issues(&registry_only, &files_only);

    print_integrity_summary(
        has_issues,
        registry_wallets.is_empty() && file_wallets.is_empty(),
        &registry_only,
        &files_only,
    );
}

/// Print successful wallet matches
fn print_successful_matches(
    matching: &std::collections::HashSet<&TaprootAddressWithPrefix<NetworkUnchecked>>,
) {
    use colored::Colorize;

    if !matching.is_empty() {
        print_wallet_list("Properly registered wallets:", matching, |address| {
            format!("  - {}", address.address_with_prefix().green())
        });
    }
}

/// Print integrity issues (missing files and unregistered files)
fn print_integrity_issues(
    registry_only: &std::collections::HashSet<&TaprootAddressWithPrefix<NetworkUnchecked>>,
    files_only: &std::collections::HashSet<&TaprootAddressWithPrefix<NetworkUnchecked>>,
) -> bool {
    use colored::Colorize;

    let mut has_issues = false;

    // Report wallets in registry but missing files
    if !registry_only.is_empty() {
        has_issues = true;
        print_wallet_list(
            "Wallets in registry but missing files:",
            registry_only,
            |address| {
                format!(
                    "  - {} (file: wallet_{}.json not found)",
                    address.address_with_prefix().yellow(),
                    address.address_without_prefix()
                )
            },
        );
    }

    // Report wallet files not in registry
    if !files_only.is_empty() {
        has_issues = true;
        print_wallet_list("Wallet files not in registry:", files_only, |address| {
            format!(
                "  - {} (wallet_{}.json exists but not registered)",
                address.address_with_prefix().yellow(),
                address.address_without_prefix()
            )
        });
    }

    has_issues
}

/// Print the final integrity summary
fn print_integrity_summary(
    has_issues: bool,
    no_wallets: bool,
    registry_only: &std::collections::HashSet<&TaprootAddressWithPrefix<NetworkUnchecked>>,
    files_only: &std::collections::HashSet<&TaprootAddressWithPrefix<NetworkUnchecked>>,
) {
    use colored::Colorize;

    if has_issues {
        println!("{}", "Integrity issues found!".red().bold());
        println!("Consider:");
        if !registry_only.is_empty() {
            println!("- Remove orphaned registry entries or restore missing wallet files");
        }
        if !files_only.is_empty() {
            println!("- Register untracked wallet files or remove them if not needed");
        }
    } else if no_wallets {
        println!(
            "{}",
            "No wallets found (this is normal for new installations)".blue()
        );
    } else {
        println!(
            "{}",
            "All wallets are properly registered and files exist!"
                .green()
                .bold()
        );
    }
}

/// Generic function to print a list of wallets with custom formatting
fn print_wallet_list<F>(
    header: &str,
    wallets: &std::collections::HashSet<&TaprootAddressWithPrefix<NetworkUnchecked>>,
    formatter: F,
) where
    F: Fn(&TaprootAddressWithPrefix<NetworkUnchecked>) -> String,
{
    println!("{}", header);
    for address in wallets {
        println!("{}", formatter(address));
    }
    println!();
}

pub(crate) fn get_mnemonic_from_wallet<T>(
    address: &TaprootAddressWithPrefix<T>,
) -> Result<Mnemonic, BridgeCliError>
where
    T: bitcoin::address::NetworkValidation,
    bitcoin::Address<T>: crate::structs::AddrDisplay,
{
    // Check if wallet file exists before prompting for passphrase
    let storage_dir = get_storage_dir()?;
    let wallet_file = storage_dir.join(format!("wallet_{}.json", address.address_without_prefix()));

    if !wallet_file.exists() {
        return Err(BridgeCliError::WalletNotFound(
            address.address_with_prefix(),
        ));
    }
    let passphrase = prompt_unlock_passphrase()?;

    let mnemonic = load_mnemonic(address, &passphrase)?;

    Ok(mnemonic)
}
