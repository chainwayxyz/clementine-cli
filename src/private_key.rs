use crate::errors::BridgeCliError;
use crate::{encryption::aes_decrypt_secure, secure_structs::SecureString};

pub fn load_private_key_secure(
    wallet_name: &str,
    passphrase: &SecureString,
) -> Result<SecureString, BridgeCliError> {
    let wallet_data = crate::wallet_storage::load_wallet_data(wallet_name)?;

    let encrypted_data = if let Some(encrypted_private_key) = &wallet_data.encrypted_private_key {
        crate::encryption::encrypted_data_from_hex(encrypted_private_key)?
    } else {
        return Err(BridgeCliError::NoEncryptedPrivateKeyFound);
    };

    let secure_private_key = aes_decrypt_secure(&encrypted_data, passphrase)?;

    Ok(secure_private_key)
}
