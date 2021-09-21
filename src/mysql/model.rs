use crate::coincheck::model::Pair;
use chrono::Utc;

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

#[derive(Debug, Clone)]
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

impl Default for MarketSummary {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            count: 0,
            recorded_at_begin: now.naive_utc(),
            recorded_at_end: now.naive_utc(),
            ex_rate_sell_max: 0.0,
            ex_rate_sell_min: 0.0,
            ex_rate_buy_max: 0.0,
            ex_rate_buy_min: 0.0,
            ex_volume_sell_total: 0.0,
            ex_volume_buy_total: 0.0,
            trade_frequency_ratio: 0.0,
        }
    }
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
