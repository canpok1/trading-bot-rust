use crate::coincheck::model::Pair;
use crate::coincheck::model::*;
use crate::error::MyError::KeyNotFound;
use crate::error::MyError::TooShort;
use crate::error::MyResult;
use crate::mysql::model::MarketSummary;
use crate::slack::client::TextMessage;

use std::collections::HashMap;

#[derive(Debug, PartialEq)]
pub struct EntryParam {
    pub pair: Pair,
    pub amount: f64,
    pub profit_ratio: f64,
    pub offset_sell_rate_ratio: f64,
}

#[derive(Debug, PartialEq)]
pub struct LossCutParam {
    pub pair: Pair,
    pub open_order_id: u64,
    pub amount: f64,
}

#[derive(Debug, PartialEq)]
pub struct SellParam {
    pub open_order_ids: Vec<u64>,
    pub pair: Pair,
    pub rate: f64,
    pub amount: f64,
}

#[derive(Debug, PartialEq)]
pub struct AvgDownParam {
    pub pair: Pair,
    pub buy_jpy_per_lot: f64,
    pub market_buy_amount: f64,
    pub open_order_id: u64,
    pub open_order_rate: f64,
    pub open_order_amount: f64,
    pub offset_sell_rate_ratio: f64,
    pub memo: String,
}

#[derive(Debug, PartialEq)]
pub struct SetProfitParam {
    pub pair: Pair,
    pub open_order_id: u64,
    pub amount: f64,
}

#[derive(Debug, PartialEq)]
pub struct NotifyParam {
    pub log_message: String,
    pub slack_message: TextMessage,
}

#[derive(Debug, PartialEq)]
pub enum ActionType {
    Entry(EntryParam),
    LossCut(LossCutParam),
    Sell(SellParam),
    AvgDown(AvgDownParam),
    SetProfit(SetProfitParam),
    Notify(NotifyParam),
}

pub trait LineMethod {
    fn get_current(&self) -> Option<f64>;
}
impl LineMethod for Vec<f64> {
    fn get_current(&self) -> Option<f64> {
        if let Some(v) = self.last() {
            Some(*v)
        } else {
            None
        }
    }
}

pub type StraightLine = Vec<f64>;

#[derive(Debug)]
pub struct TradeInfo {
    pub pair: Pair,
    pub balances: HashMap<String, Balance>, // (k,v)=(coin,balance)
    pub sell_rates: HashMap<String, f64>,   // (k,v)=(pair,rate)
    pub buy_rate: f64,
    pub open_orders: Vec<OpenOrder>,
    pub sell_rate_histories: Vec<f64>,
    pub sell_volumes: Vec<f64>,
    pub buy_volumes: Vec<f64>,
    pub support_lines_long: StraightLine,
    pub support_lines_short: StraightLine,
    pub resistance_lines: StraightLine,
    pub order_books: OrderBooks,
    pub market_summary: MarketSummary,
}

#[derive(Debug, Default)]
pub struct TradeInfoParam {
    pub pair: Pair,
    pub balances: HashMap<String, Balance>, // (k,v)=(coin,balance)
    pub sell_rates: HashMap<String, f64>,   // (k,v)=(pair,rate)
    pub buy_rate: f64,
    pub open_orders: Vec<OpenOrder>,
    pub sell_rate_histories: Vec<f64>,
    pub sell_volumes: Vec<f64>,
    pub buy_volumes: Vec<f64>,
    pub support_lines_long: StraightLine,
    pub support_lines_short: StraightLine,
    pub resistance_lines: StraightLine,
    pub order_books: OrderBooks,
    pub market_summary: MarketSummary,
}

impl TradeInfoParam {
    pub fn build(&self) -> MyResult<TradeInfo> {
        Ok(TradeInfo {
            pair: self.pair.clone(),
            balances: self.balances.clone(),
            sell_rates: self.sell_rates.clone(),
            buy_rate: self.buy_rate,
            open_orders: self.open_orders.clone(),
            sell_rate_histories: self.sell_rate_histories.clone(),
            sell_volumes: self.sell_volumes.clone(),
            buy_volumes: self.buy_volumes.clone(),
            support_lines_long: self.support_lines_long.clone(),
            support_lines_short: self.support_lines_short.clone(),
            resistance_lines: self.resistance_lines.clone(),
            order_books: self.order_books.clone(),
            market_summary: self.market_summary.clone(),
        })
    }

    pub fn support_lines(
        rate_histories: &Vec<f64>,
        period: usize,
        offset: usize,
    ) -> MyResult<StraightLine> {
        let history_size = rate_histories.len();
        if history_size < period + offset {
            return Err(Box::new(TooShort {
                name: "rate histories".to_owned(),
                len: history_size,
                required: period + offset,
            }));
        }

        let begin_idx = history_size - period - offset;
        let end_idx = history_size - offset;

        let mut begin = true;
        let mut a: f64 = 0.0;
        let mut b: f64 = 0.0;
        for _ in 0..history_size {
            let mut x: Vec<f64> = Vec::new();
            let mut y: Vec<f64> = Vec::new();
            for (i, rate) in rate_histories.iter().enumerate() {
                if i < begin_idx || i > end_idx {
                    continue;
                }
                if begin || *rate <= a * (i as f64) + b {
                    x.push(i as f64);
                    y.push(*rate);
                }
            }
            if x.len() <= 3 {
                break;
            }
            let (aa, bb) = TradeInfoParam::line_fit(&x, &y);
            a = aa;
            b = bb;
            begin = false;
        }
        Ok(TradeInfoParam::make_line(a, b, rate_histories.len()))
    }

    pub fn resistance_lines(
        rate_histories: &Vec<f64>,
        period: usize,
        offset: usize,
    ) -> MyResult<StraightLine> {
        let history_size = rate_histories.len();
        if history_size < period + offset {
            return Err(Box::new(TooShort {
                name: "rate histories".to_owned(),
                len: history_size,
                required: period + offset,
            }));
        }

        let begin_idx = history_size - period - offset;
        let end_idx = history_size - offset;

        let mut begin = true;
        let mut a: f64 = 0.0;
        let mut b: f64 = 0.0;
        for _ in 0..history_size {
            let mut x: Vec<f64> = Vec::new();
            let mut y: Vec<f64> = Vec::new();
            for (i, rate) in rate_histories.iter().enumerate() {
                if i < begin_idx || i > end_idx {
                    continue;
                }
                if begin || *rate >= a * (i as f64) + b {
                    x.push(i as f64);
                    y.push(*rate);
                }
            }
            if x.len() <= 3 {
                break;
            }
            let (aa, bb) = TradeInfoParam::line_fit(&x, &y);
            a = aa;
            b = bb;
            begin = false;
        }
        Ok(TradeInfoParam::make_line(a, b, rate_histories.len()))
    }

    fn line_fit(x: &Vec<f64>, y: &Vec<f64>) -> (f64, f64) {
        let ndata = x.len();
        if ndata < 2 {
            return (0.0, 0.0);
        }

        let mut sx = 0.0;
        let mut sy = 0.0;
        for i in 0..ndata {
            sx += x[i];
            sy += y[i];
        }
        let mut st2 = 0.0;
        let mut a = 0.0;
        let sxoss = sx / (ndata as f64);
        for i in 0..ndata {
            let t = x[i] - sxoss;
            st2 += t * t;
            a += t * y[i];
        }
        a /= st2;

        let b = (sy - sx * a) / (ndata as f64);
        (a, b)
    }

    fn make_line(a: f64, b: f64, size: usize) -> StraightLine {
        (0..size).map(|i| a * (i as f64) + b).collect()
    }
}

impl TradeInfo {
    pub fn get_sell_rate(&self) -> MyResult<f64> {
        let key = self.pair.to_string();
        if let Some(rate) = self.sell_rates.get(&key) {
            Ok(*rate)
        } else {
            Err(Box::new(KeyNotFound {
                key: key,
                collection_name: "sell_rates".to_owned(),
            }))
        }
    }

    pub fn get_balance_key(&self) -> MyResult<&Balance> {
        let key = &self.pair.key;
        if let Some(b) = self.balances.get(key) {
            Ok(b)
        } else {
            Err(Box::new(KeyNotFound {
                key: key.to_owned(),
                collection_name: "balances".to_owned(),
            }))
        }
    }

    pub fn get_balance_settlement(&self) -> MyResult<&Balance> {
        let key = &self.pair.settlement;
        if let Some(b) = self.balances.get(key) {
            Ok(b)
        } else {
            Err(Box::new(KeyNotFound {
                key: key.to_owned(),
                collection_name: "balances".to_owned(),
            }))
        }
    }

    pub fn calc_total_balance_jpy(&self) -> f64 {
        let mut total = 0.0;
        for (k, balance) in self.balances.iter() {
            if *k == self.pair.settlement {
                total += balance.total();
            } else {
                let pair = format!("{}_{}", k, self.pair.settlement);
                let rate = self.sell_rates.get(&pair).unwrap();
                total += balance.total() * rate;
            }
        }
        total
    }

    pub fn has_position(&self) -> MyResult<bool> {
        Ok(self.get_balance_key()?.total() * self.get_sell_rate()? >= 1.0)
    }

    pub fn wma(&self, period: usize) -> MyResult<f64> {
        if self.sell_rate_histories.len() < period {
            Err(Box::new(TooShort {
                name: "rate histories".to_owned(),
                len: self.sell_rate_histories.len(),
                required: period,
            }))
        } else {
            let mut sum: f64 = 0.0;
            let mut weight_sum: f64 = 0.0;
            let begin = self.sell_rate_histories.len() - period;
            for (i, r) in self.sell_rate_histories[begin..].iter().enumerate() {
                let weight = (period - i) as f64;
                sum += r * weight;
                weight_sum += weight;
            }
            Ok(sum / weight_sum)
        }
    }

    pub fn is_down_trend(&self, short_period: usize, long_period: usize) -> MyResult<bool> {
        let wma_short = self.wma(short_period)?;
        let wma_long = self.wma(long_period)?;
        Ok(wma_short < wma_long)
    }

    pub fn is_up_trend(&self, short_period: usize, long_period: usize) -> MyResult<bool> {
        let wma_short = self.wma(short_period)?;
        let wma_long = self.wma(long_period)?;
        Ok(wma_short > wma_long)
    }
}
