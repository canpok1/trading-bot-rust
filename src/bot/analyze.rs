use crate::coincheck::model::*;
use crate::error::MyError::KeyNotFound;
use crate::error::MyError::TooShort;
use crate::error::MyResult;
use crate::mysql::model::MarketSummary;

use log::debug;
use std::collections::HashMap;

#[derive(Debug)]
pub struct TradeInfo {
    pub pair: Pair,
    pub balances: HashMap<String, Balance>, // (k,v)=(coin,balance)
    pub sell_rates: HashMap<String, f64>,   // (k,v)=(pair,rate)
    pub buy_rate: f64,
    pub open_orders: Vec<OpenOrder>,
    pub rate_histories: Vec<f64>,
    pub sell_volumes: Vec<f64>,
    pub buy_volumes: Vec<f64>,
    pub support_lines_long: Vec<f64>,
    pub support_lines_short: Vec<f64>,
    pub resistance_lines: Vec<f64>,
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
    pub rate_histories: Vec<f64>,
    pub sell_volumes: Vec<f64>,
    pub buy_volumes: Vec<f64>,
    pub support_lines_long: Vec<f64>,
    pub support_lines_short: Vec<f64>,
    pub resistance_lines: Vec<f64>,
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
            rate_histories: self.rate_histories.clone(),
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
    ) -> MyResult<Vec<f64>> {
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
    ) -> MyResult<Vec<f64>> {
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

    pub fn is_upper_rebound(
        &self,
        lines: &Vec<f64>,
        width_upper: f64,
        width_lower: f64,
        period: usize,
    ) -> bool {
        let history_size = self.rate_histories.len();
        if history_size < period {
            return false;
        }

        let end_idx = history_size - 1;
        let begin_idx = end_idx - period;
        for idx in (begin_idx..=end_idx).rev() {
            let rate1 = self.rate_histories.iter().nth(idx - 2);
            let rate2 = self.rate_histories.iter().nth(idx - 1);
            let rate3 = self.rate_histories.iter().nth(idx);
            if rate1.is_none() || rate2.is_none() || rate3.is_none() {
                return false;
            }
            let line1 = lines.iter().nth(idx - 2);
            let line2 = lines.iter().nth(idx - 1);
            let line3 = lines.iter().nth(idx);
            if line1.is_none() || line2.is_none() || line3.is_none() {
                return false;
            }

            let rate1 = *rate1.unwrap();
            let rate2 = *rate2.unwrap();
            let rate3 = *rate3.unwrap();
            let line1 = *line1.unwrap();
            let line2 = *line2.unwrap();
            let line3 = *line3.unwrap();

            // rate1,rate2,rate3 のいずれかがラインを下回ったらチェック打ち切り
            if rate1 < (line1 - width_lower)
                || rate2 < (line2 - width_lower)
                || rate3 < (line3 - width_lower)
            {
                return false;
            }

            // rate1,rate2,rate3 が v字 になってないならスキップ
            if !(rate1 >= rate2 && rate2 < rate3) {
                continue;
            }

            // v字の底がラインから離れすぎていたらスキップ
            if rate2 > line2 + width_upper {
                continue;
            }

            debug!(
                "rebounded (rate: {:.3} -> {:.3} -> {:.3}) (line: {:.3} -> {:.3} -> {:.3})",
                rate1, rate2, rate3, line1, line2, line3,
            );
            return true;
        }
        false
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

    fn make_line(a: f64, b: f64, size: usize) -> Vec<f64> {
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

    pub fn is_rate_rising(&self) -> Option<bool> {
        if let Some(before_rate) = self.rate_histories.last() {
            let sell_rate = self.get_sell_rate();
            if let Ok(sell_rate) = sell_rate {
                return Some(sell_rate <= *before_rate);
            }
        }
        None
    }

    pub fn wma(&self, period: usize) -> MyResult<f64> {
        if self.rate_histories.len() < period {
            Err(Box::new(TooShort {
                name: "rate histories".to_owned(),
                len: self.rate_histories.len(),
                required: period,
            }))
        } else {
            let mut sum: f64 = 0.0;
            let mut weight_sum: f64 = 0.0;
            let begin = self.rate_histories.len() - period;
            for (i, r) in self.rate_histories[begin..].iter().enumerate() {
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
