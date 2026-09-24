use std::fmt;

use nautilus_model::{
    data::QuoteTick,
    identifiers::{ClientId, InstrumentId},
    types::{Price, fixed::FIXED_PRECISION, price::PriceRaw},
};

use crate::aggregator::venue::{self, Market, Venue};

const SCALE: PriceRaw = PriceRaw::pow(10, FIXED_PRECISION as u32);

/// How many USD one unit of a quote currency is worth, in fixed point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsdRate(PriceRaw);

impl UsdRate {
    pub const ONE: Self = Self(SCALE);

    /// The mid of a quote against USD.
    pub fn from_quote(quote: &QuoteTick) -> Self {
        Self(PriceRaw::midpoint(
            quote.bid_price.raw(),
            quote.ask_price.raw(),
        ))
    }

    /// Rounds to the nearest raw unit; prices are positive. Whole and
    /// fractional units are multiplied apart: `raw * rate` in one go
    /// overflows for prices far out in a book.
    pub fn convert(self, raw: PriceRaw) -> PriceRaw {
        let (whole, fraction) = (raw / SCALE, raw % SCALE);
        whole * self.0 + (fraction * self.0 + SCALE / 2) / SCALE
    }
}

impl fmt::Display for UsdRate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            Price::from_raw(self.0, FIXED_PRECISION)
                .as_decimal()
                .normalize()
        )
    }
}

/// Where the USD rate of a stablecoin comes from: Kraken spot, which lists
/// both against USD.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateSource {
    pub currency: &'static str,
    pub instrument_id: InstrumentId,
    pub client_id: ClientId,
}

pub fn rate_sources() -> Vec<RateSource> {
    let client_id = venue::client_id(Venue::Kraken, Market::Spot);
    [("USDT", "USDT/USD.KRAKEN"), ("USDC", "USDC/USD.KRAKEN")]
        .into_iter()
        .map(|(currency, id)| RateSource {
            currency,
            instrument_id: InstrumentId::from(id),
            client_id,
        })
        .collect()
}

/// A price converted to USD and rounded to `precision`.
pub fn usd_price(price: Price, rate: UsdRate, precision: u8) -> Price {
    let raw = rate.convert(price.raw());
    let unit = PriceRaw::pow(10, u32::from(FIXED_PRECISION - precision));
    let rounded = (raw + unit / 2).div_euclid(unit) * unit;
    Price::from_raw(rounded, precision)
}

#[cfg(test)]
mod tests {
    use nautilus_model::types::Quantity;

    use super::*;

    fn quote(bid: &str, ask: &str) -> QuoteTick {
        QuoteTick::new(
            InstrumentId::from("USDT/USD.KRAKEN"),
            Price::from(bid),
            Price::from(ask),
            Quantity::from(1),
            Quantity::from(1),
            0.into(),
            0.into(),
        )
    }

    #[test]
    fn converts_at_the_mid() {
        let rate = UsdRate::from_quote(&quote("0.99966", "0.99968"));
        assert_eq!(
            usd_price(Price::from("84000.0"), rate, 2),
            Price::from("83972.28")
        );
    }

    #[test]
    fn converts_prices_far_out_in_a_book() {
        let rate = UsdRate::from_quote(&quote("0.99966", "0.99968"));
        assert_eq!(
            usd_price(Price::from("10000000.0"), rate, 1),
            Price::from("9996700.0")
        );
    }

    #[test]
    fn usd_is_unchanged() {
        let price = Price::from("84000.1");
        assert_eq!(usd_price(price, UsdRate::ONE, 1), price);
    }
}
