use crate::bot::analyze::Analyzer;
use crate::coincheck;
use crate::coincheck::model::NewOrder;
use crate::coincheck::model::Pair;
use crate::coincheck::model::{Balance, OpenOrder, OrderType};
use crate::mysql;
use crate::mysql::model::{BotStatus, MarketsMethods};

use chrono::{Duration, Utc};
use colored::*;
use log::{debug, error, info, warn};
use std::error::Error;
use std::{thread, time};

#[derive(Debug)]
struct EntryParam {
    pair: Pair,
    amount: f64,
}

#[derive(Debug)]
struct LossCutParam {
    pair: Pair,
    open_order_id: u64,
    amount: f64,
}

#[derive(Debug)]
struct SellParam {
    pair: Pair,
    rate: f64,
    amount: f64,
}

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
        let analyzer = self.fetch().await?;
        info!(
            "{}",
            format!(
                "{} sell:{:.3} buy:{:.3} {}[{}] {}[{}]",
                analyzer.pair.to_string(),
                analyzer.sell_rate,
                analyzer.buy_rate,
                analyzer.pair.key,
                analyzer.balance_key,
                analyzer.pair.settlement,
                analyzer.balance_settlement,
            )
            .yellow(),
        );

        self.upsert(&analyzer)?;
        let params = self.make_params(&analyzer)?;
        self.action(params).await?;
        Ok(())
    }

    async fn fetch(&self) -> Result<Analyzer, Box<dyn Error>> {
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

        let analyzer = Analyzer {
            pair: Pair::new(&self.config.target_pair)?,
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
        Ok(analyzer)
    }

    fn upsert(&self, analyzer: &Analyzer) -> Result<(), Box<dyn Error>> {
        let open_orders: Vec<&OpenOrder> = analyzer
            .open_orders
            .iter()
            .filter(|o| o.pair == analyzer.pair.to_string())
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

        let rates_size = analyzer.rate_histories.len();

        match analyzer.resistance_lines(
            self.config.resistance_line_period,
            self.config.resistance_line_offset,
        ) {
            Ok(resistance_lines) => {
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
            Err(err) => {
                warn!(
                    "skip upsert bot_status, as failed to get resistance lines ({})",
                    err
                );
            }
        }

        match analyzer.support_lines(
            self.config.support_line_period,
            self.config.support_line_offset,
        ) {
            Ok(support_lines) => {
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
            Err(err) => {
                warn!(
                    "skip upsert bot_status, as failed to get support line ({})",
                    err
                );
            }
        }

        if !analyzer.has_position() {
            self.mysql_client.upsert_bot_status(&BotStatus {
                bot_name: self.config.bot_name.to_owned(),
                r#type: "total_jpy".to_owned(),
                value: analyzer.calc_total_balance_jpy(),
                memo: "残高（JPY）".to_owned(),
            })?;
        }

        Ok(())
    }

    fn make_params(&self, analyzer: &Analyzer) -> Result<Vec<ActionType>, Box<dyn Error>> {
        let mut params: Vec<ActionType> = Vec::new();

        if let Some(action_type) = self.check_unused_coin(analyzer)? {
            params.push(action_type);
        }
        let mut action_types = self.check_loss_cut(analyzer)?;
        params.append(&mut action_types);

        let skip = self.check_entry_skip(analyzer)?;
        if skip {
            return Ok(params);
        }

        if let Some(action_type) = self.check_resistance_line_breakout(analyzer)? {
            params.push(action_type);
        } else if let Some(action_type) = self.check_support_line_rebound(analyzer)? {
            params.push(action_type);
        }

        Ok(params)
    }

    // 未使用コインが一定以上なら売り注文
    fn check_unused_coin(&self, analyzer: &Analyzer) -> Result<Option<ActionType>, Box<dyn Error>> {
        let border = 1.0;
        if analyzer.balance_key.amount < border {
            info!(
                "has not unused coin (coin:{} < border:{})",
                format!("{:.3}", analyzer.balance_key.amount).yellow(),
                format!("{:.3}", border).yellow(),
            );
            return Ok(None);
        }
        info!(
            "has unused coin (coin:{} > border:{})",
            format!("{:.3}", analyzer.balance_key.amount).yellow(),
            format!("{:.3}", border).yellow(),
        );

        let used_jpy = self.calc_used_jpy(analyzer)?;
        let profit_jpy = used_jpy * self.config.funds_ratio_per_order;
        let rate = (used_jpy + profit_jpy) / analyzer.balance_key.amount;
        let action = ActionType::Sell(SellParam {
            pair: Pair::new(&self.config.target_pair)?,
            rate: rate,
            amount: analyzer.balance_key.amount,
        });
        Ok(Some(action))
    }

    // 未決済注文のレートが現レートの一定以下なら損切り
    fn check_loss_cut(&self, analyzer: &Analyzer) -> Result<Vec<ActionType>, Box<dyn Error>> {
        let mut actions = Vec::new();
        for open_order in &analyzer.open_orders {
            match open_order.order_type {
                OrderType::Sell => {
                    let lower = open_order.rate * self.config.loss_cut_rate_ratio;
                    if analyzer.sell_rate < lower {
                        actions.push(ActionType::LossCut(LossCutParam {
                            pair: Pair::new(&self.config.target_pair)?,
                            open_order_id: open_order.id,
                            amount: open_order.pending_amount,
                        }));
                    }
                }
                _ => {}
            }
        }
        Ok(actions)
    }

    fn check_entry_skip(&self, analyzer: &Analyzer) -> Result<bool, Box<dyn Error>> {
        // TODO 長期トレンドが下降気味ならreturn

        // 未決済注文のレートが現レートとあまり離れてないならスキップ
        if !analyzer.open_orders.is_empty() {
            let mut lower_rate = 0.0;
            for (i, o) in analyzer.open_orders.iter().enumerate() {
                if i == 0 || lower_rate > o.rate {
                    lower_rate = o.rate;
                }
            }
            lower_rate *= self.config.entry_skip_rate_ratio;

            if analyzer.sell_rate > lower_rate {
                info!(
                    "{} entry check (sell rate:{} > lower_rate:{} )",
                    "SKIP".red(),
                    format!("{:.3}", analyzer.sell_rate).yellow(),
                    format!("{:.3}", lower_rate).yellow(),
                );
                return Ok(true);
            }
        }

        // 残高JPYが足りず注文出せないならスキップ
        let buy_jpy = self.calc_buy_jpy()?;
        if analyzer.balance_settlement.amount < buy_jpy {
            info!(
                "{} entry check (jpy:{} < buy_jpy:{} )",
                "SKIP".red(),
                format!("{:.3}", analyzer.balance_settlement.amount).yellow(),
                format!("{:.3}", buy_jpy).yellow(),
            );
            return Ok(true);
        }

        Ok(false)
    }

    // レジスタンスラインがブレイクアウトならエントリー
    fn check_resistance_line_breakout(
        &self,
        analyzer: &Analyzer,
    ) -> Result<Option<ActionType>, Box<dyn Error>> {
        let result = analyzer.resistance_lines(
            self.config.resistance_line_period,
            self.config.resistance_line_offset,
        );
        if let Err(err) = result {
            info!("{} resistance line not breakout ({})", "NG".red(), err);
            return Ok(None);
        }

        // レジスタンスライン関連の情報
        let lines = result.unwrap();
        let slope = lines[1] - lines[0];

        let width_upper = analyzer.sell_rate * self.config.resistance_line_width_ratio_upper;
        let width_lower = analyzer.sell_rate * self.config.resistance_line_width_ratio_lower;

        let upper = lines.last().unwrap() + width_upper;
        let lower = lines.last().unwrap() + width_lower;

        // レジスタンスラインの傾きチェック
        if slope < 0.0 {
            info!(
                "{} resistance line not breakout (slope:{} < 0.0)",
                "NG".red(),
                format!("{:.3}", slope).yellow(),
            );
            return Ok(None);
        }

        // レジスタンスラインのすぐ上でリバウンドしたかチェック
        if !analyzer.is_upper_rebound(lines, width_upper, self.config.rebound_check_period) {
            info!(
                "{} resistance line not breakout (not roll reversal)",
                "NG".red()
            );
            return Ok(None);
        }

        // 現レートがレジスタンスライン近くかをチェック
        if analyzer.sell_rate < lower || analyzer.sell_rate > upper {
            info!(
                "{} resistance line not breakout (sell rate:{} is out of range:{})",
                "NG".red(),
                format!("{:.3}", analyzer.sell_rate),
                format!("{:.3}...{:.3}", lower, upper),
            );
            return Ok(None);
        }

        // レート上昇中かチェック
        let before_rate = *analyzer.rate_histories.last().unwrap();
        if analyzer.sell_rate <= before_rate {
            info!(
                "{} resistance line not breakout (sell rate is not rising) (sell rate:{} <= before:{})",
                "NG".red(),
                format!("{:.3}", analyzer.sell_rate),
                format!("{:.3}", before_rate),
            );
            return Ok(None);
        }

        match self.calc_buy_jpy() {
            Ok(buy_jpy) => {
                info!("{} resistance line breakout (roll reversal)", "OK".green());
                Ok(Some(ActionType::Entry(EntryParam {
                    pair: Pair::new(&self.config.target_pair)?,
                    amount: buy_jpy,
                })))
            }
            Err(err) => {
                warn!(
                    "{} resistance line not breakout (failed to calc buy_jpy) ({})",
                    "NG".red(),
                    err
                );
                Ok(None)
            }
        }
    }

    // サポートラインがリバウンドしてるならエントリー
    fn check_support_line_rebound(
        &self,
        analyzer: &Analyzer,
    ) -> Result<Option<ActionType>, Box<dyn Error>> {
        let result = analyzer.support_lines(
            self.config.support_line_period,
            self.config.support_line_offset,
        );
        if let Err(err) = result {
            info!("{} not rebounded on the support line ({})", "NG".red(), err);
            return Ok(None);
        }

        // サポートライン関連の情報
        let lines = result.unwrap();
        let width_upper = analyzer.sell_rate * self.config.support_line_width_ratio_upper;
        let width_lower = analyzer.sell_rate * self.config.support_line_width_ratio_lower;
        let upper = lines.last().unwrap() + width_upper;
        let lower = lines.last().unwrap() - width_lower;

        // サポートラインのすぐ上でリバウンドしたかチェック
        if !analyzer.is_upper_rebound(lines, width_upper, self.config.rebound_check_period) {
            info!("{} not rebounded on the support line", "NG".red());
            return Ok(None);
        }

        // 現レートがサポートライン近くかをチェック
        if analyzer.sell_rate < lower || analyzer.sell_rate > upper {
            info!(
                "{} not rebounded on the support line (sell rate:{} is out of range:{})",
                "NG".red(),
                format!("{:.3}", analyzer.sell_rate),
                format!("{:.3}...{:.3}", lower, upper),
            );
            return Ok(None);
        }

        match self.calc_buy_jpy() {
            Ok(buy_jpy) => {
                info!("{} rebounded on the support line", "OK".green());
                Ok(Some(ActionType::Entry(EntryParam {
                    pair: Pair::new(&self.config.target_pair)?,
                    amount: buy_jpy,
                })))
            }
            Err(err) => {
                warn!(
                    "{} not rebounded on the support line (failed to calc buy_jpy) ({})",
                    "NG".green(),
                    err
                );
                Ok(None)
            }
        }
    }

    fn calc_buy_jpy(&self) -> Result<f64, Box<dyn Error>> {
        let result = self
            .mysql_client
            .select_bot_status(&self.config.bot_name, "total_jpy");
        if let Err(err) = result {
            warn!("failed to select bot_status, {}", err);
            return Ok(0.0);
        }

        let total_jpy = result.unwrap();
        let buy_jpy = total_jpy.value * self.config.funds_ratio_per_order;
        Ok(buy_jpy)
    }

    fn calc_used_jpy(&self, analyzer: &Analyzer) -> Result<f64, Box<dyn Error>> {
        let result = self
            .mysql_client
            .select_bot_status(&self.config.bot_name, "total_jpy");
        if let Err(err) = result {
            warn!("failed to select bot_status, {}", err);
            return Ok(0.0);
        }

        let total_jpy = result.unwrap();
        let used_jpy = total_jpy.value - analyzer.balance_settlement.total();
        Ok(used_jpy)
    }

    async fn action(&self, tt: Vec<ActionType>) -> Result<(), Box<dyn Error>> {
        if tt.is_empty() {
            info!("skip action (action is empty)");
            return Ok(());
        }

        for t in tt.iter() {
            match t {
                ActionType::Entry(param) => match self.action_entry(param).await {
                    Ok(_) => {
                        info!("{} entry ({:?})", "success".green(), param);
                    }
                    Err(err) => {
                        error!("{} entry, {} ({:?})", "failure".red(), err, param);
                    }
                },
                ActionType::LossCut(param) => match self.action_loss_cut(param).await {
                    Ok(_) => {
                        info!("{} loss cut ({:?})", "success".green(), param);
                    }
                    Err(err) => {
                        error!("{} loss cut, {} ({:?})", "failure".red(), err, param);
                    }
                },
                ActionType::Sell(param) => match self.action_sell(param).await {
                    Ok(_) => {
                        info!("{} sell ({:?})", "success".green(), param);
                    }
                    Err(err) => {
                        error!("{} sell, {} ({:?})", "failure".red(), err, param);
                    }
                },
            }
        }
        Ok(())
    }

    async fn action_entry(&self, param: &EntryParam) -> Result<(), Box<dyn Error>> {
        if self.config.demo_mode {
            info!("{}", "skip entry as demo mode".green());
            return Ok(());
        }

        // 買い注文で増加したコイン数を算出するため最初の残高を保存しておく
        let coin_amount_begin = {
            let balances = self.coincheck_client.get_accounts_balance().await?;
            let balance = balances.get(&param.pair.key).unwrap();
            balance.amount
        };

        // 成行買い注文
        let buy_order = {
            let req = NewOrder::new_market_buy_order(&param.pair, param.amount);
            self.coincheck_client.post_exchange_orders(&req).await?
        };

        // 約定待ち
        loop {
            let open_orders = self.coincheck_client.get_exchange_orders_opens().await?;
            let mut contracted = true;
            for open_order in open_orders {
                if open_order.id == buy_order.id {
                    contracted = false;
                    break;
                }
            }
            if contracted {
                break;
            }
            // 約定待ちのため1秒待つ
            thread::sleep(time::Duration::from_secs(1));
        }

        // 売り注文
        let total_jpy = param.amount * (1.0 + self.config.profit_ratio_per_order);
        let coin_amount = {
            let balances = self.coincheck_client.get_accounts_balance().await?;
            let balance = balances.get(&param.pair.key).unwrap();
            balance.amount - coin_amount_begin
        };
        let rate = total_jpy / coin_amount;
        let req = NewOrder::new_sell_order(&param.pair, rate, coin_amount);
        let _order = self.coincheck_client.post_exchange_orders(&req).await?;

        Ok(())
    }

    async fn action_loss_cut(&self, param: &LossCutParam) -> Result<(), Box<dyn Error>> {
        if self.config.demo_mode {
            info!("{}", "skip loss cut as demo mode".green());
            return Ok(());
        }

        // 注文キャンセル
        let cancel_id = self
            .coincheck_client
            .delete_exchange_orders(param.open_order_id)
            .await?;

        // キャンセル待ち
        loop {
            let canceled = self
                .coincheck_client
                .get_exchange_orders_cancel_status(cancel_id)
                .await?;
            if canceled {
                break;
            }
            // キャンセル待ちのため1秒待つ
            thread::sleep(time::Duration::from_secs(1));
        }

        // 成行売り注文
        let new_order = NewOrder::new_market_sell_order(&param.pair, param.amount);
        let _order = self
            .coincheck_client
            .post_exchange_orders(&new_order)
            .await?;
        Ok(())
    }

    async fn action_sell(&self, param: &SellParam) -> Result<(), Box<dyn Error>> {
        if self.config.demo_mode {
            info!("{}", "skip sell as demo mode".green());
            return Ok(());
        }

        let order = NewOrder::new_sell_order(&param.pair, param.rate, param.amount);
        self.coincheck_client.post_exchange_orders(&order).await?;
        Ok(())
    }
}
