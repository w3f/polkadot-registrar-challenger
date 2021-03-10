use crate::Result;
use parity_scale_codec::{Decode, Encode};
use sp_runtime::{generic::UncheckedExtrinsic, MultiSignature, MultiSigner};
use substrate_subxt::system::{System, SystemEventsDecoder};
use substrate_subxt::{
    extrinsic::DefaultExtra, ClientBuilder, DefaultNodeRuntime, EventSubscription, EventsDecoder,
};
use tokio::time::{self, Duration};

type Extrinsic<T: System> = UncheckedExtrinsic<MultiSigner, Call, MultiSignature, DefaultExtra<T>>;

#[tokio::test]
async fn watcher() -> Result<()> {
    let client = ClientBuilder::<DefaultNodeRuntime>::new()
        .set_url("wss://rpc.polkadot.io")
        .build()
        .await?;

    let mut subscription = client.subscribe_finalized_blocks().await?;

    loop {
        let header = subscription.next().await;
        let mut block = client
            .block(Some(header.hash()))
            .await
            .map_err(|err| anyhow!("failed to fetch from remote PRC: {:?}", err))?
            .ok_or(anyhow!("No block available from remote PRC"))?
            .block;

        for extrinsic in &mut block.extrinsics {
            if let Ok(system_call) = <Extrinsic<DefaultNodeRuntime> as Decode>::decode(
                &mut extrinsic.encode().as_slice(),
            ) {
                match system_call.function {
                    Call::Remark(remark) => {
                        if let Ok(value) = String::from_utf8(remark) {
                            info!("Found system remark for registrar: {}", "");
                        }
                    }
                    _ => {}
                }
            }
        }

        time::sleep(Duration::from_secs(3)).await;
    }
}

#[derive(Debug, PartialEq, Encode, Decode)]
enum Call {
    #[codec(index = 1)]
    Remark(Vec<u8>),
}
