use crate::coincheck::model::Pair;

#[derive(Debug)]
pub struct EntryParam {
    pub pair: Pair,
    pub amount: f64,
    pub profit_ratio: f64,
}

#[derive(Debug)]
pub struct LossCutParam {
    pub pair: Pair,
    pub open_order_id: u64,
    pub amount: f64,
}

#[derive(Debug)]
pub struct SellParam {
    pub open_order_ids: Vec<u64>,
    pub pair: Pair,
    pub rate: f64,
    pub amount: f64,
}

pub enum ActionType {
    Entry(EntryParam),
    LossCut(LossCutParam),
    Sell(SellParam),
}
