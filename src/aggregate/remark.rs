use super::Aggregate;
use crate::event::{Event, OnChainRemark, RemarkFound};
use crate::manager::NetworkAddress;
use crate::Result;

#[derive(Eq, PartialEq, Hash, Clone, Debug)]
pub struct RemarkWatcherId;

#[derive(Debug, Clone)]
pub enum RemarkWatcherCommand {
    AddRemark {
        net_address: NetworkAddress,
        remark: OnChainRemark,
    },
}

#[derive(Debug, Clone)]
pub struct RemarkWatcher;

#[async_trait]
impl Aggregate for RemarkWatcher {
    type Id = RemarkWatcherId;
    type State = ();
    type Event = Event;
    type Command = RemarkWatcherCommand;
    type Error = anyhow::Error;

    #[cfg(test)]
    fn wipe(&mut self) {}

    fn state(&self) -> &Self::State {
        unimplemented!()
    }

    async fn apply(&mut self, _event: Self::Event) -> Result<()> {
        // No state is managed for remarks.
        Ok(())
    }

    async fn handle(&self, command: Self::Command) -> Result<Option<Vec<Self::Event>>> {
        match command {
            RemarkWatcherCommand::AddRemark {
                net_address,
                remark,
            } => Ok(Some(vec![Event::from(RemarkFound {
                net_address: net_address,
                remark: remark,
            })])),
        }
    }
}
