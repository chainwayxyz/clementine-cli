use secrecy::SecretBox;

pub(crate) type SecureString = SecretBox<String>;
pub(crate) type SecureByteSlice = SecretBox<[u8; 32]>;
pub(crate) trait AddressExt {
    fn is_taproot(&self) -> bool;
}

impl AddressExt for bitcoin::Address {
    fn is_taproot(&self) -> bool {
        self.address_type() == Some(bitcoin::AddressType::P2tr)
    }
}
