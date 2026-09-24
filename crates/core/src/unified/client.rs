use std::{any::Any, cell::RefCell, rc::Rc};

use async_trait::async_trait;

use nautilus_common::{
    cache::CacheView,
    clients::DataClient,
    clock::Clock,
    factories::{ClientConfig, DataClientFactory},
    messages::data::{
        SubscribeBookDeltas, SubscribeInstrument, SubscribeTrades, UnsubscribeBookDeltas,
        UnsubscribeInstrument, UnsubscribeTrades,
    },
};
use nautilus_model::identifiers::{ClientId, Venue};

use super::VENUE;

/// The data client of venue `SVP`. The [`Unifier`](super::Unifier) publishes
/// the data itself; this client only exists so the data engine accepts
/// subscriptions to unified instruments instead of logging that no client
/// serves the venue.
#[derive(Debug)]
pub struct SvpDataClient {
    client_id: ClientId,
    is_connected: bool,
}

#[async_trait(?Send)]
impl DataClient for SvpDataClient {
    fn client_id(&self) -> ClientId {
        self.client_id
    }

    fn venue(&self) -> Option<Venue> {
        Some(Venue::new(VENUE))
    }

    fn start(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn stop(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn reset(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn dispose(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    // The node waits on these before stopping.
    async fn connect(&mut self) -> anyhow::Result<()> {
        self.is_connected = true;
        Ok(())
    }

    async fn disconnect(&mut self) -> anyhow::Result<()> {
        self.is_connected = false;
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.is_connected
    }

    fn is_disconnected(&self) -> bool {
        !self.is_connected
    }

    fn subscribe_instrument(&mut self, _cmd: SubscribeInstrument) -> anyhow::Result<()> {
        Ok(())
    }

    fn subscribe_book_deltas(&mut self, _cmd: SubscribeBookDeltas) -> anyhow::Result<()> {
        Ok(())
    }

    fn subscribe_trades(&mut self, _cmd: SubscribeTrades) -> anyhow::Result<()> {
        Ok(())
    }

    fn unsubscribe_instrument(&mut self, _cmd: &UnsubscribeInstrument) -> anyhow::Result<()> {
        Ok(())
    }

    fn unsubscribe_book_deltas(&mut self, _cmd: &UnsubscribeBookDeltas) -> anyhow::Result<()> {
        Ok(())
    }

    fn unsubscribe_trades(&mut self, _cmd: &UnsubscribeTrades) -> anyhow::Result<()> {
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct SvpDataClientConfig;

impl ClientConfig for SvpDataClientConfig {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Debug, Default)]
pub struct SvpDataClientFactory;

impl DataClientFactory for SvpDataClientFactory {
    fn create(
        &self,
        name: &str,
        _config: &dyn ClientConfig,
        _cache: CacheView,
        _clock: Rc<RefCell<dyn Clock>>,
    ) -> anyhow::Result<Box<dyn DataClient>> {
        Ok(Box::new(SvpDataClient {
            client_id: ClientId::new(name),
            is_connected: false,
        }))
    }

    fn name(&self) -> &'static str {
        VENUE
    }

    fn config_type(&self) -> &'static str {
        "SvpDataClientConfig"
    }
}
