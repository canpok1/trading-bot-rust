use crate::bot::model::{ActionType, TradeInfo};
use crate::coincheck;
use crate::error::MyResult;
use async_trait::async_trait;
use chrono::DateTime;
use chrono::Utc;

#[async_trait]
pub trait Strategy {
    async fn judge<T>(
        &self,
        now: &DateTime<Utc>,
        info: &TradeInfo,
        buy_jpy_per_lot: f64,
        client: &T,
    ) -> MyResult<Vec<ActionType>>
    where
        T: coincheck::client::Client + std::marker::Sync;
}

#[derive(Debug)]
pub enum StrategyType {
    Scalping,
}
