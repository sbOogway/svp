use nautilus_common::enums::Environment;
use nautilus_live::node::LiveNode;
use nautilus_model::identifiers::TraderId;

use crate::{
    actor::TradeLogger,
    venue::{self, DataClientSpec, Feed},
};

/// Building the node initializes Nautilus logging, which registers the global
/// `log` logger: the caller must not have registered another one.
pub fn build(feeds: &[Feed]) -> anyhow::Result<LiveNode> {
    let mut builder = LiveNode::builder(TraderId::from("SVP-001"), Environment::Live)?
        .with_name("svp")
        .with_delay_post_stop_secs(1);

    for feed in feeds {
        let DataClientSpec { factory, config } = feed.data_client();
        builder = builder.add_data_client(Some(feed.client_id().to_string()), factory, config)?;
    }

    let mut node = builder.build()?;
    node.add_actor(TradeLogger::new(venue::subscriptions(feeds)))?;
    Ok(node)
}
