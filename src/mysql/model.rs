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
}

impl MarketsMethods for Markets {
    fn rate_histories(&self) -> Vec<f64> {
        self.iter().map(|m| m.ex_rate_sell).collect()
    }
}

#[derive(Debug)]
pub struct BotStatus {
    pub bot_name: String,
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
