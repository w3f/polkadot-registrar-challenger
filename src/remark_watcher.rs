// wss://rpc.polkadot.io
use crate::Result;
use frame_system::Call;
use substrate_subxt::system::{System, SystemEventsDecoder};
use substrate_subxt::{
    sp_core::{Decode, Encode},
    ClientBuilder, DefaultNodeRuntime, EventSubscription, EventsDecoder, extrinsic::DefaultExtra,
};
use tokio::time::{self, Duration};
use sp_runtime::{generic::UncheckedExtrinsic, MultiSignature, MultiSigner};

type Extrinsic<T: System> = UncheckedExtrinsic<MultiSigner, (), MultiSignature, DefaultExtra<T>>;

#[tokio::test]
async fn watcher() -> Result<()> {
    let client = ClientBuilder::<DefaultNodeRuntime>::new()
        .set_url("wss://rpc.polkadot.io")
        .build()
        .await?;

    let mut subscription = client.subscribe_finalized_blocks().await?;

    loop {
        let header = subscription.next().await;
        let block = client
            .block(Some(header.hash()))
            .await
            .map_err(|err| anyhow!("failed to fetch from remote PRC: {:?}", err))?
            .ok_or(anyhow!("No block available from remote PRC"))?
            .block;

        for extrinsic in &block.extrinsics {}

        time::sleep(Duration::from_secs(3)).await;
    }
}
