//! `useMarketRatioFinder` from `TradeBulk.vue`: the "Market Ratio" row the exchange
//! table merges into its offers.
//!
//! poe.ninja prices the item in chaos; the row restates that price in the table's two
//! currencies: the cheapest offer of one item for `N` chaos, or of `N` items for one
//! divine. `mergeWithMarketRatio` then puts the row where its price belongs.

use wraex_poe1::parser::item::Item;
use wraex_poe1::prices::Prices;
use wraex_poe1::results::BulkPricingResult;

/// `interface MarketRatio`: the row's two amounts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarketRatio {
    pub exchange_amount: f64,
    pub item_amount: f64,
}

impl MarketRatio {
    /// The row's sort key: `exchangeAmount / itemAmount`.
    pub fn price(&self) -> f64 {
        self.exchange_amount / self.item_amount
    }
}

/// `{ xchgChaos, xchgStable }`
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MarketRatioRow {
    pub xchg_chaos: Option<MarketRatio>,
    pub xchg_stable: Option<MarketRatio>,
}

/// `marketRatio[selectedCurr]`
pub fn for_currency(rows: MarketRatioRow, stable: bool) -> Option<MarketRatio> {
    if stable { rows.xchg_stable } else { rows.xchg_chaos }
}

/// `find(item, filters)`: what one unit is worth on the market, in chaos and in divines.
/// Both sides stay `None` when poe.ninja has no price for the item.
pub fn find(prices: &Prices, item: &Item) -> MarketRatioRow {
    let Some(price) = wraex_poe1::trends::get_details_id(item).and_then(|id| prices.find_price(&id.query())) else {
        return MarketRatioRow::default();
    };
    let rate = prices.xchg_rate.filter(|rate| *rate != 0.0);

    // Below one divine, the chaos side asks for a round number of chaos (`20` or `10`),
    // so the row reads like a trade search rather than a fraction of an item.
    let xchg_chaos = (price.info.chaos < rate.unwrap_or(9999.0) * 1.06).then(|| {
        if price.info.chaos > 4.0 {
            MarketRatio { exchange_amount: price.info.chaos, item_amount: 1.0 }
        } else {
            let exchange_amount = if price.info.chaos > 1.0 { 20.0 } else { 10.0 };
            MarketRatio { exchange_amount, item_amount: exchange_amount / price.info.chaos }
        }
    });

    let xchg_stable = rate.map(|rate| {
        if price.info.chaos > rate * 0.94 {
            MarketRatio { exchange_amount: price.info.chaos / rate, item_amount: 1.0 }
        } else {
            let exchange_amount = if price.info.chaos > rate * 0.2 { 2.0 } else { 1.0 };
            MarketRatio { exchange_amount, item_amount: exchange_amount * (rate / price.info.chaos) }
        }
    });

    MarketRatioRow { xchg_chaos, xchg_stable }
}

/// One table row: an offer, or the market ratio among them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Row {
    /// `results[index]`
    Offer(usize),
    Ratio(MarketRatio),
}

/// `mergeWithMarketRatio`: the first row whose price is at or above the market ratio's,
/// or the end of the table.
pub fn merge(results: &[BulkPricingResult], ratio: MarketRatio) -> Vec<Row> {
    if results.is_empty() {
        return vec![Row::Ratio(ratio)];
    }
    let mut out: Vec<Row> = (0..results.len()).map(Row::Offer).collect();
    let index = results
        .iter()
        .position(|result| result.exchange_amount / result.item_amount >= ratio.price())
        .unwrap_or(results.len());
    out.insert(index, Row::Ratio(ratio));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLOB: &str = concat!(
        r#"{"overviews":[{"type":"Currency","lines":[{"name":"Divine Orb","chaos":890.9,"graph":[]}"#,
        r#",{"name":"Exalted Orb","chaos":12,"graph":[]}"#,
        r#",{"name":"Orb of Alchemy","chaos":2,"graph":[]}]}]}"#
    );

    fn prices() -> Prices {
        let mut prices = Prices::default();
        prices.loaded(BLOB, 0);
        prices
    }

    fn item(slug: &str) -> Item {
        let data = crate::poe1::price_check::data();
        wraex_poe1::parser::parse_clipboard(data, &crate::corpus(slug)).expect("a parsed item")
    }

    /// The divine orb's row: 890.9 chaos, or one divine — the reference screenshot's row.
    #[test]
    fn a_divine_is_its_own_ratio() {
        let prices = prices();
        let rows = find(&prices, &item("currency-divine-orb"));
        let chaos = rows.xchg_chaos.expect("a chaos row");
        assert_eq!((chaos.exchange_amount, chaos.item_amount), (890.9, 1.0));
        let stable = rows.xchg_stable.expect("a divine row");
        assert_eq!((stable.exchange_amount, stable.item_amount), (1.0, 1.0));
    }

    /// A cheap item asks for a round number of items instead of a fraction of one.
    #[test]
    fn a_cheap_item_counts_items() {
        let mut item = item("currency-divine-orb");
        item.info.as_mut().unwrap().ref_name = "Orb of Alchemy".to_owned();
        let rows = find(&prices(), &item);
        let chaos = rows.xchg_chaos.expect("a chaos row");
        // 2 chaos each: 20 chaos buys ten of them.
        assert_eq!((chaos.exchange_amount, chaos.item_amount), (20.0, 10.0));
        let stable = rows.xchg_stable.expect("a divine row");
        // One divine is worth 890.9 / 2 of them, so the row asks for one divine.
        assert_eq!(stable.exchange_amount, 1.0);
        assert!((stable.item_amount - 890.9 / 2.0).abs() < 1e-9);
    }

    /// An item priced above four chaos asks for itself, not for a round number.
    #[test]
    fn an_expensive_item_asks_for_one() {
        let mut item = item("currency-divine-orb");
        item.info.as_mut().unwrap().ref_name = "Exalted Orb".to_owned();
        let chaos = find(&prices(), &item).xchg_chaos.expect("a chaos row");
        assert_eq!((chaos.exchange_amount, chaos.item_amount), (12.0, 1.0));
    }

    /// An item poe.ninja does not price has no row at all.
    #[test]
    fn an_unknown_item_has_no_ratio() {
        let mut item = item("currency-divine-orb");
        item.info.as_mut().unwrap().ref_name = "Not A Real Orb".to_owned();
        assert_eq!(find(&prices(), &item), MarketRatioRow::default());
    }
}
