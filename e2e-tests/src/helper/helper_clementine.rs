use std::time::Duration;

use bitcoincore_rpc::RpcApi;
use citrea_e2e::{
    bitcoin::DEFAULT_FINALITY_DEPTH,
    clementine::{
        ClementineAggregator,
        client::clementine::{EntityStatuses, entity_status_with_id},
    },
};
use tracing::info;

pub async fn wait_until_all_state_managers_synced(
    bitcoin_client: &bitcoincore_rpc::Client,
    aggregator: &mut ClementineAggregator,
) -> anyhow::Result<()> {
    loop {
        let all_synced = are_all_state_managers_synced(bitcoin_client, aggregator)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to check state manager sync status: {}", e))?;

        if all_synced {
            return Ok(());
        }

        info!("Waiting for Clementine state managers to sync...");
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}
async fn are_all_state_managers_synced(
    bitcoin_client: &bitcoincore_rpc::Client,
    aggregator: &mut ClementineAggregator,
) -> anyhow::Result<bool> {
    let min_next_sync_height = get_min_next_state_manager_height(aggregator)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get minimum next state manager height: {}", e))?;
    let current_chain_height = bitcoin_client.get_block_count().await? as u32;
    let finality_depth = DEFAULT_FINALITY_DEPTH;
    let current_finalized_chain_height =
        current_chain_height.saturating_sub((finality_depth - 1) as u32);
    Ok(min_next_sync_height > current_finalized_chain_height)
}

/// Calls get_entity_statuses and returns the minimum next state manager height
async fn get_min_next_state_manager_height(
    aggregator: &mut ClementineAggregator,
) -> eyre::Result<u32> {
    let l1_sync_status = aggregator
        .client
        .get_entity_statuses(false)
        .await
        .map_err(|e| eyre::eyre!("Failed to get entity statuses: {}", e))?;

    let min_next_sync_height = get_next_sync_heights(l1_sync_status)
        .await?
        .into_iter()
        .min()
        .ok_or_else(|| eyre::eyre!("No entities found"))?;
    Ok(min_next_sync_height)
}

/// Get the minimum next state manager height from all the state managers
/// If automation is off for any entity, their state manager is assumed to be synced
/// (by setting their next height to u32::MAX).
async fn get_next_sync_heights(entity_statuses: EntityStatuses) -> eyre::Result<Vec<u32>> {
    entity_statuses
        .entity_statuses
        .into_iter()
        .map(|entity| {
            if let Some(entity_status_with_id::StatusResult::Status(status)) = entity.status_result
            {
                if status.automation {
                    Ok(status.state_manager_next_height.unwrap_or(0))
                } else {
                    // assume synced if automation is off
                    Ok(u32::MAX)
                }
            } else {
                Err(eyre::eyre!(
                    "Couldn't retrieve sync status from entity {:?}, status result: {:?}",
                    entity.entity_id,
                    entity.status_result
                ))
            }
        })
        .collect::<Result<Vec<_>, _>>()
}
