//! Wire protocol shared between the svp server and app.

use serde::{Deserialize, Serialize};

/// Identifies a bar stream as `venue:symbol:timeframe`, e.g. `BINANCE:BTCUSDT-PERP:1m`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StreamId(pub String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_id_is_transparent_string() {
        let id = StreamId("BINANCE:BTCUSDT-PERP:1m".into());
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"BINANCE:BTCUSDT-PERP:1m\"");
    }
}
