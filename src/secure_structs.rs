use secrecy::ExposeSecret;
use secrecy::SecretBox;

pub type SecureString = SecretBox<String>;
pub type SecureByteSlice = SecretBox<[u8; 32]>;

pub trait SecureStringExt {
    fn is_empty(&self) -> bool;
}

impl SecureStringExt for SecureString {
    fn is_empty(&self) -> bool {
        self.expose_secret().is_empty()
    }
}
