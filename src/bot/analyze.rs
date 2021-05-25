use crate::coincheck::model::*;
use crate::error::Error::TooShort;
use crate::Config;

use std::error::Error;

use colored::Colorize;
use log::debug;

// #[derive(Debug)]
// pub struct TradeInfoBuilder {
//     pub pair: Option<Pair>,
//     pub sell_rate: Option<f64>,
//     pub buy_rate: Option<f64>,
//     pub balance_key: Option<Balance>,
//     pub balance_settlement: Option<Balance>,
//     pub open_orders: Option<Box<Vec<OpenOrder>>>,
//     pub rate_histories: Option<Vec<f64>>,
// }
//
// impl TradeInfoBuilder {
//     pub fn new() -> TradeInfoBuilder {
//         TradeInfoBuilder {
//             pair: None,
//             sell_rate: None,
//             buy_rate: None,
//             balance_key: None,
//             balance_settlement: None,
//             open_orders: None,
//             rate_histories: None,
//         }
//     }
//
//     pub fn build(&mut self) -> Result<TradeInfo, Box<dyn Error>> {
//         let open_orders = *self
//             .open_orders
//             .clone()
//             .ok_or("open_orders is none, it is required field")?;
//
//         Ok(TradeInfo {
//             pair: self
//                 .pair
//                 .clone()
//                 .ok_or("pair is none, it is required field")?,
//             sell_rate: self
//                 .sell_rate
//                 .ok_or("sell_rate is none, it is required field")?,
//             buy_rate: self
//                 .buy_rate
//                 .ok_or("buy_rate is none, it is required field")?,
//             balance_key: self
//                 .balance_key
//                 .clone()
//                 .ok_or("balance_key is none, it is required field")?,
//             balance_settlement: self
//                 .balance_settlement
//                 .clone()
//                 .ok_or("balance_settlement is none, it is required field")?,
//             open_orders: open_orders,
//             rate_histories: self
//                 .rate_histories
//                 .ok_or("rate_histories is none, it is required field")?,
//         })
//     }
// }

#[derive(Debug)]
pub struct TradeInfo {
    pub pair: Pair,
    pub sell_rate: f64,
    pub buy_rate: f64,
    pub balance_key: Balance,
    pub balance_settlement: Balance,
    pub open_orders: Vec<OpenOrder>,
    pub rate_histories: Vec<f64>,
}

impl TradeInfo {
    pub fn calc_total_balance_jpy(&self) -> f64 {
        self.balance_key.total() * self.sell_rate + self.balance_settlement.total()
    }

    pub fn has_position(&self) -> bool {
        self.balance_key.total() * self.sell_rate >= 1.0
    }

    pub fn support_lines(&self, period: usize, offset: usize) -> Result<Vec<f64>, Box<dyn Error>> {
        let history_size = self.rate_histories.len();
        if history_size < period + offset {
            return Err(Box::new(TooShort {
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
            for (i, rate) in self.rate_histories.iter().enumerate() {
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
            let (aa, bb) = line_fit(&x, &y);
            a = aa;
            b = bb;
            begin = false;
        }
        Ok(make_line(a, b, self.rate_histories.len()))
    }

    pub fn resistance_lines(
        &self,
        period: usize,
        offset: usize,
    ) -> Result<Vec<f64>, Box<dyn Error>> {
        let history_size = self.rate_histories.len();
        if history_size < period + offset {
            return Err(Box::new(TooShort {
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
            for (i, rate) in self.rate_histories.iter().enumerate() {
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
            let (aa, bb) = line_fit(&x, &y);
            a = aa;
            b = bb;
            begin = false;
        }
        Ok(make_line(a, b, self.rate_histories.len()))
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

#[derive(Debug)]
pub struct Signal {
    pub turned_on: bool,
    pub name: String,
    pub detail: String,
}

impl Signal {
    pub fn to_string(&self) -> String {
        format!(
            "{} {}({})",
            if self.turned_on {
                "OK".green()
            } else {
                "NG".red()
            },
            self.name,
            self.detail
        )
    }
}

#[derive(Debug)]
pub struct SignalChecker<'a> {
    pub config: &'a Config,
}

impl SignalChecker<'_> {
    pub fn check_resistance_line_breakout(&self, info: &TradeInfo) -> Signal {
        let mut signal = Signal {
            turned_on: false,
            name: "resistance line breakout".to_owned(),
            detail: "".to_owned(),
        };

        let result = info.resistance_lines(
            self.config.resistance_line_period,
            self.config.resistance_line_offset,
        );
        if let Err(err) = result {
            signal.detail = format!("error occured, {}", err);
            return signal;
        }

        // レジスタンスライン関連の情報
        let lines = result.unwrap();
        let slope = lines[1] - lines[0];

        let width_upper = info.sell_rate * self.config.resistance_line_width_ratio_upper;
        let width_lower = info.sell_rate * self.config.resistance_line_width_ratio_lower;

        let upper = lines.last().unwrap() + width_upper;
        let lower = lines.last().unwrap() + width_lower;

        // レジスタンスラインの傾きチェック
        if slope < 0.0 {
            signal.detail = format!("slope:{:.3}", slope);
            return signal;
        }

        // レジスタンスラインのすぐ上でリバウンドしたかチェック
        if !info.is_upper_rebound(
            &lines,
            width_upper,
            width_lower,
            self.config.rebound_check_period,
        ) {
            signal.detail = "not roll reversal".to_owned();
            return signal;
        }

        // 現レートがレジスタンスライン近くかをチェック
        if info.sell_rate < lower || info.sell_rate > upper {
            signal.detail = format!(
                "sell rate:{:.3} is out of range:{:.3}...{:.3}",
                info.sell_rate, lower, upper,
            );
            return signal;
        }

        // レート上昇中かチェック
        let before_rate = *info.rate_histories.last().unwrap();
        if info.sell_rate <= before_rate {
            signal.detail = format!(
                "sell rate is not rising, sell rate:{:.3} <= before:{:.3}",
                info.sell_rate, before_rate,
            );
            return signal;
        }

        signal.turned_on = true;
        signal
    }

    // サポートラインがリバウンドしてるならエントリー
    fn check_support_line_rebound(&self, info: &TradeInfo) -> Signal {
        let mut signal = Signal {
            turned_on: false,
            name: "support line rebound".to_owned(),
            detail: "".to_owned(),
        };

        let result = info.support_lines(
            self.config.support_line_period_long,
            self.config.support_line_offset,
        );
        if let Err(err) = result {
            signal.detail = format!("error occured, {}", err);
            return signal;
        }

        // サポートライン関連の情報
        let lines = result.unwrap();
        let _slope = lines[1] - lines[0];
        let width_upper = info.sell_rate * self.config.support_line_width_ratio_upper;
        let width_lower = info.sell_rate * self.config.support_line_width_ratio_lower;
        let upper = lines.last().unwrap() + width_upper;
        let lower = lines.last().unwrap() - width_lower;

        // サポートラインのすぐ上でリバウンドしたかチェック
        if !info.is_upper_rebound(
            &lines,
            width_upper,
            width_lower,
            self.config.rebound_check_period,
        ) {
            signal.detail = "is_upper_rebound: false".to_owned();
            return signal;
        }

        // 現レートがサポートライン近くかをチェック
        if info.sell_rate < lower || info.sell_rate > upper {
            signal.detail = format!(
                "sell rate:{:.3} is out of range:{:.3}...{:.3}",
                info.sell_rate, lower, upper,
            );
            return signal;
        }

        signal.turned_on = true;
        signal
    }
}
