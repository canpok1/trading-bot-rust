use crate::coincheck;
use crate::coincheck::model::{Balance, OpenOrder, OrderType};
use crate::error::Error::TooShort;
use crate::mysql;
use crate::mysql::model::{BotStatus, MarketsMethods};

use chrono::{Duration, Utc};
use colored::*;
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
    fn calc_total_balance_jpy(&self) -> f64 {
        self.balance_key.total() * self.sell_rate + self.balance_settlement.total()
    }

    fn has_position(&self) -> bool {
        self.balance_key.total() * self.sell_rate >= 1.0
    }

    fn support_lines(&self, period: usize, offset: usize) -> Result<Vec<f64>, Box<dyn Error>> {
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
            let (aa, bb) = ExchangeInfo::line_fit(&x, &y);
            a = aa;
            b = bb;
            begin = false;
        }
        Ok(ExchangeInfo::make_line(a, b, self.rate_histories.len()))
    }

    fn resistance_lines(&self, period: usize, offset: usize) -> Result<Vec<f64>, Box<dyn Error>> {
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
            let (aa, bb) = ExchangeInfo::line_fit(&x, &y);
            a = aa;
            b = bb;
            begin = false;
        }
        Ok(ExchangeInfo::make_line(a, b, self.rate_histories.len()))
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

    fn is_upper_rebound(&self, lines: Vec<f64>, width: f64, period: usize) -> bool {
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
            if !(rate1 < line1 || rate2 < line2 || rate3 < line3) {
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

#[derive(Debug)]
struct EntryParam {
    amount: f64,
}

#[derive(Debug)]
struct LossCutParam {
    id: u64,
}

#[derive(Debug)]
struct SellParam {}

enum ActionType {
    Entry(EntryParam),
    LossCut(LossCutParam),
    Sell(SellParam),
}

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

        let rates_size = info.rate_histories.len();

        let line_begin =
            rates_size - self.config.resistance_line_period - self.config.resistance_line_offset;
        let line_end = rates_size - self.config.resistance_line_offset;

        if let Ok(resistance_lines) = info.resistance_lines(line_begin, line_end) {
            let resistance_line = resistance_lines.last().unwrap();
            self.mysql_client.upsert_bot_status(&BotStatus {
                bot_name: self.config.bot_name.to_owned(),
                r#type: "resistance_line_value".to_owned(),
                value: resistance_line.to_owned(),
                memo: "レジスタンスラインの現在値".to_owned(),
            })?;

            let resistance_lines_before = resistance_lines.get(rates_size - 2).unwrap();
            self.mysql_client.upsert_bot_status(&BotStatus {
                bot_name: self.config.bot_name.to_owned(),
                r#type: "resistance_line_slope".to_owned(),
                value: resistance_line - resistance_lines_before,
                memo: "レジスタンスラインの傾き".to_owned(),
            })?;
        }

        let line_begin =
            rates_size - self.config.support_line_period - self.config.support_line_offset;
        let line_end = rates_size - self.config.support_line_offset;

        if let Ok(support_lines) = info.support_lines(line_begin, line_end) {
            let support_line = support_lines.last().unwrap();
            self.mysql_client.upsert_bot_status(&BotStatus {
                bot_name: self.config.bot_name.to_owned(),
                r#type: "support_line_value".to_owned(),
                value: support_line.to_owned(),
                memo: "サポートラインの現在値".to_owned(),
            })?;

            let support_lines_before = support_lines.get(rates_size - 2).unwrap();
            self.mysql_client.upsert_bot_status(&BotStatus {
                bot_name: self.config.bot_name.to_owned(),
                r#type: "support_line_slope".to_owned(),
                value: support_line - support_lines_before,
                memo: "サポートラインの傾き".to_owned(),
            })?;
        }

        if !info.has_position() {
            self.mysql_client.upsert_bot_status(&BotStatus {
                bot_name: self.config.bot_name.to_owned(),
                r#type: "total_jpy".to_owned(),
                value: info.calc_total_balance_jpy(),
                memo: "残高（JPY）".to_owned(),
            })?;
        }

        Ok(())
    }

    fn make_params(&self, info: &ExchangeInfo) -> Result<Vec<ActionType>, Box<dyn Error>> {
        debug!("called make_params");

        let mut params: Vec<ActionType> = Vec::new();

        if let Some(action_type) = self.check_unused_coin(info)? {
            params.push(action_type);
        }
        let mut action_types = self.check_loss_cut(info)?;
        params.append(&mut action_types);

        let buy_jpy = self.calc_buy_jpy()?;
        if info.balance_settlement.amount < buy_jpy {
            return Ok(params);
        }

        // TODO 長期トレンドが下降気味ならreturn

        if let Some(action_type) = self.check_resistance_line_breakout(info)? {
            params.push(action_type);
        } else if let Some(action_type) = self.check_support_line_rebound(info)? {
            params.push(action_type);
        }

        Ok(params)
    }

    // 未使用コインが一定以上なら売り注文
    fn check_unused_coin(&self, info: &ExchangeInfo) -> Result<Option<ActionType>, Box<dyn Error>> {
        let border = 1.0;
        if info.balance_key.amount < border {
            info!(
                "has not unused coin (coin:{} < border:{})",
                format!("{:.3}", info.balance_key.amount).yellow(),
                format!("{:.3}", border).yellow(),
            );
            return Ok(None);
        }
        info!(
            "has unused coin (coin:{} > border:{})",
            format!("{:.3}", info.balance_key.amount).yellow(),
            format!("{:.3}", border).yellow(),
        );

        let action = ActionType::Sell(SellParam {});
        Ok(Some(action))
    }

    // 未決済注文のレートが現レートの一定以下なら損切り
    fn check_loss_cut(&self, info: &ExchangeInfo) -> Result<Vec<ActionType>, Box<dyn Error>> {
        let mut actions = Vec::new();
        for open_order in &info.open_orders {
            let lower = open_order.rate * self.config.loss_cut_rate_ratio;
            if info.sell_rate < lower {
                actions.push(ActionType::LossCut(LossCutParam { id: open_order.id }))
            }
        }
        Ok(actions)
    }

    // レジスタンスラインがブレイクアウトならエントリー
    fn check_resistance_line_breakout(
        &self,
        info: &ExchangeInfo,
    ) -> Result<Option<ActionType>, Box<dyn Error>> {
        let result = info.resistance_lines(
            self.config.resistance_line_period,
            self.config.resistance_line_offset,
        );
        if let Err(err) = result {
            info!("{} resistance line not breakout ({})", "NG".red(), err);
            return Ok(None);
        }
        let lines = result.unwrap();

        // レジスタンスラインの傾きチェック
        let slope = lines[1] - lines[0];
        if slope < 0.0 {
            info!(
                "{} resistance line not breakout (slope:{} < 0.0)",
                "NG".red(),
                format!("{:.3}", slope).yellow(),
            );
            return Ok(None);
        }

        // レジスタンスラインのすぐ上でリバウンドしたかチェック
        let width = info.sell_rate * self.config.resistance_line_width_ratio;
        if !info.is_upper_rebound(lines, width, self.config.rebound_check_period) {
            info!(
                "{} resistance line not breakout (not roll reversal)",
                "NG".red()
            );
            return Ok(None);
        }

        info!("{} resistance line breakout (roll reversal)", "OK".green());
        Ok(Some(ActionType::Entry(EntryParam {
            amount: self.calc_buy_jpy()?,
        })))
    }

    // サポートラインがリバウンドしてるならエントリー
    fn check_support_line_rebound(
        &self,
        info: &ExchangeInfo,
    ) -> Result<Option<ActionType>, Box<dyn Error>> {
        let result = info.support_lines(
            self.config.support_line_period,
            self.config.support_line_offset,
        );
        if let Err(err) = result {
            info!("{} not rebounded on the support line ({})", "NG".red(), err);
            return Ok(None);
        }
        let lines = result.unwrap();

        // サポートラインのすぐ上でリバウンドしたかチェック
        let width = info.sell_rate * self.config.support_line_width_ratio;
        if !info.is_upper_rebound(lines, width, self.config.rebound_check_period) {
            info!("{} not rebounded on the support line", "NG".red());
            return Ok(None);
        }

        info!("{} rebounded on the support line", "OK".green());
        Ok(Some(ActionType::Entry(EntryParam {
            amount: self.calc_buy_jpy()?,
        })))
    }

    fn calc_buy_jpy(&self) -> Result<f64, Box<dyn Error>> {
        let total_jpy = self
            .mysql_client
            .select_bot_status(&self.config.bot_name, "total_jpy")?;
        let buy_jpy = total_jpy.value * self.config.funds_ratio_per_order;
        Ok(buy_jpy)
    }

    fn action(&self, t: &ActionType) -> Result<(), Box<dyn Error>> {
        match t {
            ActionType::Entry(param) => {
                debug!("action entry! {:?}", param);
                // TODO エントリー
            }
            ActionType::LossCut(param) => {
                debug!("action loss cut! {:?}", param);
                // TODO 損切り
            }
            ActionType::Sell(param) => {
                debug!("action sell! {:?}", param);
                // TODO 売り注文
            }
        }
        Ok(())
    }
}
