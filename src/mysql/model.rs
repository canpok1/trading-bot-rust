use crate::coincheck::model::Pair;

#[derive(Debug)]
pub struct Market {
    pub pair: String,
    pub store_rate_avg: f64,
    pub ex_rate_sell: f64,
    pub ex_rate_buy: f64,
    pub ex_volume_sell: f64,
    pub ex_volume_buy: f64,
    pub recorded_at: chrono::NaiveDateTime,
}

pub type Markets = Vec<Market>;

pub trait MarketsMethods {
    fn rate_histories(&self) -> Vec<f64>;
    fn sell_volumes(&self) -> Vec<f64>;
    fn buy_volumes(&self) -> Vec<f64>;
}

impl MarketsMethods for Markets {
    fn rate_histories(&self) -> Vec<f64> {
        self.iter().map(|m| m.ex_rate_sell).collect()
    }

    fn sell_volumes(&self) -> Vec<f64> {
        self.iter().map(|m| m.ex_volume_sell).collect()
    }

    fn buy_volumes(&self) -> Vec<f64> {
        self.iter().map(|m| m.ex_volume_buy).collect()
    }
}

#[derive(Debug)]
pub struct MarketSummary {
    pub count: u64,
    pub recorded_at_begin: chrono::NaiveDateTime,
    pub recorded_at_end: chrono::NaiveDateTime,
    pub ex_rate_sell_max: f64,
    pub ex_rate_sell_min: f64,
    pub ex_rate_buy_max: f64,
    pub ex_rate_buy_min: f64,
    pub ex_volume_sell_total: f64,
    pub ex_volume_buy_total: f64,
    pub trade_frequency_ratio: f64,
}

#[derive(Debug)]
pub struct BotStatus {
    pub bot_name: String,
    pub pair: String,
    pub r#type: String,
    pub value: f64,
    pub memo: String,
}

#[derive(Debug)]
pub enum EventType {
    Sell,
    Buy,
}

#[derive(Debug)]
pub struct Event {
    pub pair: Pair,
    pub event_type: EventType,
    pub memo: String,
    pub recorded_at: chrono::NaiveDateTime,
}
