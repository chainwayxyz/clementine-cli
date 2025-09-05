use crate::{
    BitcoinAddress, CitreaAddress,
    backend::{backend_withdrawal_status, send_withdrawal_signatures_to_operators},
    config::BridgeCliConfig,
    deposit,
    errors::BridgeCliError,
    generate_withdrawal_signature,
    structs::TaprootAddressWithPrefix,
    wallet::Purpose,
    withdraw::{self, start_withdrawal},
};
use bitcoin::{Amount, Network, OutPoint, taproot::Signature};
use colored::Colorize;
use std::str::FromStr;

pub async fn withdrawal_status(
    withdrawal_index: u32,
    config: &BridgeCliConfig,
) -> Result<(), BridgeCliError> {
    let withdrawal_statuses = backend_withdrawal_status(withdrawal_index, config).await?;
    if withdrawal_statuses.is_empty() {
        println!(
            "{} No withdrawals found for index {}",
            "INFO".bold(),
            withdrawal_index.to_string().bold()
        );
        return Ok(());
    }

    println!(
        "{} Withdrawal status(es) for withdrawal index {}: \n",
        "INFO".bold(),
        withdrawal_index
    );

    for (i, status) in withdrawal_statuses.iter().enumerate() {
        println!("{}. {}", i + 1, status);
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn send_withdrawal_signatures(
    signer_address: &str,
    withdrawal_address: &str,
    withdrawal_utxo_outpoint: &str,
    amount: f64,
    signature: &str,
    config: &BridgeCliConfig,
    withdrawal_index: u32,
) -> Result<(), BridgeCliError> {
    let withdrawal_outpoint = OutPoint::from_str(withdrawal_utxo_outpoint)?;
    send_withdrawal_signatures_to_operators(
        signer_address,
        withdrawal_address,
        withdrawal_outpoint,
        withdrawal_index,
        signature,
        config,
        amount,
    )
    .await?;
    Ok(())
}

pub async fn cli_get_deposit_address(
    citrea_address: &CitreaAddress,
    recovery_taproot_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    config: &BridgeCliConfig,
) -> Result<BitcoinAddress, BridgeCliError> {
    let deposit_address =
        deposit::get_deposit_address(citrea_address, recovery_taproot_address, config).await?;
    Ok(deposit_address)
}

pub async fn cli_start_withdrawal(
    signer_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    claim_address: &BitcoinAddress,
    config: &BridgeCliConfig,
) -> Result<(), BridgeCliError> {
    start_withdrawal(signer_address, claim_address, config)?;
    Ok(())
}

pub async fn cli_scan_withdrawals(
    signer_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    claim_address: &BitcoinAddress,
    config: &BridgeCliConfig,
) -> Result<(), BridgeCliError> {
    let utxos = withdraw::scan_withdrawal(signer_address, claim_address, config).await;

    let mut utxos =
        utxos.inspect_err(|e| eprintln!("{} Failed to scan withdrawals: {}", "ERROR".bold(), e))?;

    utxos.sort_by_key(|(outpoint, _)| outpoint.txid);

    let utxos_with_wrong_amount: Vec<_> = utxos
        .iter()
        .filter(|(_, amount)| *amount != Amount::from_sat(330))
        .collect();

    if !utxos_with_wrong_amount.is_empty() {
        eprintln!(
            "{} The following UTXOs have amounts different than 0.00000330 btc. They will be ignored for withdrawal operations.",
            "WARNING".bold()
        );
        for (outpoint, amount) in utxos_with_wrong_amount {
            eprintln!(" - OutPoint: {}, Amount: {}", outpoint, amount);
        }
        eprintln!(
            "Please ensure you send exactly 0.00000330 btc to the signer address for each withdrawal operation."
        );

        // sleep for 2 seconds to ensure user sees the warning
        std::thread::sleep(std::time::Duration::from_secs(2));

        println!();
    }

    utxos.retain(|(_, amount)| *amount == Amount::from_sat(330));

    if utxos.is_empty() {
        eprintln!(
            "No UTXOs found. Please send 0.00000330 btc first using 'withdrawal start' command"
        );
    } else {
        let print_withdrawal_cmd = |outpoint: &_| {
            println!(
                "clementine-cli withdraw generate-withdrawal-signature --network {} {} {} {} {}",
                config.network,
                &signer_address.address_with_prefix(),
                claim_address,
                outpoint,
                config.optimistic_withdrawal_amount.to_btc()
            );
        };
        let print_operator_note = || {
            println!(
                "{} For operator-paid withdrawals, use the amount {}",
                "Important Note".bold(),
                config.operator_withdrawal_amount.to_btc()
            )
        };
        if utxos.len() == 1 {
            println!("Run:");
            let (outpoint, _) = &utxos[0];
            print_withdrawal_cmd(outpoint);
            print_operator_note();
        } else {
            println!(
                "{} Multiple UTXOs found, we advise to use one UTXO for one withdrawal operation",
                "WARNING".bold()
            );
            println!(
                "{} For your security: Use a unique signer address for each withdrawal.",
                "IMPORTANT NOTICE!".bold()
            );
            println!("Run one of these:");
            for (outpoint, _) in utxos.iter() {
                print_withdrawal_cmd(outpoint);
                println!()
            }
            print_operator_note();
        }
    }

    Ok(())
}

pub fn cli_generate_withdrawal_signature(
    signer_address: &TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    claim_address: &BitcoinAddress,
    withdrawal_utxo: &OutPoint,
    amount: &Amount,
    network: Network,
) -> Result<Signature, BridgeCliError> {
    let keypair = crate::wallet::wallet_utils::load_key_with_purpose_check(
        signer_address,
        Purpose::Withdrawal,
    )?;

    generate_withdrawal_signature(
        *keypair.as_ref(),
        signer_address,
        claim_address,
        withdrawal_utxo,
        amount,
        network,
    )
}
