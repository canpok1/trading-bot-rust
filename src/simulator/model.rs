use crate::error::MyResult;
use crate::mysql::model::Market;
use chrono::NaiveDateTime;
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct CSVRecord {
    pub id: u64,
    pub pair: String,
    pub store_rate_avg: f64,
    pub ex_rate_sell: f64,
    pub ex_rate_buy: f64,
    pub ex_volume_sell: f64,
    pub ex_volume_buy: f64,
    pub recorded_at: String,
    pub created_at: String,
    pub updated_at: String,
}

impl CSVRecord {
    pub fn to_model(&self) -> MyResult<Market> {
        Ok(Market {
            pair: self.pair.clone(),
            store_rate_avg: self.store_rate_avg,
            ex_rate_sell: self.ex_rate_sell,
            ex_rate_buy: self.ex_rate_buy,
            ex_volume_sell: self.ex_volume_sell,
            ex_volume_buy: self.ex_volume_buy,
            recorded_at: NaiveDateTime::parse_from_str(&self.recorded_at, "%Y-%m-%d %H:%M:%S")?,
        })
    }
}
