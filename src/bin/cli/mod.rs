#![allow(clippy::result_large_err)]

pub mod macros;
pub mod network;

use std::{
    fs, io,
    io::Write,
    path::{Path, PathBuf},
    str::FromStr,
};

use bitcoin::{
    Address, Amount, Network, OutPoint,
    address::{NetworkChecked, NetworkUnchecked},
    taproot::Signature,
};
use chrono::{TimeZone, Utc};
use colored::Colorize;
use eyre::eyre;

use dialoguer::{Input, Select, theme::ColorfulTheme};
use eyre::Result;
use url::Url;

use clementine_cli::config::{self, BridgeCliConfig, NetworkConfigs, UNSPENDABLE_XONLY_PUBKEY};
use clementine_cli::deposit::{
    self, CitreaAddress, DepositAddressStorageResult, DepositStatusWithVout,
    GetDepositAddressResult, get_all_deposit_address_details,
    get_deposit_address_details_for_deposit_address,
};
use clementine_cli::errors::BridgeCliError;
use clementine_cli::secure_display::{display_mnemonic_securely, display_private_key_securely};
use clementine_cli::secure_types::SecureString;
use clementine_cli::wallet::{
    BitcoinAddress, Purpose, TaprootAddressWithPrefix, backup_wallet, create_encrypted_wallet,
    derive_and_validate_mnemonic_import, ensure_wallet_exists, get_mnemonic_from_wallet,
    get_private_key_from_wallet, import_wallet_from_file, import_wallet_from_mnemonic,
    import_wallet_from_private_key, load_key_with_purpose_check, prompt_mnemonic,
    prompt_passphrase, prompt_unlock_passphrase, validate_imported_wallet_file,
    validate_wallet_availability,
};
use clementine_cli::withdraw::{self, generate_withdrawal_signatures, start_withdrawal};
use clementine_cli::{
    MempoolTx, UtxoInfo, backend_deposit_status, backend_withdrawal_status,
    get_block_height_for_tx, get_current_block_height, get_mempool_txs, get_tx_details, get_utxos,
    send_withdrawal_signature_to_operators,
};

use bitcoin::XOnlyPublicKey;
use crossterm::cursor::{MoveToColumn, SavePosition};
use crossterm::terminal::{Clear, ClearType};
use crossterm::{
    cursor::MoveUp,
    event::{Event, KeyEventKind, poll, read},
    execute,
};
use std::time::Duration;
use tempfile::NamedTempFile;
use toml_edit::{DocumentMut, Item, Value, value};
use zeroize::Zeroizing;

/// Parse aggregated public key from hex string and warn if it differs from config
fn parse_and_validate_aggregated_key(
    key_hex: &str,
    config_key: XOnlyPublicKey,
) -> Result<XOnlyPublicKey, BridgeCliError> {
    if key_hex.len() != 64 {
        return Err(BridgeCliError::Eyre(eyre::eyre!(
            "aggregated_public_key must be exactly 64 hex characters, got {}",
            key_hex.len()
        )));
    }

    let key = XOnlyPublicKey::from_str(key_hex).map_err(|e| {
        tracing::error!("Failed to parse aggregated_public_key: {}", e);
        BridgeCliError::Eyre(eyre::eyre!("Failed to parse aggregated_public_key"))
    })?;

    if key != config_key {
        println!(
            "{} Using aggregated_public_key different from config",
            "WARNING".bold()
        );
        println!("  Config key:   {}", config_key);
        println!("  Provided key: {}", key);
    }

    Ok(key)
}

/// Initialize the Clementine CLI environment.
///
/// Creates the Clementine home and keys directories (with secure
/// permissions on Unix), and writes a default `bridge_cli_config.toml` if
/// one does not already exist.
pub fn cli_init() -> Result<(), BridgeCliError> {
    println!("{}", "Initializing Clementine CLI...".bold());
    let clementine_home_dir = get_clementine_home_dir()?;
    std::fs::create_dir_all(&clementine_home_dir).map_err(|e| {
        tracing::error!(
            "Failed to create Clementine home directory {}: {}",
            clementine_home_dir.display(),
            e
        );
        BridgeCliError::Eyre(eyre!(
            "Failed to create Clementine home directory {}",
            clementine_home_dir.display()
        ))
    })?;

    #[cfg(unix)]
    {
        set_permissions(&clementine_home_dir, 0o700)?;
    }

    println!(
        "{} Storage directory initialized at: {}",
        "SUCCESS".bold(),
        clementine_home_dir.display()
    );

    let config_file = clementine_home_dir.join("bridge_cli_config.toml");
    if config_file.exists() {
        println!(
            "{} Configuration file already exists at: {}",
            "INFO".bold(),
            config_file.display()
        );

        print!("Do you want to overwrite it with a fresh configuration? [y/N]: ");
        io::stdout().flush().ok();

        let mut answer = String::new();
        io::stdin().read_line(&mut answer).map_err(|e| {
            tracing::error!("Failed to read input: {}", e);
            BridgeCliError::Eyre(eyre!("Failed to read input"))
        })?;

        let overwrite = matches!(answer.trim().to_lowercase().as_str(), "y" | "yes");

        if !overwrite {
            println!("{} Keeping existing configuration.", "INFO".bold());
            return Ok(());
        }
    }

    let mut cfgs = if config_file.exists() {
        let contents = fs::read_to_string(&config_file).map_err(|e| {
            tracing::error!(
                "Failed to read existing configuration, {}: {}",
                config_file.display(),
                e
            );
            BridgeCliError::Eyre(eyre!(
                "Failed to read existing configuration '{}'",
                config_file.display()
            ))
        })?;

        toml::from_str::<NetworkConfigs>(&contents).map_err(|e| {
            tracing::error!(
                "Failed to parse existing configuration, {}: {}",
                config_file.display(),
                e
            );
            BridgeCliError::Eyre(eyre!(
                "Failed to parse existing configuration '{}'",
                config_file.display()
            ))
        })?
    } else {
        config::default_networks()
    };

    // Reset mainnet/testnet4 to defaults before interactive network setup,
    // while keeping other config sections intact.
    cfgs.bitcoin = BridgeCliConfig::defaults_for(Network::Bitcoin);
    cfgs.testnet4 = BridgeCliConfig::defaults_for(Network::Testnet4);

    setup_networks(&mut cfgs)?;
    config::write_config_to(&config_file, &cfgs)?;
    println!(
        "{} Default configuration file created at: {}",
        "SUCCESS".bold(),
        config_file.display()
    );
    println!("Clementine CLI initialized successfully.");
    Ok(())
}

/// Interactive setup for the known Bitcoin networks.
///
/// Prompts users to choose Bitcoin Esplora API provider for mainnet and testnet4.
/// Signet and regtest use their default configurations without prompting.
pub fn setup_networks(cfgs: &mut NetworkConfigs) -> Result<()> {
    let theme = ColorfulTheme::default();

    // All networks use Bitcoin Esplora API only (no Bitcoin Core RPC)
    for net in [&mut cfgs.bitcoin, &mut cfgs.testnet4] {
        net.bitcoin_config = None;
    }

    struct ApiProvider {
        name: &'static str,
        url: &'static str,
    }

    const BLOCKSTREAM_MAINNET: ApiProvider = ApiProvider {
        name: "Blockstream.info",
        url: "https://blockstream.info/api/",
    };
    const MEMPOOL_MAINNET: ApiProvider = ApiProvider {
        name: "Mempool.space",
        url: "https://mempool.space/api/",
    };
    const MEMPOOL_TESTNET4: ApiProvider = ApiProvider {
        name: "Mempool.space",
        url: "https://mempool.space/testnet4/api/",
    };

    let networks: Vec<(&str, &mut BridgeCliConfig, Vec<&ApiProvider>)> = vec![
        (
            "mainnet",
            &mut cfgs.bitcoin,
            vec![&BLOCKSTREAM_MAINNET, &MEMPOOL_MAINNET],
        ),
        ("testnet4", &mut cfgs.testnet4, vec![&MEMPOOL_TESTNET4]),
    ];

    for (name, net, providers) in networks {
        println!(
            "\n== Configure Bitcoin Esplora API for '{}' network ==",
            name
        );

        let mut menu_items: Vec<String> = providers
            .iter()
            .map(|p| format!("{} ({})", p.name, p.url))
            .collect();
        menu_items.push("Custom URL".to_string());

        let choice = Select::with_theme(&theme)
            .with_prompt(format!("Choose Bitcoin Esplora API provider for {}", name))
            .items(&menu_items)
            .default(0)
            .interact()?;

        let api_url = if choice < providers.len() {
            // Selected a predefined provider
            let provider = providers[choice];
            net.esplora_rest_api
                .clone()
                .unwrap_or_else(|| Url::parse(provider.url).expect("Provider URL should be valid"))
        } else {
            let custom_url: String = Input::with_theme(&theme)
                .with_prompt(format!("[{}] Custom Bitcoin Esplora API URL", name))
                .validate_with(|input: &String| -> Result<(), String> {
                    Url::parse(input).map(|_| ()).map_err(|e| {
                        tracing::error!("Invalid URL {}: {}", input, e);
                        "Invalid URL".to_string()
                    })
                })
                .interact_text()?;

            Url::parse(&custom_url).map_err(|e| {
                tracing::error!("Invalid custom URL {}: {}", custom_url, e);
                eyre!("Invalid custom URL")
            })?
        };

        let mut url_str = api_url.to_string();
        if !url_str.ends_with('/') {
            url_str.push('/');
        }

        net.esplora_rest_api = Some(Url::parse(&url_str).map_err(|e| {
            tracing::error!(
                "Failed to parse URL with trailing slash, {}: {}",
                url_str,
                e
            );
            eyre!("Failed to parse URL with trailing slash")
        })?);

        println!(
            "{} {} Bitcoin Esplora API set to: {}",
            "✓".bold().green(),
            name,
            net.esplora_rest_api.as_ref().unwrap()
        );
    }

    Ok(())
}

fn network_table_name(network: Network) -> Result<&'static str, BridgeCliError> {
    match network {
        Network::Bitcoin => Ok("bitcoin"),
        Network::Testnet4 => Ok("testnet4"),
        Network::Signet => Ok("signet"),
        Network::Regtest => Ok("regtest"),
        _ => Err(BridgeCliError::Eyre(eyre!(
            "Unsupported network for config operation: {}",
            network
        ))),
    }
}

/// Set directory permissions on Unix. Mode is a raw permission bits value
/// (e.g. 0o700). The function validates the mode is in the canonical range
/// (0..=0o777) and returns a `BridgeCliError` on failure.
///
/// Note: This function does NOT support special bits like sticky (0o1000),
/// setgid (0o2000), or setuid (0o4000). It only allows basic rwx permissions.
#[cfg(unix)]
fn set_permissions(path: &Path, mode: u32) -> Result<(), BridgeCliError> {
    if mode > 0o777 {
        return Err(BridgeCliError::Eyre(eyre!(
            "Invalid permission mode: {:o}. Must be <= 0o777",
            mode
        )));
    }
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).map_err(|e| {
        tracing::error!("Failed to set permissions for {}: {}", path.display(), e);
        BridgeCliError::Eyre(eyre!("Failed to set permissions for {}", path.display()))
    })?;
    Ok(())
}

// Helper: read and parse TOML config into a mutable document
fn parse_config_to_doc(config_path: &Path) -> Result<DocumentMut, BridgeCliError> {
    let contents = std::fs::read_to_string(config_path).map_err(|e| {
        tracing::error!(
            "Failed to read config file {}: {}",
            config_path.display(),
            e
        );
        BridgeCliError::Eyre(eyre!(
            "Failed to read config file {}",
            config_path.display()
        ))
    })?;

    let doc = contents.parse::<DocumentMut>().map_err(|e| {
        tracing::error!(
            "Failed to parse TOML config {}: {}",
            config_path.display(),
            e
        );
        BridgeCliError::Eyre(eyre!(
            "Failed to parse TOML config {}",
            config_path.display()
        ))
    })?;

    Ok(doc)
}

// Helper: create a toml_edit::Item from an existing Value type and a new string
fn create_item_from_existing(
    existing_val: &Value,
    new_val_str: &str,
    item_path: &str,
) -> Result<Item, BridgeCliError> {
    let new_item: Item = match existing_val {
        Value::Boolean(_) => {
            let parsed = new_val_str.parse::<bool>().map_err(|e| {
                tracing::error!(
                    "Failed to parse boolean value '{}' for path '{}': {}",
                    new_val_str,
                    item_path,
                    e
                );
                BridgeCliError::Eyre(eyre!(
                    "Failed to parse '{}' as boolean for {}",
                    new_val_str,
                    item_path
                ))
            })?;
            value(parsed)
        }
        Value::Integer(_) => {
            let parsed = new_val_str.parse::<i64>().map_err(|e| {
                tracing::error!(
                    "Failed to parse integer value '{}' for path '{}': {}",
                    new_val_str,
                    item_path,
                    e
                );
                BridgeCliError::Eyre(eyre!(
                    "Failed to parse '{}' as integer for {}",
                    new_val_str,
                    item_path
                ))
            })?;
            value(parsed)
        }
        Value::Float(_) => {
            let parsed = new_val_str.parse::<f64>().map_err(|e| {
                tracing::error!(
                    "Failed to parse float value '{}' for path '{}': {}",
                    new_val_str,
                    item_path,
                    e
                );
                BridgeCliError::Eyre(eyre!(
                    "Failed to parse '{}' as float for {}",
                    new_val_str,
                    item_path
                ))
            })?;
            value(parsed)
        }
        Value::String(_) => value(new_val_str.to_string()),
        other => {
            tracing::warn!(
                "Updating {} with unsupported existing type ({:?}), writing as string",
                item_path,
                other
            );
            value(new_val_str.to_string())
        }
    };

    Ok(new_item)
}

// Helper: prompt the user for confirmation (or respect assume_yes)
fn prompt_confirm(
    assume_yes: bool,
    item_path: &str,
    old_display: &str,
    new_display: &str,
) -> Result<bool, BridgeCliError> {
    if assume_yes {
        return Ok(true);
    }

    print!(
        "Update '{}' from '{}' to '{}'? [y/N]: ",
        item_path, old_display, new_display
    );
    io::stdout().flush().ok();
    let mut answer = String::new();
    io::stdin().read_line(&mut answer).map_err(|e| {
        tracing::error!("Failed to read input: {}", e);
        BridgeCliError::Eyre(eyre!("Failed to read input"))
    })?;
    Ok(matches!(answer.trim().to_lowercase().as_str(), "y" | "yes"))
}

// Helper: write the modified DocumentMut to a temp file and persist atomically
fn persist_doc_atomic(doc: &DocumentMut, config_path: &Path) -> Result<(), BridgeCliError> {
    let clementine_home_dir = get_clementine_home_dir_with_existence_check()?;
    let dir = clementine_home_dir;
    let mut tmp = NamedTempFile::new_in(&dir).map_err(|e| {
        tracing::error!("Failed to create temp file in '{}': {}", dir.display(), e);
        BridgeCliError::Eyre(eyre!("Failed to create temp file in {}", dir.display()))
    })?;

    tmp.write_all(doc.to_string().as_bytes()).map_err(|e| {
        tracing::error!("Failed to write to temp config file: {}", e);
        BridgeCliError::Eyre(eyre!("Failed to write to temp config file"))
    })?;
    tmp.as_file().sync_all().map_err(|e| {
        tracing::error!("Failed to flush temp config file: {}", e);
        BridgeCliError::Eyre(eyre!("Failed to flush temp config file"))
    })?;

    tmp.persist(config_path).map_err(|e| {
        tracing::error!(
            "Failed to persist temp config file: {}: {}",
            config_path.display(),
            e.error
        );
        BridgeCliError::Eyre(eyre!(
            "Failed to persist temp config file to {}",
            config_path.display()
        ))
    })?;

    Ok(())
}

/// Update configuration values in the bundled TOML config for a network table.
///
/// This function loads the config file, looks up keys inside the selected
/// network table (dot-separated keys target nested tables), and updates only
/// existing scalar values. For each change it asks the user to confirm unless
/// `assume_yes` is true. If any updates are applied the config file is written
/// atomically and a short summary is printed.
///
/// Parameters:
/// - `network`: Which network table to update (e.g. `bitcoin`, `testnet4`).
/// - `values`: Vec of `(key, new_value)` pairs. Keys may be dot-separated to
///   address nested tables.
/// - `assume_yes`: If true, skip interactive confirmation and apply changes.
///
/// Returns `Ok(())` on success. Errors are returned if the config cannot be
/// read/parsed, a key path does not exist, the existing value is non-scalar,
/// type parsing of the new value fails, or writing the updated config fails.
pub fn update_config_with_confirm(
    network: Network,
    values: Vec<(String, String)>,
    assume_yes: bool,
) -> Result<(), BridgeCliError> {
    let config_path = get_clementine_config_path_with_existence_check()?;
    let mut doc = parse_config_to_doc(&config_path)?;

    let table_name = network_table_name(network)?;

    if !doc.as_table().contains_key(table_name) {
        return Err(BridgeCliError::Eyre(eyre!(
            "Config table '{}' not found in {}",
            table_name,
            config_path.display()
        )));
    }

    let mut applied_updates: Vec<(String, String, String)> = Vec::new();

    for (key, new_val_str) in values.into_iter() {
        let item_path = format!("{}.{}", table_name, key);

        let parts: Vec<&str> = key.split('.').collect();

        let mut table = doc[table_name].as_table_mut().ok_or_else(|| {
            BridgeCliError::Eyre(eyre!(
                "Config table '{}' is not a table in {}",
                table_name,
                config_path.display()
            ))
        })?;

        for part in parts.iter().take(parts.len().saturating_sub(1)) {
            if !table.contains_key(part) {
                return Err(BridgeCliError::Eyre(eyre!(
                    "Config path '{}' does not exist (missing table '{}')",
                    item_path,
                    part
                )));
            }
            if !table[part].is_table() {
                return Err(BridgeCliError::Eyre(eyre!(
                    "Config path '{}' expected '{}' to be a table",
                    item_path,
                    part
                )));
            }
            table = table[part].as_table_mut().ok_or_else(|| {
                BridgeCliError::Eyre(eyre!("Failed to access table '{}' in {}", part, item_path))
            })?;
        }

        let last = parts.last().unwrap();

        let existing_item = table.get(last).ok_or_else(|| {
            BridgeCliError::Eyre(eyre!(
                "Config key '{}' does not exist; refusing to create new keys. Please run 'clementine-cli show-config' to see existing keys.",
                item_path
            ))
        })?;

        let existing_val = existing_item.as_value().ok_or_else(|| {
            BridgeCliError::Eyre(eyre!(
                "Config key '{}' is not a scalar value; refusing to overwrite",
                item_path
            ))
        })?;

        let new_item: Item = create_item_from_existing(existing_val, &new_val_str, &item_path)?;

        let old_display = existing_val.clone().decorated("", "").to_string();
        let new_display = if let Some(v) = new_item.as_value() {
            v.to_string()
        } else {
            new_item.to_string()
        };

        let proceed = prompt_confirm(assume_yes, &item_path, &old_display, &new_display)?;

        if proceed {
            table[last] = new_item;
            applied_updates.push((item_path.clone(), old_display, new_display));
        } else {
            println!("Skipped {}", item_path);
        }
    }

    if !applied_updates.is_empty() {
        persist_doc_atomic(&doc, &config_path)?;

        println!(
            "{} Configuration updated for '{}' table in {}",
            "SUCCESS".bold(),
            table_name,
            config_path.display()
        );
    } else {
        println!("No changes applied.");
    }

    if !applied_updates.is_empty() {
        println!("\nApplied updates:");
        for (path, old, new) in applied_updates.iter() {
            println!("  - {}: {} -> {}", path, old, new);
        }
    }

    Ok(())
}

pub fn cli_show_config(network: Network) -> Result<(), BridgeCliError> {
    let config_path = get_clementine_config_path_with_existence_check()?;

    let contents = std::fs::read_to_string(&config_path).map_err(|e| {
        tracing::error!(
            "Failed to read config file {}: {}",
            config_path.display(),
            e
        );
        BridgeCliError::Eyre(eyre!(
            "Failed to read config file {}",
            config_path.display()
        ))
    })?;

    let doc = contents.parse::<DocumentMut>().map_err(|e| {
        tracing::error!(
            "Failed to parse TOML config {}: {}",
            config_path.display(),
            e
        );
        BridgeCliError::Eyre(eyre!(
            "Failed to parse TOML config {}",
            config_path.display()
        ))
    })?;

    let table_name = network_table_name(network)?;

    let root_table = doc.as_table();
    if !root_table.contains_key(table_name) {
        return Err(BridgeCliError::Eyre(eyre!(
            "Config table '{}' not found in {}",
            table_name,
            config_path.display()
        )));
    }

    let table = doc[table_name].as_table().ok_or_else(|| {
        BridgeCliError::Eyre(eyre!(
            "Config '{}' is not a table in {}",
            table_name,
            config_path.display()
        ))
    })?;

    println!("{} Configuration ({}):", "INFO".bold(), table_name);

    fn print_item(prefix: &str, key: &str, item: &toml_edit::Item, depth: usize) {
        let indent = "  ".repeat(depth);
        if let Some(val) = item.as_value() {
            let val = val.clone().decorated("", "");
            println!("{}{}{} = {}", indent, prefix, key, val);
        } else if item.is_table() {
            println!("{}[{}{}]", indent, prefix, key);
            if let Some(tbl) = item.as_table() {
                for (k, v) in tbl.iter() {
                    print_item(&format!("{}{}.", prefix, key), k, v, depth + 1);
                }
            }
        } else {
            println!("Invalid item at {}{}{}", indent, prefix, key);
        }
    }

    for (k, v) in table.iter() {
        print_item("", k, v, 0);
    }

    Ok(())
}
pub async fn cli_create_wallet(
    network: Network,
    label: String,
    purpose: Purpose,
) -> Result<TaprootAddressWithPrefix<NetworkChecked>, BridgeCliError> {
    // Duplicate pre-check before passphrase prompt for better UX
    validate_wallet_availability(Some(&label), None, None).await?;

    let passphrase = prompt_passphrase(true)?;

    let (address, mnemonic) =
        create_encrypted_wallet(network, label, purpose, passphrase, None).await?;
    let _ = crossterm::terminal::enable_raw_mode();
    print!("\r\n");
    print!(
        "{} Wallet created with address: {}\r\n",
        "SUCCESS".bold(),
        address.address_with_prefix()
    );

    if purpose == Purpose::Deposit {
        print!(
            "{} Please do not send funds directly to this address!\r\n",
            "WARNING".bold(),
        );
    }

    print!("Press any key to continue...\r\n");
    io::stdout().flush().ok();

    // Save the cursor *after* the prompt (the natural place you want to end up)
    execute!(io::stdout(), SavePosition)?;

    loop {
        if poll(Duration::from_secs(60)).unwrap_or(false)
            && let Ok(Event::Key(key_event)) = read()
            && key_event.kind == KeyEventKind::Press
        {
            // Remove the prompt line but keep the cursor where it naturally was
            execute!(
                io::stdout(),
                MoveUp(1),
                Clear(ClearType::CurrentLine),
                MoveToColumn(0),
            )?;
            break;
        }
    }

    let _ = crossterm::terminal::disable_raw_mode();

    display_mnemonic_securely(&mnemonic)?;

    Ok(address)
}

pub async fn cli_backup_wallet(
    address_with_prefix: &str,
    destination_path: &str,
) -> Result<(TaprootAddressWithPrefix<NetworkUnchecked>, PathBuf), BridgeCliError> {
    let address = TaprootAddressWithPrefix::from_string_with_prefix_unchecked(address_with_prefix)?;

    // Pre-check to ensure wallet exists before prompting for passphrase for better UX
    ensure_wallet_exists(&address, None).await?;

    let dest_path = Path::new(destination_path);
    let final_dest = backup_wallet(&address, dest_path, None).await?;

    Ok((address, final_dest))
}

pub async fn cli_import_wallet_from_mnemonic(
    network: Network,
    label: &str,
    purpose: Purpose,
) -> Result<TaprootAddressWithPrefix<NetworkChecked>, BridgeCliError> {
    // Duplicate pre-check before mnemonic prompt for better UX
    validate_wallet_availability(Some(label), None, None).await?;
    println!("{}", "Import Wallet with Mnemonic".bold());

    let mnemonic = prompt_mnemonic()?;

    // Pre-check derived address before prompting for passphrase
    let _derived_address =
        derive_and_validate_mnemonic_import(network, None, purpose, &mnemonic, None).await?;

    let passphrase = prompt_passphrase(true)?;

    import_wallet_from_mnemonic(network, label, purpose, mnemonic, passphrase, None).await
}

pub async fn cli_import_wallet_from_file(
    file_path: &str,
    label: Option<&str>,
) -> Result<TaprootAddressWithPrefix<NetworkChecked>, BridgeCliError> {
    let file_path = Path::new(file_path);

    // Duplicate pre-check before passphrase prompt for better UX
    validate_imported_wallet_file(file_path, label, None).await?;

    let passphrase = prompt_unlock_passphrase()?;
    import_wallet_from_file(file_path, label, passphrase, None).await
}

pub async fn cli_import_wallet_from_private_key(
    network: Network,
    label: &str,
    purpose: Purpose,
) -> Result<TaprootAddressWithPrefix<NetworkChecked>, BridgeCliError> {
    // Duplicate pre-check before passphrase prompt for better UX
    validate_wallet_availability(Some(label), None, None).await?;

    let private_key_input = rpassword::prompt_password("Enter your private key (hex format): ")
        .map_err(|e| {
            tracing::error!("Failed to read private key: {}", e);
            BridgeCliError::Eyre(eyre!("Failed to read private key"))
        })?;

    let secure_private_key = SecureString::init_with(|| private_key_input);

    let passphrase = prompt_passphrase(true)?;

    import_wallet_from_private_key(
        network,
        label,
        purpose,
        secure_private_key,
        passphrase,
        None,
    )
    .await
}

/// Show mnemonic securely for a wallet
pub async fn cli_show_mnemonic(
    address: &TaprootAddressWithPrefix<bitcoin::address::NetworkUnchecked>,
) -> Result<(), BridgeCliError> {
    // Pre-check to ensure wallet exists before prompting for passphrase for better UX
    ensure_wallet_exists(address, None).await?;
    let passphrase = prompt_unlock_passphrase()?;
    let mnemonic = get_mnemonic_from_wallet(address, &passphrase, None).await?;
    display_mnemonic_securely(&mnemonic)?;
    Ok(())
}

/// Show private key securely for a wallet
pub async fn cli_show_private_key(
    address: &TaprootAddressWithPrefix<bitcoin::address::NetworkUnchecked>,
) -> Result<(), BridgeCliError> {
    // Pre-check to ensure wallet exists before prompting for passphrase for better UX
    ensure_wallet_exists(address, None).await?;
    let passphrase = prompt_unlock_passphrase()?;
    let private_key = get_private_key_from_wallet(address, &passphrase, None).await?;
    display_private_key_securely(&private_key)?;
    Ok(())
}

/// Send a safe withdrawal transaction with prompted secret key
pub async fn cli_send_safe_withdrawal(
    params: withdraw::SafeWithdrawalParams,
    config: &BridgeCliConfig,
) -> Result<alloy::rpc::types::TransactionReceipt, BridgeCliError> {
    let secret_key = prompt_secret_key()?;

    withdraw::send_safe_withdrawal(params, secret_key, config).await
}

pub async fn deposit_status(
    taproot_address: Address,
    config: &BridgeCliConfig,
) -> Result<(), BridgeCliError> {
    let mut utxos = match get_utxos(&taproot_address, config).await {
        Ok(utxos) => utxos,
        Err(_) => {
            eprintln!("ERROR Failed to fetch UTXOs from Esplora API");
            vec![]
        }
    };

    utxos.sort_by_key(|utxo| utxo.block_height.unwrap_or(u64::MAX));
    let deposits_with_incorrect_amount: Vec<&UtxoInfo> = utxos
        .iter()
        .filter(|utxo| utxo.value != config.bridge_amount)
        .collect();

    if !deposits_with_incorrect_amount.is_empty() {
        println!(
            "{} Deposits with incorrect amount for address {}:",
            "WARNING".bold(),
            taproot_address
        );
    }

    let current_block_height = get_current_block_height(config).await?;

    let refund_info = |block_height: Option<u64>, move_tx_on_chain: bool| {
        let refund_in_blocks = block_height.and_then(|h| {
            h.checked_add(config.user_takes_after)
                .map(|target| target.saturating_sub(current_block_height))
        });
        if move_tx_on_chain {
            let refund_threshold = config.user_takes_after / 4;
            match refund_in_blocks {
                Some(0) => "\n  You can refund your deposit now using 'deposit create-signed-recovery-tx' subcommand.".to_string(),
                Some(blocks) if blocks <= refund_threshold => {
                    format!("\n  Refund in (approx.) blocks: {}", blocks)
                }
                Some(_) => String::new(),
                None => "\n  Refund information not available.".to_string(),
            }
        } else {
            String::new()
        }
    };

    let block_display = |block_height: Option<u64>| {
        block_height
            .map(|h| h.to_string())
            .unwrap_or_else(|| "N/A".to_string())
    };

    for utxo in &deposits_with_incorrect_amount {
        let refund_msg = refund_info(utxo.block_height, true);
        print_incorrect_deposit(utxo, &refund_msg, &block_display(utxo.block_height));
    }

    if !deposits_with_incorrect_amount.is_empty() {
        println!();
    }

    let deposit_statuses_backend = match backend_deposit_status(&taproot_address, config).await {
        Ok(statuses) => statuses,
        Err(_) => {
            eprintln!("ERROR Failed to fetch deposit statuses from backend");
            vec![]
        }
    };

    let deposit_statuses_message = if deposit_statuses_backend.is_empty() {
        format!(
            "{} No confirmed deposits found for address {}",
            "INFO".bold(),
            taproot_address.to_string().bold()
        )
    } else {
        format!(
            "{} Deposit status(es) for address {}:",
            "INFO".bold(),
            taproot_address
        )
    };

    println!("{}", deposit_statuses_message);

    for status in &deposit_statuses_backend {
        let corresponding_utxo = utxos
            .iter()
            .find(|utxo| utxo.txid.to_string() == status.txid);
        let block_height = corresponding_utxo.and_then(|u| u.block_height);
        let tx_details = get_tx_details(&bitcoin::Txid::from_str(&status.txid)?, config).await;

        let (vout, found) = match tx_details {
            Ok((tx, _, _)) => {
                match tx
                    .output
                    .iter()
                    .position(|o| o.script_pubkey == taproot_address.script_pubkey())
                {
                    Some(v) => (v as u32, true),
                    None => {
                        return Err(BridgeCliError::Eyre(eyre!(
                            "Could not find vout for deposit txid {} and address {}",
                            status.txid,
                            taproot_address
                        )));
                    }
                }
            }
            Err(_) => (0, false),
        };

        let move_txid = status
            .move_txid
            .as_deref()
            .filter(|value| !value.is_empty());

        let move_block_height = if let Some(txid) = move_txid {
            get_block_height_for_tx(&bitcoin::Txid::from_str(txid)?, config)
                .await
                .ok()
        } else {
            None
        };

        let move_block_finalization_height =
            move_block_height.map(|h| h + config.move_tx_finalization_blocks - 1);

        let remaining_finalization_blocks = move_block_finalization_height
            .map(|finalization_height| finalization_height.saturating_sub(current_block_height));

        let deposit_status_with_vout = DepositStatusWithVout {
            deposit_status: status,
            vout: if found { Some(vout) } else { None },
            remaining_finalization_blocks,
        };

        let refund_msg = refund_info(block_height, move_txid.is_some());
        println!("{} {}", deposit_status_with_vout, refund_msg);
    }

    // Mempool transaction checking is only available with Esplora API
    if config.esplora_rest_api.is_some() {
        let mempool_txs = match get_mempool_txs(&taproot_address, config).await {
            Ok(txs) => txs,
            Err(_) => {
                eprintln!("ERROR Failed to fetch mempool transactions");
                vec![]
            }
        };

        if !mempool_txs.is_empty() {
            println!(
                "\n{} Deposit transactions in mempool for address {}:",
                "INFO".bold(),
                taproot_address
            );

            for tx in &mempool_txs {
                print_mempool_tx(&taproot_address, tx);
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn deposit_create_signed_recovery_tx(
    citrea_addr: &CitreaAddress,
    recovery_taproot_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    outpoint: &OutPoint,
    destination_addr: &BitcoinAddress,
    fee_rate: u64,
    amount: f64,
    config: &BridgeCliConfig,
    aggregated_public_key: String,
) -> Result<(), BridgeCliError> {
    // Pre-check to ensure wallet exists before prompting for passphrase for better UX
    ensure_wallet_exists(recovery_taproot_address, None).await?;

    let keypair =
        load_key_with_purpose_check(recovery_taproot_address, Purpose::Deposit, None).await?;

    // Parse and apply aggregated_public_key override if provided
    let mut effective_config = config.clone();
    let key =
        parse_and_validate_aggregated_key(&aggregated_public_key, config.aggregated_public_key)?;
    effective_config.aggregated_public_key = key;

    let recovery_params = deposit::RecoveryTxParams {
        citrea_addr: *citrea_addr,
        recovery_taproot_address: recovery_taproot_address.clone(),
        outpoint: *outpoint,
        destination_addr: destination_addr.clone(),
        fee_rate: Some(fee_rate),
        amount: Some(amount),
    };

    let tx = deposit::create_signed_recovery_tx(recovery_params, &effective_config, keypair, None)
        .await?;

    let raw_tx = hex::encode(bitcoin::consensus::serialize(&tx));
    println!("Raw transaction: {raw_tx}");
    println!();
    println!("Now you can broadcast the transaction using your preferred method.");

    Ok(())
}

pub async fn withdrawal_status(
    withdrawal_utxo: OutPoint,
    config: &BridgeCliConfig,
) -> Result<(), BridgeCliError> {
    let withdrawal_statuses = backend_withdrawal_status(withdrawal_utxo, config).await?;
    if withdrawal_statuses.is_empty() {
        println!(
            "{} No withdrawals found for OutPoint {}",
            "INFO".bold(),
            withdrawal_utxo.to_string().bold()
        );
        return Ok(());
    }

    println!(
        "{} Withdrawal status(es) for withdrawal OutPoint {}: \n",
        "INFO".bold(),
        withdrawal_utxo
    );

    for (i, status) in withdrawal_statuses.iter().enumerate() {
        println!("{}. {}", i + 1, status);
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn send_withdrawal_signature(
    signer_address: &str,
    destination_address: &str,
    withdrawal_utxo_outpoint: &str,
    amount: u64,
    signature: &str,
    config: &BridgeCliConfig,
) -> Result<(), BridgeCliError> {
    let withdrawal_outpoint = OutPoint::from_str(withdrawal_utxo_outpoint).map_err(|e| {
        tracing::error!(
            "Failed to parse withdrawal UTXO outpoint '{}': {}",
            withdrawal_utxo_outpoint,
            e
        );
        BridgeCliError::Eyre(eyre!(
            "Failed to parse withdrawal UTXO outpoint '{}'",
            withdrawal_utxo_outpoint,
        ))
    })?;
    send_withdrawal_signature_to_operators(
        signer_address,
        destination_address,
        withdrawal_outpoint,
        signature,
        config,
        amount,
    )
    .await?;
    Ok(())
}

fn print_incorrect_deposit(utxo: &UtxoInfo, refund_message: &str, block_display: &str) {
    let outpoint = format!("{}:{}", utxo.txid, utxo.vout);
    println!(
        "\nIncorrect Deposit\n  TxID:        {}\n  VOut:        {}\n  OutPoint:    {}\n  Value:       {}\n  Block:       {}{}",
        utxo.txid, utxo.vout, outpoint, utxo.value, block_display, refund_message
    );
}

fn print_mempool_tx(address: &BitcoinAddress, tx: &MempoolTx) {
    for out in &tx.vout {
        if let Some(addr_str) = out
            .get("scriptpubkey_address")
            .and_then(|addr| addr.as_str())
            && addr_str == address.to_string()
        {
            let value = out.get("value").and_then(|v| v.as_u64()).unwrap_or(0);
            println!("\nDeposit in Mempool");
            println!(
                "  TxID:       {}\n  Value:      {}",
                tx.txid,
                Amount::from_sat(value)
            );
        };
    }
}

/// Result of starting a deposit, including TapTree data
pub struct DepositAddressDetails {
    /// The deposit address
    pub deposit_address: BitcoinAddress,
    /// PSBT_OUT_TAP_TREE serialized in BIP-371 format (hex-encoded)
    pub tap_tree_hex: String,
    /// Internal key (unspendable) used in the TapTree (hex-encoded)
    pub internal_key_hex: String,
}

pub async fn cli_start_deposit(
    citrea_address: &CitreaAddress,
    recovery_taproot_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    config: &BridgeCliConfig,
) -> Result<DepositAddressDetails, BridgeCliError> {
    let GetDepositAddressResult {
        deposit_address,
        storage_result,
        tap_tree_hex,
    } = deposit::get_deposit_address(citrea_address, recovery_taproot_address, config, None)
        .await?;

    if storage_result == DepositAddressStorageResult::Exists {
        println!(
            "{} Deposit address {} already exists, please check the details using 'get-deposit-address-details' command.",
            "INFO".bold(),
            deposit_address
        );
        println!();
    }

    // Get the internal key hex (unspendable key used in TapTree)
    let internal_key_hex = hex::encode(UNSPENDABLE_XONLY_PUBKEY.serialize());

    Ok(DepositAddressDetails {
        deposit_address,
        tap_tree_hex,
        internal_key_hex,
    })
}

pub async fn cli_start_withdrawal(
    signer_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    destination_address: &BitcoinAddress,
    config: &BridgeCliConfig,
) -> Result<(), BridgeCliError> {
    start_withdrawal(signer_address, destination_address, config).await?;
    Ok(())
}

pub async fn cli_scan_withdrawals(
    signer_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    destination_address: &BitcoinAddress,
    config: &BridgeCliConfig,
) -> Result<(), BridgeCliError> {
    // Print contextual information about WHY we're scanning
    println!("{} Scanning for Withdrawal UTXOs", "INFO".bold());
    println!(
        "Locating available UTXOs that can be used to withdraw funds from Citrea back to Bitcoin\n"
    );

    // Print WHERE information
    println!("{} Withdrawal Parameters", "SCANNING".bold());
    println!("  Network:             {}", config.network);
    println!(
        "  Signer Address:      {}",
        signer_address.address_with_prefix()
    );
    println!("  Destination Address: {}", destination_address);
    println!(
        "  Expected UTXO Amount: {} BTC ({} sats)",
        config.dust_utxo_amount.to_btc(),
        config.dust_utxo_amount.to_sat()
    );
    println!();

    let utxos = withdraw::scan_withdrawal(signer_address, destination_address, config).await;

    let mut utxos = utxos.inspect_err(|_| {
        eprintln!("{} Failed to scan withdrawals", "ERROR".bold());
    })?;

    utxos.sort_by_key(|utxo| utxo.block_height.unwrap_or(u64::MAX));

    let utxos_with_wrong_amount: Vec<_> = utxos
        .iter()
        .filter(|utxo| utxo.value != config.dust_utxo_amount)
        .collect();

    if !utxos_with_wrong_amount.is_empty() {
        eprintln!(
            "{} The following UTXOs have amounts different than {} BTC. They will be ignored for withdrawal operations.",
            "WARNING".bold(),
            config.dust_utxo_amount.to_btc()
        );
        for utxo in &utxos_with_wrong_amount {
            let outpoint = OutPoint {
                txid: utxo.txid,
                vout: utxo.vout,
            };
            let block_info = utxo
                .block_height
                .map(|h| format!("Block: {}", h))
                .unwrap_or_else(|| "Block: Unconfirmed".to_string());
            eprintln!(
                "  - OutPoint: {}, Amount: {}, {}",
                outpoint, utxo.value, block_info
            );
        }
        eprintln!(
            "Please ensure you send exactly {} BTC to the signer address for each withdrawal operation.",
            config.dust_utxo_amount.to_btc()
        );

        // sleep for 2 seconds to ensure user sees the warning
        std::thread::sleep(std::time::Duration::from_secs(2));

        println!();
    }

    utxos.retain(|utxo| utxo.value == config.dust_utxo_amount);

    // Now for valid UTXOs, check if the backend already has a withdrawal for them
    // Ask status of each UTXO
    let mut available_utxos: Vec<UtxoInfo> = Vec::new();
    let mut used_utxos: Vec<UtxoInfo> = Vec::new();
    for utxo_info in utxos.into_iter() {
        let outpoint = OutPoint {
            txid: utxo_info.txid,
            vout: utxo_info.vout,
        };
        if backend_withdrawal_status(outpoint, config)
            .await?
            .is_empty()
        {
            tracing::debug!("No withdrawal found for UTXO: {}", outpoint);
            // If no status is returned, it's available
            available_utxos.push(utxo_info);
        } else {
            used_utxos.push(utxo_info);
        }
    }

    if !used_utxos.is_empty() {
        println!(
            "{} Found UTXO(s) already used in withdrawal operations:",
            "WARNING".bold()
        );
        println!();
        for (idx, utxo) in used_utxos.iter().enumerate() {
            let outpoint = OutPoint {
                txid: utxo.txid,
                vout: utxo.vout,
            };
            println!("UTXO #{}", idx + 1);
            println!("  OutPoint:     {}", outpoint);
            println!(
                "  Amount:       {} BTC ({} sats)",
                utxo.value.to_btc(),
                utxo.value.to_sat()
            );

            println!();
        }
        println!(
            "{} This address has been used in a withdrawal operation. Please avoid reusing it.",
            "IMPORTANT:".bold()
        );
        println!();
    }

    if available_utxos.is_empty() {
        eprintln!(
            "{} No valid withdrawal UTXOs found. Please send {} BTC first using 'withdraw start' command",
            "ERROR".bold(),
            config.dust_utxo_amount.to_btc()
        );
    } else {
        // Print WHEN information - showing details about found UTXOs
        println!(
            "{} Found {} valid withdrawal UTXO(s)",
            "SUCCESS".bold(),
            available_utxos.len()
        );
        println!();

        for (idx, utxo) in available_utxos.iter().enumerate() {
            let outpoint = OutPoint {
                txid: utxo.txid,
                vout: utxo.vout,
            };
            println!("UTXO #{}", idx + 1);
            println!("  OutPoint:     {}", outpoint);
            println!(
                "  Amount:       {} BTC ({} sats)",
                utxo.value.to_btc(),
                utxo.value.to_sat()
            );

            if let Some(block_height) = utxo.block_height {
                println!("  Block Height: {} (confirmed)", block_height);
                println!("  Status:       Ready for withdrawal");
            } else {
                println!("  Block Height: Unconfirmed (in mempool)");
                println!("  Status:       Waiting for confirmation before withdrawal");
            }
            println!();
        }

        let print_withdrawal_cmd = |outpoint: &OutPoint| {
            println!(
                "$ clementine-cli withdraw generate-withdrawal-signatures --network {} {} {} {}",
                config.network,
                &signer_address.address_with_prefix(),
                destination_address,
                outpoint,
            );
        };

        println!("{} Next Steps", "INSTRUCTIONS".bold());
        if available_utxos.len() == 1 {
            let utxo = &available_utxos[0];
            let outpoint = OutPoint {
                txid: utxo.txid,
                vout: utxo.vout,
            };

            if utxo.block_height.is_some() {
                println!("Your withdrawal UTXO is confirmed and ready to use.");
                println!("\nRun the following command to generate withdrawal signatures:");
                println!();
                print_withdrawal_cmd(&outpoint);
            } else {
                println!(
                    "Your withdrawal UTXO is unconfirmed. Please wait for it to be confirmed on the Bitcoin network."
                );
                println!(
                    "\nOnce confirmed, run the following command to generate withdrawal signatures:"
                );
                println!();
                print_withdrawal_cmd(&outpoint);
            }
        } else {
            println!(
                "{} Multiple UTXOs found. We advise using one UTXO for one withdrawal operation.",
                "WARNING".bold()
            );
            println!(
                "{} For your security: Use a unique signer address for each withdrawal.",
                "IMPORTANT".bold()
            );
            println!("\nChoose one of the following commands to generate withdrawal signatures:");
            println!();
            for utxo_info in available_utxos.iter() {
                let utxo = OutPoint {
                    txid: utxo_info.txid,
                    vout: utxo_info.vout,
                };
                print_withdrawal_cmd(&utxo);
                println!()
            }
        }
    }

    Ok(())
}

pub async fn cli_generate_withdrawal_signatures(
    signer_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    destination_address: &BitcoinAddress,
    withdrawal_utxo: &OutPoint,
    optimistic_withdrawal_amount: &Amount,
    operator_withdrawal_amount: &Amount,
    config: &BridgeCliConfig,
) -> Result<(Signature, Signature), BridgeCliError> {
    // Pre-check to ensure wallet exists before prompting for passphrase for better UX
    ensure_wallet_exists(signer_address, None).await?;
    let keypair = load_key_with_purpose_check(signer_address, Purpose::Withdrawal, None).await?;

    generate_withdrawal_signatures(
        keypair,
        signer_address,
        destination_address,
        withdrawal_utxo,
        optimistic_withdrawal_amount,
        operator_withdrawal_amount,
        config,
        None,
    )
    .await
}

pub async fn cli_verify_recovery_tx_with_validation(
    params: deposit::VerifyRecoveryTxParams,
    config: &BridgeCliConfig,
    aggregated_public_key: String,
) -> Result<(bitcoin::Txid, BitcoinAddress, Amount), BridgeCliError> {
    let effective_key =
        parse_and_validate_aggregated_key(&aggregated_public_key, config.aggregated_public_key)?;

    // Create effective config
    let mut effective_config = config.clone();
    effective_config.aggregated_public_key = effective_key;

    deposit::verify_recovery_tx(params, &effective_config)
}

pub async fn cli_list_all_deposit_addresses() -> Result<(), BridgeCliError> {
    let deposits = get_all_deposit_address_details(None).await.map_err(|e| {
        tracing::error!("Failed to retrieve stored deposit addresses: {}", e);
        BridgeCliError::Eyre(eyre!("Failed to retrieve stored deposit addresses"))
    })?;

    if deposits.is_empty() {
        println!("No stored deposit addresses found.");
    } else {
        println!("Stored Deposit Address(es):");
        for d in deposits {
            println!(
                "- Deposit address: {} | Network: {:?}",
                d.deposit_address, d.network
            );
        }
    }

    Ok(())
}

pub async fn cli_get_deposit_address_details(deposit_address: &str) -> Result<(), BridgeCliError> {
    // check if deposit_address is a valid Bitcoin address
    let _ = BitcoinAddress::from_str(deposit_address).map_err(|e| {
        tracing::error!("Invalid bitcoin address '{}': {}", deposit_address, e);
        BridgeCliError::Eyre(eyre!("Invalid bitcoin address '{}'", deposit_address,))
    })?;
    let details = get_deposit_address_details_for_deposit_address(deposit_address, None)
        .await
        .map_err(|e| {
            tracing::error!(
                "Failed to retrieve details for deposit address {}: {}",
                deposit_address,
                e
            );
            BridgeCliError::Eyre(eyre!(
                "Failed to retrieve details for deposit address {}",
                deposit_address
            ))
        })?;

    match details {
        None => {
            println!(
                "No stored details found for deposit address {}",
                deposit_address
            );
        }
        Some(d) => {
            println!("Deposit address details:");
            println!("  Deposit address: {}", d.deposit_address);
            println!("  Aggregated public key: {}", d.aggregated_public_key);
            println!("  Recovery taproot address: {}", d.recovery_taproot_address);
            println!("  Citrea address: {}", d.citrea_address);
            println!("  User takes after: {} blocks", d.user_takes_after);
            println!("  Network: {:?}", d.network);
            let created_at = Utc
                .timestamp_opt(d.created_at as i64, 0)
                .single()
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| format!("{} (invalid timestamp)", d.created_at));
            println!("  Created at: {}", created_at);
        }
    }

    Ok(())
}

/// Prompt user for a secret key (private key for Citrea transactions)
pub(crate) fn prompt_secret_key() -> Result<SecureString, BridgeCliError> {
    let secret_key = rpassword::prompt_password("Enter secret key: ").map_err(|e| {
        tracing::error!("Error reading secret key: {}", e);
        BridgeCliError::Eyre(eyre::eyre!("Failed to read secret key."))
    })?;

    let secret_key = Zeroizing::new(secret_key);
    let trimmed = secret_key.trim();

    if trimmed.len() != 64 {
        return Err(BridgeCliError::Eyre(eyre::eyre!(
            "Invalid secret key length. Expected 64 hex characters, got {}",
            trimmed.len()
        )));
    }

    Ok(SecureString::init_with(|| trimmed.to_string()))
}

fn get_clementine_home_dir_with_existence_check() -> Result<PathBuf, BridgeCliError> {
    let home_dir = get_clementine_home_dir()?;
    if !home_dir.exists() {
        return Err(BridgeCliError::Eyre(eyre::eyre!(
            "Clementine home directory not found at {:?}. Please run 'clementine-cli init' to create one.",
            home_dir
        )));
    }
    Ok(home_dir)
}

fn get_clementine_home_dir() -> Result<PathBuf, BridgeCliError> {
    let home_dir = dirs::home_dir().ok_or(BridgeCliError::HomeDirectoryNotFound)?;
    Ok(home_dir.join(".clementine"))
}

fn get_clementine_config_path_with_existence_check() -> Result<PathBuf, BridgeCliError> {
    let home_dir = get_clementine_home_dir()?;
    let config_path = home_dir.join("bridge_cli_config.toml");
    if !config_path.exists() {
        return Err(BridgeCliError::Eyre(eyre::eyre!(
            "Configuration file not found at {:?}. Please run 'clementine-cli init' to create one.",
            config_path
        )));
    }
    Ok(config_path)
}
