use anyhow::Result;
use futures::StreamExt;
use polymarket_client_sdk_v2::{
    clob::ws::{BookUpdate, Client as WsClient},
    types::U256,
};
use tokio::sync::mpsc;

pub async fn ws_task(asset_ids: Vec<U256>, tx: mpsc::Sender<BookUpdate>) -> Result<()> {
    let client = WsClient::default();
    let mut stream = Box::pin(client.subscribe_orderbook(asset_ids)?);

    while let Some(result) = stream.next().await {
        if let Ok(book) = result
            && tx.send(book).await.is_err()
        {
            break;
        }
    }

    Ok(())
}
