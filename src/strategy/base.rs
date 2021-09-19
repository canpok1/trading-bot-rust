use crate::bot::analyze::TradeInfo;
use crate::bot::model::ActionType;
use crate::config::Config;
use crate::error::MyResult;
use crate::strategy::scalping::ScalpingStrategy;
use chrono::DateTime;
use chrono::Utc;

pub trait Strategy {
    fn judge(&self, now: &DateTime<Utc>, info: &TradeInfo) -> MyResult<Vec<ActionType>>;
}

#[derive(Debug)]
pub enum StrategyType {
    Scalping,
}

pub fn new<S>(t: StrategyType, config: &Config) -> MyResult<Box<dyn Strategy + '_>> {
    match t {
        StrategyType::Scalping => Ok(Box::new(ScalpingStrategy { config: config })),
    }
}
