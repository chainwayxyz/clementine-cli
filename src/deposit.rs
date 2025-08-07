// Deposit-related commands and logic for Clementine CLI

use crate::EVMAddress;
use crate::backend::create_deposit_account;
use crate::bitcoin_utils::{
    calculate_deposit_address, confirm_private_key_storage, generate_key_and_taproot_address,
};
use crate::bitcoin_utils::{
    generate_keypair_and_taproot_address_from_private_key,
    sign_recovery_tx as utils_sign_recovery_tx,
};
use crate::parameters::get_citrea_deposit_params;
use crate::storage::load_key;
use crate::storage::store_key;
use crate::withdrawal::{get_tx_details, get_txout_details_from_rpc};
use bitcoin::AddressType;
use bitcoin::consensus::deserialize;
use bitcoin::{Address, Network, address::NetworkUnchecked};
use bitcoin::{Amount, FeeRate, OutPoint, Transaction, Txid};
use colored::*;
use std::str::FromStr;

pub fn parse_address(
    address: &str,
    network: Network,
) -> Result<Address, Box<dyn std::error::Error>> {
    let unchecked_address: Address<NetworkUnchecked> = address
        .parse()
        .map_err(|_| "Invalid Bitcoin address format")?;
    let address = unchecked_address.require_network(network)?;
    Ok(address)
}

/// Parse and validate taproot address for the specified network
pub fn parse_taproot_address(
    address: &str,
    network: Network,
) -> Result<Address, Box<dyn std::error::Error>> {
    let address = parse_address(address, network)?;

    // Verify it's a taproot (P2TR) address
    if address.address_type() != Some(AddressType::P2tr) {
        return Err("Address is not a taproot (P2TR) address".into());
    }

    Ok(address)
}

/// Generate a new recovery key and taproot address for deposit operations
pub fn generate_recovery_key(
    auto_yes: bool,
    private_key: Option<String>,
    network: Network,
) -> Result<(), Box<dyn std::error::Error>> {
    // Confirm with user about private key storage
    if !confirm_private_key_storage(auto_yes)? {
        println!("Operation cancelled by user.");
        return Ok(());
    }

    let (keypair, address) = if let Some(private_key) = private_key {
        generate_keypair_and_taproot_address_from_private_key(&private_key, network)
    } else {
        generate_key_and_taproot_address(network)
    }?;

    // Store the key securely
    let stored_address = store_key(&keypair, network, None)?;

    // Verify the stored address matches the generated one
    if stored_address != address {
        return Err("Address mismatch after storage".into());
    }

    println!("{} {}", "ADDRESS".cyan().bold(), address);
    println!("{} {}", "NETWORK".blue().bold(), network);

    Ok(())
}

/// Get deposit address from backend
pub fn get_deposit_address(
    citrea_address: &str,
    recovery_taproot_address: &str,
    network: Network,
) -> Result<(), Box<dyn std::error::Error>> {
    let citrea_address = EVMAddress::try_from(citrea_address)?;
    let recovery_taproot_address = parse_taproot_address(recovery_taproot_address, network)?;

    // Call backend to create deposit account
    let deposit_address =
        create_deposit_account(&citrea_address, &recovery_taproot_address, network)?;

    println!("{} {}", "DEPOSIT_ADDRESS".green().bold(), deposit_address);

    let (calculated_deposit_address, _) =
        calculate_deposit_address(&citrea_address, &recovery_taproot_address, network)?;

    assert_eq!(deposit_address, calculated_deposit_address);

    println!(
        "{} {}",
        "Deposit address:".blue().bold(),
        calculated_deposit_address
    );
    Ok(())
}

pub async fn get_deposit_params(
    move_to_vault_txid: &str,
    bitcoin_rpc_url: &str,
    bitcoin_rpc_user: &str,
    bitcoin_rpc_password: &str,
    network: Network,
) -> Result<(), Box<dyn std::error::Error>> {
    let move_to_vault_txid = Txid::from_str(move_to_vault_txid)?;
    // 2. Get the prepare tx details
    let (move_to_vault_tx, move_to_vault_block, move_to_vault_block_height) = get_tx_details(
        &move_to_vault_txid,
        Some(bitcoin_rpc_url),
        Some(bitcoin_rpc_user),
        Some(bitcoin_rpc_password),
        network,
    )
    .await?;

    let move_to_vault_txout = get_txout_details_from_rpc(
        bitcoin_rpc_url,
        bitcoin_rpc_user,
        bitcoin_rpc_password,
        &move_to_vault_tx.input[0].previous_output.txid,
        move_to_vault_tx.input[0].previous_output.vout,
    )
    .await?;

    let deposit_params = get_citrea_deposit_params(
        move_to_vault_txout,
        &move_to_vault_tx,
        &move_to_vault_block,
        move_to_vault_block_height,
    )?;

    println!("{}", "Encoded deposit params:".blue().bold());
    println!("{}", hex::encode(deposit_params));

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn sign_recovery_tx(
    evm_address: &str,
    recovery_taproot_address: &str,
    deposit_txid: &str,
    deposit_vout: u32,
    claim_address: &str,
    fee_rate: Option<u64>,
    amount: Option<f64>,
    network: Network,
) -> Result<(), Box<dyn std::error::Error>> {
    let evm_addr = EVMAddress::try_from(evm_address)?;
    let recovery_addr = parse_taproot_address(recovery_taproot_address, network)?;
    let claim_addr = Address::from_str(claim_address)?.require_network(network)?;
    let txid = Txid::from_str(deposit_txid)?;
    let outpoint = OutPoint {
        txid,
        vout: deposit_vout,
    };
    let keypair = load_key(recovery_taproot_address, network, None)?;

    // Convert BTC amount to satoshis if provided
    let deposit_amount = match amount {
        Some(btc) => Some(Amount::from_btc(btc)?),
        None => None,
    };

    let fee_rate_opt = fee_rate.map(FeeRate::from_sat_per_vb_unchecked);
    let signed_tx = utils_sign_recovery_tx(
        &keypair,
        &evm_addr,
        &recovery_addr,
        &outpoint,
        deposit_amount,
        &claim_addr,
        fee_rate_opt,
        network,
    )?;
    println!(
        "Signed Recovery Transaction: {}",
        hex::encode(bitcoin::consensus::serialize(&signed_tx))
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn verify_recovery_tx(
    recovery_tx: &str,
    evm_address: &str,
    recovery_taproot_address: &str,
    amount: Option<f64>,
    network: Network,
) -> Result<(Txid, Address, Amount), Box<dyn std::error::Error>> {
    let recovery_tx: Transaction = deserialize(&hex::decode(recovery_tx)?)?;

    let (txid, address, amount) = crate::bitcoin_utils::verify_recovery_tx(
        &recovery_tx,
        &EVMAddress::try_from(evm_address)?,
        &parse_taproot_address(recovery_taproot_address, network)?,
        amount.map(|amount| Amount::from_btc(amount).unwrap()),
        network,
    )?;

    println!(
        "{} Recovery transaction verification successful!",
        "SUCCESS".green().bold()
    );
    println!("{} {}", "Output address:".blue().bold(), address);
    println!("{} {} BTC", "Output amount:".blue().bold(), amount.to_btc());
    println!(
        "\n{} This transaction can be broadcast after 200 blocks from {}",
        "NOTE:".yellow().bold(),
        txid
    );

    Ok((txid, address, amount))
}

// TODO: Implement deposit.deposit_status

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_taproot_address_valid() {
        let addr_str = "bc1pdqrcrxa8vx6gy75mfdfj84puhxffh4fq46h3gkp6jxdd0vjcsdyspfxcv6";
        let addr = parse_taproot_address(addr_str, Network::Bitcoin).unwrap();
        assert_eq!(addr.address_type(), Some(AddressType::P2tr));
    }

    #[test]
    fn test_parse_taproot_address_invalid_type() {
        let non_taproot = "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx"; // P2WPKH
        assert!(parse_taproot_address(non_taproot, Network::Testnet).is_err());
    }

    #[test]
    fn test_parse_taproot_address_wrong_network() {
        let mainnet_addr = "bc1pqqqqp399et2xygdj5xreqhjjvcmzhxw4aywxecjdzew6hylgvsesf3hn0c";
        assert!(parse_taproot_address(mainnet_addr, Network::Testnet).is_err());
    }

    #[test]
    fn test_parse_taproot_address_invalid_format() {
        let invalid = "invalid_address";
        assert!(parse_taproot_address(invalid, Network::Testnet).is_err());
    }
}
