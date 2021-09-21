use crate::bot::model::{ActionType, TradeInfo};
use crate::error::MyResult;
use chrono::DateTime;
use chrono::Utc;

pub trait Strategy {
    fn judge(
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
