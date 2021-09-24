use crate::bot::model::{ActionType, TradeInfo};
use crate::error::MyResult;
use async_trait::async_trait;
use chrono::DateTime;
use chrono::Utc;

#[async_trait]
pub trait Strategy {
    async fn judge(
        &self,
        now: &DateTime<Utc>,
        info: &TradeInfo,
        buy_jpy_per_lot: f64,
    ) -> MyResult<Vec<ActionType>>;
}

#[derive(Debug)]
pub enum StrategyType {
    Scalping,
}
