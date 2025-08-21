use secrecy::SecretBox;

pub(crate) type SecureString = SecretBox<String>;
pub(crate) type SecureByteSlice = SecretBox<[u8; 32]>;
