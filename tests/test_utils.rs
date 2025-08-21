use bitcoin::Network;
use clementine_cli::{
    bitcoin_utils,
    encryption::{EncryptedData, aes_encrypt_secure},
    mnemonic::{generate_mnemonic_secure, get_master_seed_from_mnemonic},
    secure_structs::SecureString,
};
use std::fs;
use tempfile::TempDir;

/// Common test data structure
pub struct TestWalletData {
    pub address: String,
    pub secure_mnemonic: SecureString,
    pub secure_private_key: SecureString,
    pub encrypted_mnemonic: EncryptedData,
    pub encrypted_private_key: EncryptedData,
    pub passphrase: SecureString,
    pub network: Network,
}

/// Set up a temporary test storage directory
pub fn setup_test_storage() -> TempDir {
    tempfile::tempdir().expect("Failed to create temp directory")
}

/// Get test storage directory path
pub fn get_test_storage_dir(temp_dir: &TempDir) -> std::path::PathBuf {
    temp_dir.path().join(".clementine").join("keys")
}

/// Generate complete test wallet data with encryption
pub fn generate_test_wallet_data(
    network: Network,
    passphrase: &str,
) -> eyre::Result<TestWalletData> {
    let passphrase = SecureString::init_with(|| passphrase.to_string());
    let secure_mnemonic = generate_mnemonic_secure()?;

    let master_seed = get_master_seed_from_mnemonic(&secure_mnemonic)?;
    let master_private_key = bitcoin::secp256k1::SecretKey::from_slice(&master_seed)?;
    let keypair =
        bitcoin::secp256k1::Keypair::from_secret_key(&bitcoin_utils::SECP, &master_private_key);
    let address = bitcoin_utils::calculate_taproot_address(&keypair, network);

    let secure_private_key =
        SecureString::init_with(|| master_private_key.display_secret().to_string());

    let encrypted_mnemonic = aes_encrypt_secure(&secure_mnemonic, &passphrase)?;
    let encrypted_private_key = aes_encrypt_secure(&secure_private_key, &passphrase)?;

    Ok(TestWalletData {
        address: address.to_string(),
        secure_mnemonic,
        secure_private_key,
        encrypted_mnemonic,
        encrypted_private_key,
        passphrase,
        network,
    })
}

/// Store test wallet data to file
pub fn store_test_wallet(
    storage_dir: &std::path::Path,
    wallet_data: &TestWalletData,
) -> eyre::Result<std::path::PathBuf> {
    fs::create_dir_all(storage_dir)?;

    let wallet_file = storage_dir.join(format!("wallet_{}.json", wallet_data.address));

    let wallet_json = serde_json::json!({
        "address": wallet_data.address,
        "network": wallet_data.network.to_string(),
        "encrypted_mnemonic": {
            "ciphertext": hex::encode(&wallet_data.encrypted_mnemonic.ciphertext),
            "nonce": hex::encode(wallet_data.encrypted_mnemonic.nonce),
            "salt": hex::encode(wallet_data.encrypted_mnemonic.salt)
        },
        "encrypted_private_key": {
            "ciphertext": hex::encode(&wallet_data.encrypted_private_key.ciphertext),
            "nonce": hex::encode(wallet_data.encrypted_private_key.nonce),
            "salt": hex::encode(wallet_data.encrypted_private_key.salt)
        },
        "created_at": chrono::Utc::now().to_rfc3339(),
        "encryption_method": "aes256_gcm_argon2id_secure",
        "data_format": "separate_encrypted_fields"
    });

    fs::write(&wallet_file, serde_json::to_string_pretty(&wallet_json)?)?;
    Ok(wallet_file)
}

/// Load and verify wallet JSON structure
pub fn load_and_verify_wallet_json(
    wallet_file: &std::path::Path,
) -> eyre::Result<serde_json::Value> {
    let json_data = fs::read_to_string(wallet_file)?;
    let wallet_data: serde_json::Value = serde_json::from_str(&json_data)?;

    // Basic structure validation
    assert!(wallet_data["address"].is_string());
    assert!(wallet_data["network"].is_string());
    assert!(wallet_data["encrypted_mnemonic"].is_object());
    assert!(wallet_data["encrypted_private_key"].is_object());

    Ok(wallet_data)
}

/// Create EncryptedData from JSON object
pub fn encrypted_data_from_json(json_obj: &serde_json::Value) -> eyre::Result<EncryptedData> {
    Ok(EncryptedData {
        ciphertext: hex::decode(json_obj["ciphertext"].as_str().unwrap())?,
        nonce: hex::decode(json_obj["nonce"].as_str().unwrap())?
            .try_into()
            .map_err(|_| eyre::eyre!("Invalid nonce length"))?,
        salt: hex::decode(json_obj["salt"].as_str().unwrap())?
            .try_into()
            .map_err(|_| eyre::eyre!("Invalid salt length"))?,
    })
}
