use crate::coincheck::model::Balance;
use crate::coincheck::model::OpenOrder;
use crate::coincheck::model::Pair;
use crate::error::Error::TooShort;
use log::debug;
use std::error::Error;

#[derive(Debug)]
pub struct Analyzer {
    pub pair: Pair,
    pub sell_rate: f64,
    pub buy_rate: f64,
    pub balance_key: Balance,
    pub balance_settlement: Balance,
    pub open_orders: Vec<OpenOrder>,
    pub rate_histories: Vec<f64>,
}

impl Analyzer {
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
        loop {
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
        loop {
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

    pub fn is_upper_rebound(&self, lines: Vec<f64>, width: f64, period: usize) -> bool {
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
            if rate1 < line1 || rate2 < line2 || rate3 < line3 {
                return false;
            }

            // rate1,rate2,rate3 が v字 になってないならスキップ
            if !(rate1 >= rate2 && rate2 < rate3) {
                continue;
            }

            // v字の底がラインから離れすぎていたらスキップ
            if rate2 > line2 + width {
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
