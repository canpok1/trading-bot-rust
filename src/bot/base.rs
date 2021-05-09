use crate::coincheck;
use crate::coincheck::model::{Balance, OpenOrder, OrderType};
use crate::mysql;
use crate::mysql::model::{BotStatus, MarketsMethods};
use chrono::{Duration, Utc};
use log::{debug, info};
use std::error::Error;
use std::{thread, time};

#[derive(Debug)]
struct ExchangeInfo {
    pair: String,
    sell_rate: f64,
    buy_rate: f64,
    balance_key: Balance,
    balance_settlement: Balance,
    open_orders: Vec<OpenOrder>,
    rate_histories: Vec<f64>,
}

impl ExchangeInfo {
    fn support_lines(&self, begin_idx: usize, end_idx: usize) -> Vec<f64> {
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
            let (aa, bb) = ExchangeInfo::line_fit(&x, &y);
            a = aa;
            b = bb;
            begin = false;
        }
        ExchangeInfo::make_line(a, b, self.rate_histories.len())
    }

    fn resistance_lines(&self, begin_idx: usize, end_idx: usize) -> Vec<f64> {
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
            let (aa, bb) = ExchangeInfo::line_fit(&x, &y);
            a = aa;
            b = bb;
            begin = false;
        }
        ExchangeInfo::make_line(a, b, self.rate_histories.len())
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

#[derive(Debug)]
struct ActionParam {}

#[derive(Debug)]
pub struct Bot {
    pub config: crate::config::Config,
    pub coincheck_client: coincheck::client::Client,
    pub mysql_client: mysql::client::Client,
}

impl Bot {
    pub fn wait(&self) -> Result<(), Box<dyn Error>> {
        let d = time::Duration::from_secs(self.config.interval_sec);
        debug!("wait ... [{:?}]", d);
        thread::sleep(d);
        Ok(())
    }

    pub async fn trade(&self) -> Result<(), Box<dyn Error>> {
        let info = self.fetch().await?;
        info!("{:?}", info);

        self.upsert(&info)?;
        let params = self.make_params(&info)?;
        for param in params.iter() {
            self.action(param)?;
        }
        Ok(())
    }

    async fn fetch(&self) -> Result<ExchangeInfo, Box<dyn Error>> {
        let sell_rate = self
            .coincheck_client
            .get_exchange_orders_rate(OrderType::Sell, &self.config.target_pair)
            .await?;
        let buy_rate = self
            .coincheck_client
            .get_exchange_orders_rate(OrderType::Buy, &self.config.target_pair)
            .await?;

        let balances = self.coincheck_client.get_accounts_balance().await?;
        let key = self.config.key_currency();
        let balance_key = balances
            .get(&key)
            .ok_or(format!("balance {} is empty", key))?;

        let settlement = self.config.settlement_currency();
        let balance_settlement = balances
            .get(&settlement)
            .ok_or(format!("balance {} is empty", settlement))?;

        let begin = Utc::now() - Duration::hours(self.config.rate_period_minutes);
        let markets = self
            .mysql_client
            .select_markets(&self.config.target_pair, begin)?;

        let open_orders = self.coincheck_client.get_exchange_orders_opens().await?;

        let info = ExchangeInfo {
            pair: self.config.target_pair.clone(),
            sell_rate: sell_rate,
            buy_rate: buy_rate,
            balance_key: Balance {
                amount: balance_key.amount,
                reserved: balance_key.reserved,
            },
            balance_settlement: Balance {
                amount: balance_settlement.amount,
                reserved: balance_settlement.reserved,
            },
            open_orders: open_orders,
            rate_histories: markets.rate_histories(),
        };
        Ok(info)
    }

    fn upsert(&self, info: &ExchangeInfo) -> Result<(), Box<dyn Error>> {
        debug!("called upsert");

        let open_orders: Vec<&OpenOrder> = info
            .open_orders
            .iter()
            .filter(|o| o.pair == info.pair)
            .collect();
        let v = if open_orders.len() == 0 {
            -1.0
        } else {
            open_orders
                .iter()
                .map(|o| o.rate)
                .fold(0.0 / 0.0, |m, v| v.min(m))
        };
        self.mysql_client.upsert_bot_status(&BotStatus {
            bot_name: self.config.bot_name.to_owned(),
            r#type: "sell_rate".to_owned(),
            value: v,
            memo: "約定待ちの売注文レート".to_owned(),
        })?;

        let required = self.config.trend_line_period + self.config.trend_line_offset;
        let rates_size = info.rate_histories.len();
        if rates_size >= required {
            let trend_line_begin =
                rates_size - self.config.trend_line_period - self.config.trend_line_offset;
            let trend_line_end = rates_size - self.config.trend_line_offset;

            let resistance_lines = info.resistance_lines(trend_line_begin, trend_line_end);
            if let Some(resistance_line) = resistance_lines.last() {
                self.mysql_client.upsert_bot_status(&BotStatus {
                    bot_name: self.config.bot_name.to_owned(),
                    r#type: "resistance_line_value".to_owned(),
                    value: resistance_line.to_owned(),
                    memo: "レジスタンスラインの現在値".to_owned(),
                })?;

                if let Some(resistance_lines_before) = resistance_lines.get(rates_size - 2) {
                    self.mysql_client.upsert_bot_status(&BotStatus {
                        bot_name: self.config.bot_name.to_owned(),
                        r#type: "resistance_line_slope".to_owned(),
                        value: resistance_line - resistance_lines_before,
                        memo: "レジスタンスラインの傾き".to_owned(),
                    })?;
                }
            }

            let support_lines = info.support_lines(trend_line_begin, trend_line_end);
            if let Some(support_line) = support_lines.last() {
                self.mysql_client.upsert_bot_status(&BotStatus {
                    bot_name: self.config.bot_name.to_owned(),
                    r#type: "support_line_value".to_owned(),
                    value: support_line.to_owned(),
                    memo: "サポートラインの現在値".to_owned(),
                })?;

                if let Some(support_lines_before) = support_lines.get(rates_size - 2) {
                    self.mysql_client.upsert_bot_status(&BotStatus {
                        bot_name: self.config.bot_name.to_owned(),
                        r#type: "support_line_slope".to_owned(),
                        value: support_line - support_lines_before,
                        memo: "サポートラインの傾き".to_owned(),
                    })?;
                }
            }
        }

        Ok(())
    }

    fn make_params(&self, _info: &ExchangeInfo) -> Result<Vec<ActionParam>, Box<dyn Error>> {
        debug!("called make_params");

        Ok(vec![])
    }

    fn action(&self, _param: &ActionParam) -> Result<(), Box<dyn Error>> {
        debug!("called action");
        Ok(())
    }
}
