use crate::{
    BitcoinAddress,
    config::BridgeCliConfig,
    errors::BridgeCliError,
    structs::TaprootAddressWithPrefix,
    wallet::{Purpose, wallet_utils::address_exists},
};

pub(crate) async fn is_wallet_address(
    address: &BitcoinAddress,
    config: &BridgeCliConfig,
) -> Result<bool, BridgeCliError> {
    if address.address_type() == Some(bitcoin::AddressType::P2tr) {
        let address = TaprootAddressWithPrefix::from_string_without_prefix(
            &address.to_string(),
            Purpose::Withdrawal,
            config.network,
        )?;
        return address_exists(&address).await;
    }
    Ok(false)
}
