use crate::{
    backup_wallet, create_encrypted_wallet,
    errors::BridgeCliError,
    import_wallet_from_file, import_wallet_from_mnemonic, import_wallet_from_private_key,
    secure_display::display_mnemonic_securely,
    structs::{SecureString, TaprootAddressWithPrefix},
    wallet::{
        Purpose, get_mnemonic_from_wallet, get_private_key_from_wallet, get_registry_wallet_set,
        mnemonic::prompt_mnemonic,
        passphrase::{prompt_passphrase, prompt_unlock_passphrase},
        scan_wallet_files,
        wallet_storage::get_storage_dir,
        wallet_utils::{
            WalletValidationMode, ensure_wallet_exists, parse_and_validate_imported_wallet,
            report_integrity_results, validate_wallet_availability,
        },
    },
};
use bitcoin::{
    Network,
    address::{NetworkChecked, NetworkUnchecked},
};
use colored::Colorize;
use eyre::eyre;
use std::path::{Path, PathBuf};

pub fn cli_create_wallet(
    network: Network,
    label: String,
    purpose: Purpose,
) -> Result<TaprootAddressWithPrefix<NetworkChecked>, BridgeCliError> {
    // Duplicate pre-check before passphrase prompt for better UX
    validate_wallet_availability(Some(&label), None, WalletValidationMode::Label)?;

    let passphrase = prompt_passphrase(true)?;
    let (address, mnemonic) = create_encrypted_wallet(network, label, purpose, passphrase)?;

    println!(
        "{} Wallet created with address: {}",
        "SUCCESS".bold(),
        address.address_with_prefix()
    );

    // Sleep for 2 seconds to allow user to see the success message before showing mnemonic
    std::thread::sleep(std::time::Duration::from_secs(2));

    display_mnemonic_securely(&mnemonic)?;

    Ok(address)
}

pub fn cli_backup_wallet(
    address_with_prefix: &str,
    destination_path: &str,
) -> Result<(TaprootAddressWithPrefix<NetworkUnchecked>, PathBuf), BridgeCliError> {
    let address = TaprootAddressWithPrefix::from_string_with_prefix_unchecked(address_with_prefix)?;

    // Pre-check to ensure wallet exists before prompting for passphrase for better UX
    ensure_wallet_exists(&address)?;

    let dest_path = Path::new(destination_path);
    let final_dest = backup_wallet(&address, dest_path)?;

    Ok((address, final_dest))
}

pub fn cli_import_wallet_from_mnemonic(
    network: Network,
    label: &str,
    purpose: Purpose,
) -> Result<TaprootAddressWithPrefix<NetworkChecked>, BridgeCliError> {
    // Duplicate pre-check before mnemonic prompt for better UX
    validate_wallet_availability(Some(label), None, WalletValidationMode::Label)?;
    println!("{}", "Import Wallet with Mnemonic".bold());

    let mnemonic = prompt_mnemonic()?;

    import_wallet_from_mnemonic(network, label, purpose, mnemonic)
}

pub fn cli_verify_wallet_integrity() -> Result<(), BridgeCliError> {
    let storage_dir = get_storage_dir()?;

    println!("{}", "Verifying Wallet Integrity".bold());
    println!("Storage directory: {}", storage_dir.display());
    println!();

    // Load registered wallets from wallets.json
    let registry_wallets = get_registry_wallet_set()?;

    // Scan for actual wallet files in storage directory
    let file_wallets = scan_wallet_files()?;

    // Report integrity results
    report_integrity_results(&registry_wallets, &file_wallets);

    Ok(())
}

pub fn cli_import_wallet_from_file(
    file_path: &str,
    label: Option<&str>,
) -> Result<TaprootAddressWithPrefix<NetworkChecked>, BridgeCliError> {
    let file_path = Path::new(file_path);

    // Duplicate pre-check before passphrase prompt for better UX
    parse_and_validate_imported_wallet(file_path, label)?;

    let passphrase = prompt_unlock_passphrase()?;
    import_wallet_from_file(file_path, label, passphrase)
}

pub fn cli_import_wallet_from_private_key(
    network: Network,
    label: &str,
    purpose: Purpose,
) -> Result<TaprootAddressWithPrefix<NetworkChecked>, BridgeCliError> {
    // Duplicate pre-check before passphrase prompt for better UX
    validate_wallet_availability(Some(label), None, WalletValidationMode::Label)?;

    let private_key_input = rpassword::prompt_password("Enter your private key (hex format): ")
        .map_err(|e| BridgeCliError::Eyre(eyre!("Failed to read private key: {}", e)))?;

    let secure_private_key = SecureString::init_with(|| private_key_input);

    let passphrase = prompt_passphrase(true)?;

    import_wallet_from_private_key(network, label, purpose, secure_private_key, passphrase)
}

/// Show mnemonic securely for a wallet
pub fn cli_show_mnemonic(
    address: &TaprootAddressWithPrefix<bitcoin::address::NetworkUnchecked>,
) -> Result<(), BridgeCliError> {
    // Pre-check to ensure wallet exists before prompting for passphrase for better UX
    ensure_wallet_exists(address)?;
    let passphrase = prompt_unlock_passphrase()?;
    let mnemonic = get_mnemonic_from_wallet(address, &passphrase)?;
    crate::secure_display::display_mnemonic_securely(&mnemonic)?;
    Ok(())
}

/// Show private key securely for a wallet
pub fn cli_show_private_key(
    address: &TaprootAddressWithPrefix<bitcoin::address::NetworkUnchecked>,
) -> Result<(), BridgeCliError> {
    // Pre-check to ensure wallet exists before prompting for passphrase for better UX
    ensure_wallet_exists(address)?;
    let passphrase = prompt_unlock_passphrase()?;
    let private_key = get_private_key_from_wallet(address, &passphrase)?;
    crate::secure_display::display_private_key_securely(&private_key)?;
    Ok(())
}
