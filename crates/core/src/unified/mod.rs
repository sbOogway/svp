//! Unified instruments: one per coin and market (`BTC-PERP.SVP`,
//! `BTC-SPOT.SVP`) carrying the trades and the L2 book of every venue, merged.
//!
//! The [`Unifier`] publishes straight on the unified instrument's msgbus
//! topics, so subscribers get standard `TradeTick`s and `OrderBookDeltas`. It
//! can't go through the data engine, which is mid-borrow while it hands the
//! actor the venue data; venue `SVP` has a data client that does nothing.

mod actor;
mod book;
mod client;

pub use actor::Unifier;
pub use book::MergedBook;
pub use client::{SvpDataClientConfig, SvpDataClientFactory};
use nautilus_model::{
    data::TradeTick,
    identifiers::{InstrumentId, Symbol, TradeId},
    instruments::{CryptoPerpetual, CurrencyPair, Instrument, InstrumentAny},
    types::{Currency, Price, Quantity, fixed::FIXED_PRECISION},
};

use crate::venue::{Market, Subscription};

pub const VENUE: &str = "SVP";

#[derive(Debug, Clone)]
pub struct Unified {
    pub instrument_id: InstrumentId,
    pub market: Market,
    pub members: Vec<Subscription>,
}

/// Groups venue subscriptions by coin and market, perps and spot apart.
pub fn unify(subscriptions: &[Subscription]) -> Vec<Unified> {
    let mut unified: Vec<Unified> = Vec::new();
    for &sub in subscriptions {
        let instrument_id = instrument_id(sub);
        match unified
            .iter_mut()
            .find(|u| u.instrument_id == instrument_id)
        {
            Some(u) => u.members.push(sub),
            None => unified.push(Unified {
                instrument_id,
                market: sub.market,
                members: vec![sub],
            }),
        }
    }
    unified
}

fn instrument_id(sub: Subscription) -> InstrumentId {
    let market = match sub.market {
        Market::Spot => "SPOT",
        Market::Futures => "PERP",
    };
    InstrumentId::from(format!("{}-{market}.{VENUE}", sub.coin).as_str())
}

/// A venue size in coins. Some venues count derivatives in contracts (an OKX
/// or Coinbase BTC perp contract is 0.01 BTC); the instrument's multiplier is
/// the contract size, and 1 where sizes are already in coins.
pub fn size_in_coins(size: Quantity, instrument: &InstrumentAny) -> Quantity {
    let multiplier = instrument.multiplier();
    let coins = size * multiplier;
    Quantity::from_raw(coins.raw(), coin_precision(size.precision, multiplier))
}

/// `Quantity` multiplication keeps the larger precision, which is too coarse
/// here: 3 contracts of 0.01 BTC at precision 2 would print as 0.03, but
/// 0.03 contracts are 0.0003 BTC.
fn coin_precision(size_precision: u8, multiplier: Quantity) -> u8 {
    (size_precision + multiplier.precision).min(FIXED_PRECISION)
}

/// The unified instrument: the finest tick and size step of its venues, in
/// USD and coins.
pub fn build_instrument(
    instrument_id: InstrumentId,
    market: Market,
    members: &[InstrumentAny],
) -> anyhow::Result<InstrumentAny> {
    anyhow::ensure!(
        !members.is_empty(),
        "{instrument_id} has no venue instrument"
    );
    let price_precision = members
        .iter()
        .map(Instrument::price_precision)
        .max()
        .unwrap_or(0);
    let tick = members
        .iter()
        .map(Instrument::price_increment)
        .min()
        .expect("members is not empty");
    let size_increments: Vec<_> = members
        .iter()
        .map(|m| size_in_coins(m.size_increment(), m))
        .collect();
    let size_precision = size_increments
        .iter()
        .map(|q| q.precision)
        .max()
        .unwrap_or(0);
    let size_step = size_increments.iter().min().expect("members is not empty");

    let price_increment = Price::from_raw(tick.raw(), price_precision);
    let size_increment = Quantity::from_raw(size_step.raw(), size_precision);
    let raw_symbol = Symbol::new(instrument_id.symbol.as_str());
    let base_currency = members[0]
        .base_currency()
        .ok_or_else(|| anyhow::anyhow!("{} has no base currency", members[0].id()))?;
    let usd = Currency::USD();
    let instrument = match market {
        Market::Futures => InstrumentAny::CryptoPerpetual(
            CryptoPerpetual::builder()
                .instrument_id(instrument_id)
                .raw_symbol(raw_symbol)
                .base_currency(base_currency)
                .quote_currency(usd)
                .settlement_currency(usd)
                .is_inverse(false)
                .price_precision(price_precision)
                .size_precision(size_precision)
                .price_increment(price_increment)
                .size_increment(size_increment)
                .ts_event(0.into())
                .ts_init(0.into())
                .build()?,
        ),
        Market::Spot => InstrumentAny::CurrencyPair(
            CurrencyPair::builder()
                .instrument_id(instrument_id)
                .raw_symbol(raw_symbol)
                .base_currency(base_currency)
                .quote_currency(usd)
                .price_precision(price_precision)
                .size_precision(size_precision)
                .price_increment(price_increment)
                .size_increment(size_increment)
                .ts_event(0.into())
                .ts_init(0.into())
                .build()?,
        ),
    };
    Ok(instrument)
}

/// A venue trade as a trade of the unified instrument: size in coins, the
/// venue's price, side and timestamps, and an ID unique across venues.
pub fn unify_trade(
    trade: &TradeTick,
    venue_instrument: &InstrumentAny,
    unified: &InstrumentAny,
) -> TradeTick {
    TradeTick::new(
        unified.id(),
        with_price_precision(trade.price, unified.price_precision()),
        with_size_precision(
            size_in_coins(trade.size, venue_instrument),
            unified.size_precision(),
        ),
        trade.aggressor_side,
        trade_id(trade.instrument_id.venue.as_str(), trade.trade_id.as_str()),
        trade.ts_event,
        trade.ts_init,
    )
}

// A venue that joins after the unified instrument was built can quote finer
// than it: only then is a value rounded.
fn with_price_precision(price: Price, precision: u8) -> Price {
    if price.precision <= precision {
        Price::from_raw(price.raw(), precision)
    } else {
        Price::new(price.as_f64(), precision)
    }
}

fn with_size_precision(size: Quantity, precision: u8) -> Quantity {
    if size.precision <= precision {
        Quantity::from_raw(size.raw(), precision)
    } else {
        Quantity::new(size.as_f64(), precision)
    }
}

/// `VENUE-id`, within the 36 characters a `TradeId` holds. Coinbase and
/// Kraken use UUIDs, which only fit without their dashes and with their
/// leading digits cut; the rest is still unique.
fn trade_id(venue: &str, id: &str) -> TradeId {
    const MAX_LEN: usize = 36;
    let room = MAX_LEN - venue.len() - 1;
    let id = if id.len() > room {
        id.replace('-', "")
    } else {
        id.to_string()
    };
    let id = &id[id.len().saturating_sub(room)..];
    TradeId::new(format!("{venue}-{id}"))
}

#[cfg(test)]
mod tests {
    use nautilus_model::enums::AggressorSide;

    use super::*;
    use crate::venue::{Coin, FeedsBuilder, Venue, subscriptions};

    fn perp(id: &str, tick: &str, size_step: &str, multiplier: &str) -> InstrumentAny {
        let tick = Price::from(tick);
        let size_step = Quantity::from(size_step);
        InstrumentAny::CryptoPerpetual(
            CryptoPerpetual::builder()
                .instrument_id(InstrumentId::from(id))
                .raw_symbol(Symbol::from(id.split('.').next().unwrap()))
                .base_currency(Currency::BTC())
                .quote_currency(Currency::USDT())
                .settlement_currency(Currency::USDT())
                .is_inverse(false)
                .price_precision(tick.precision)
                .size_precision(size_step.precision)
                .price_increment(tick)
                .size_increment(size_step)
                .multiplier(Quantity::from(multiplier))
                .ts_event(0.into())
                .ts_init(0.into())
                .build()
                .unwrap(),
        )
    }

    #[test]
    fn groups_perps_and_spot_apart() {
        let feeds = FeedsBuilder::new()
            .add_venue(Venue::Binance)
            .add_venue(Venue::Hyperliquid)
            .add_market(Market::Spot)
            .add_market(Market::Futures)
            .add_instrument(Coin::BTC)
            .add_instrument(Coin::ETH)
            .build()
            .unwrap();
        let unified = unify(&subscriptions(&feeds));
        let summary: Vec<_> = unified
            .iter()
            .map(|u| (u.instrument_id.to_string(), u.members.len()))
            .collect();
        assert_eq!(
            summary,
            [
                ("BTC-SPOT.SVP".to_string(), 1),
                ("ETH-SPOT.SVP".to_string(), 1),
                ("BTC-PERP.SVP".to_string(), 2),
                ("ETH-PERP.SVP".to_string(), 2),
            ]
        );
    }

    #[test]
    fn takes_the_finest_tick_and_size_step() {
        let members = [
            perp("BTCUSDT-PERP.BINANCE", "0.1", "0.001", "1"),
            perp("PF_XBTUSD.KRAKEN", "1", "0.0001", "1"),
            perp("BTC-USDT-SWAP.OKX", "0.1", "0.01", "0.01"),
        ];
        let unified = build_instrument(
            InstrumentId::from("BTC-PERP.SVP"),
            Market::Futures,
            &members,
        )
        .unwrap();
        assert_eq!(unified.price_increment(), Price::from("0.1"));
        assert_eq!(unified.size_increment(), Quantity::from("0.0001"));
        assert_eq!(unified.base_currency(), Some(Currency::BTC()));
        assert_eq!(unified.quote_currency(), Currency::USD());
        assert_eq!(unified.multiplier(), Quantity::from(1));
    }

    #[test]
    fn keeps_the_finest_precision_when_ticks_do_not_nest() {
        let members = [
            perp("A-PERP.X", "0.1", "1", "1"),
            perp("B-PERP.Y", "0.25", "1", "1"),
        ];
        let unified = build_instrument(
            InstrumentId::from("BTC-PERP.SVP"),
            Market::Futures,
            &members,
        )
        .unwrap();
        assert_eq!(unified.price_increment(), Price::from("0.10"));
        assert_eq!(unified.price_precision(), 2);
    }

    #[test]
    fn converts_contracts_to_coins() {
        let okx = perp("BTC-USDT-SWAP.OKX", "0.1", "0.01", "0.01");
        let size = size_in_coins(Quantity::from("3.00"), &okx);
        assert_eq!(size, Quantity::from("0.0300"));
        assert_eq!(size.precision, 4);
    }

    #[test]
    fn keeps_sizes_already_in_coins() {
        let binance = perp("BTCUSDT-PERP.BINANCE", "0.1", "0.001", "1");
        assert_eq!(
            size_in_coins(Quantity::from("0.25"), &binance),
            Quantity::from("0.25")
        );
    }

    #[test]
    fn unifies_a_trade() {
        let okx = perp("BTC-USDT-SWAP.OKX", "0.1", "0.01", "0.01");
        let unified = build_instrument(
            InstrumentId::from("BTC-PERP.SVP"),
            Market::Futures,
            &[okx.clone(), perp("PF_XBTUSD.KRAKEN", "0.5", "0.0001", "1")],
        )
        .unwrap();
        let trade = TradeTick::new(
            okx.id(),
            Price::from("84000.1"),
            Quantity::from("3.00"),
            AggressorSide::Sell,
            TradeId::from("123"),
            7.into(),
            8.into(),
        );
        let out = unify_trade(&trade, &okx, &unified);
        assert_eq!(out.instrument_id, InstrumentId::from("BTC-PERP.SVP"));
        assert_eq!(out.price, Price::from("84000.1"));
        assert_eq!(out.size, Quantity::from("0.0300"));
        assert_eq!(out.aggressor_side, AggressorSide::Sell);
        assert_eq!(out.trade_id, TradeId::from("OKX-123"));
        assert_eq!((out.ts_event, out.ts_init), (7.into(), 8.into()));
    }

    #[test]
    fn trade_ids_fit_even_from_uuids() {
        let uuid = "0f8c5a3e-6b1d-4c2a-9e7f-1234567890ab";
        let id = trade_id("COINBASE", uuid);
        assert_eq!(id.as_str(), "COINBASE-a3e6b1d4c2a9e7f1234567890ab");
        assert_eq!(trade_id("BINANCE", "42").as_str(), "BINANCE-42");
    }
}
