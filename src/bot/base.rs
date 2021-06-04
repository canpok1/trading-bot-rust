use crate::bot::analyze::{SignalChecker, TradeInfo};
use crate::bot::model::AvgDownParam;
use crate::bot::model::NotifyParam;
use crate::bot::model::{ActionType, EntryParam, LossCutParam, SellParam};
use crate::coincheck;
use crate::coincheck::model::{Balance, NewOrder, OpenOrder, OrderType, Pair};
use crate::mysql;
use crate::mysql::model::{BotStatus, Event, EventType, MarketsMethods};
use crate::slack;
use crate::TextMessage;
use std::collections::HashMap;

use chrono::{Duration, Utc};
use colored::Colorize;
use log::{debug, error, info, warn};
use std::error::Error;
use std::{thread, time};

#[derive(Debug)]
pub struct Bot<'a> {
    pub config: &'a crate::config::Config,
    pub coincheck_client: &'a coincheck::client::Client,
    pub mysql_client: &'a mysql::client::Client,
    pub slack_client: &'a slack::client::Client,
    pub signal_checker: &'a SignalChecker<'a>,
}

impl Bot<'_> {
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

    async fn fetch(&self) -> Result<TradeInfo, Box<dyn Error>> {
        let pair = Pair::new(&self.config.target_pair)?;
        let sell_rate = self
            .coincheck_client
            .get_exchange_orders_rate(OrderType::Sell, &self.config.target_pair)
            .await?;
        let buy_rate = self
            .coincheck_client
            .get_exchange_orders_rate(OrderType::Buy, &self.config.target_pair)
            .await?;

        let balances = self.coincheck_client.get_accounts_balance().await?;

        let balance_key = self.fetch_balance_key(&balances)?;
        let balance_settlement = self.fetch_balance_settlement(&balances)?;

        let begin = Utc::now() - Duration::minutes(self.config.rate_period_minutes);
        let markets = self
            .mysql_client
            .select_markets(&self.config.target_pair, begin)?;
        let rate_histories = markets.rate_histories();
        let sell_volumes = markets.sell_volumes();
        let buy_volumes = markets.buy_volumes();

        let open_orders = self.coincheck_client.get_exchange_orders_opens().await?;

        Ok(TradeInfo {
            pair: pair,
            sell_rate: sell_rate,
            buy_rate: buy_rate,
            balance_key: balance_key,
            balance_settlement: balance_settlement,
            open_orders: open_orders,
            rate_histories: rate_histories,
            sell_volumes: sell_volumes,
            buy_volumes: buy_volumes,
        })
    }

    fn fetch_balance_key(
        &self,
        balances: &HashMap<String, Balance>,
    ) -> Result<Balance, Box<dyn Error>> {
        let key = self.config.key_currency();
        let balance = balances
            .get(&key)
            .ok_or(format!("balance {} is empty", key))?;
        Ok(Balance {
            amount: balance.amount,
            reserved: balance.reserved,
        })
    }

    fn fetch_balance_settlement(
        &self,
        balances: &HashMap<String, Balance>,
    ) -> Result<Balance, Box<dyn Error>> {
        let settlement = self.config.settlement_currency();
        let balance = balances
            .get(&settlement)
            .ok_or(format!("balance {} is empty", settlement))?;
        Ok(Balance {
            amount: balance.amount,
            reserved: balance.reserved,
        })
    }

    fn upsert(&self, analyzer: &TradeInfo) -> Result<(), Box<dyn Error>> {
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
            self.config.support_line_period_long,
            self.config.support_line_offset,
        ) {
            Ok(support_lines) => {
                let support_line = support_lines.last().unwrap();
                self.mysql_client.upsert_bot_status(&BotStatus {
                    bot_name: self.config.bot_name.to_owned(),
                    r#type: "support_line_value".to_owned(),
                    value: support_line.to_owned(),
                    memo: "サポートライン（長期）の現在値".to_owned(),
                })?;

                let support_lines_before = support_lines.get(rates_size - 2).unwrap();
                self.mysql_client.upsert_bot_status(&BotStatus {
                    bot_name: self.config.bot_name.to_owned(),
                    r#type: "support_line_slope".to_owned(),
                    value: support_line - support_lines_before,
                    memo: "サポートライン（長期）の傾き".to_owned(),
                })?;
            }
            Err(err) => {
                warn!(
                    "skip upsert bot_status, as failed to get support line ({})",
                    err
                );
            }
        }

        match analyzer.support_lines(
            self.config.support_line_period_short,
            self.config.support_line_offset,
        ) {
            Ok(support_lines) => {
                let support_line = support_lines.last().unwrap();
                self.mysql_client.upsert_bot_status(&BotStatus {
                    bot_name: self.config.bot_name.to_owned(),
                    r#type: "support_line_short_value".to_owned(),
                    value: support_line.to_owned(),
                    memo: "サポートライン（短期）の現在値".to_owned(),
                })?;

                let support_lines_before = support_lines.get(rates_size - 2).unwrap();
                self.mysql_client.upsert_bot_status(&BotStatus {
                    bot_name: self.config.bot_name.to_owned(),
                    r#type: "support_line_short_slope".to_owned(),
                    value: support_line - support_lines_before,
                    memo: "サポートライン（短期）の傾き".to_owned(),
                })?;
            }
            Err(err) => {
                warn!(
                    "skip upsert bot_status, as failed to get support line ({})",
                    err
                );
            }
        }

        let total_balance_jpy = analyzer.calc_total_balance_jpy();
        let total_jpy = match self
            .mysql_client
            .select_bot_status(&self.config.bot_name, "total_jpy")
        {
            Ok(v) => v.value,
            Err(_) => 0.0,
        };

        if !analyzer.has_position() || total_jpy < total_balance_jpy {
            self.mysql_client.upsert_bot_status(&BotStatus {
                bot_name: self.config.bot_name.to_owned(),
                r#type: "total_jpy".to_owned(),
                value: total_balance_jpy,
                memo: "残高（JPY）".to_owned(),
            })?;
        }

        Ok(())
    }

    fn make_params(&self, analyzer: &TradeInfo) -> Result<Vec<ActionType>, Box<dyn Error>> {
        let mut params: Vec<ActionType> = Vec::new();

        if let Some(action_type) = self.check_unused_coin(analyzer)? {
            params.push(action_type);
            return Ok(params);
        }
        let mut action_types = self.check_loss_cut_or_avg_down(analyzer)?;
        if !action_types.is_empty() {
            params.append(&mut action_types);
            return Ok(params);
        }

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

    // 未使用コインが一定以上なら通知
    fn check_unused_coin(
        &self,
        analyzer: &TradeInfo,
    ) -> Result<Option<ActionType>, Box<dyn Error>> {
        let border = 1.0;
        if analyzer.balance_key.amount < border {
            debug!(
                "{}",
                format!(
                    "has not unused coin (coin:{:.3} < border:{:.3})",
                    analyzer.balance_key.amount, border
                )
                .blue(),
            );
            return Ok(None);
        }
        info!(
            "has unused coin (coin:{} > border:{})",
            format!("{:.3}", analyzer.balance_key.amount).yellow(),
            format!("{:.3}", border).yellow(),
        );

        let message = format!(
            "unused coin exist ({} {})",
            self.config.key_currency(),
            analyzer.balance_key.amount
        );
        let action = ActionType::Notify(NotifyParam {
            log_message: message.to_string(),
            slack_message: TextMessage {
                text: message.to_string(),
            },
        });
        Ok(Some(action))
    }

    // 未決済注文のレートが現レートの一定以下なら損切りorナンピン
    fn check_loss_cut_or_avg_down(
        &self,
        analyzer: &TradeInfo,
    ) -> Result<Vec<ActionType>, Box<dyn Error>> {
        let mut actions = Vec::new();
        for open_order in &analyzer.open_orders {
            match open_order.order_type {
                OrderType::Sell => {
                    // 損切り？
                    let lower = open_order.rate * self.config.loss_cut_rate_ratio;
                    if analyzer.sell_rate < lower {
                        actions.push(ActionType::LossCut(LossCutParam {
                            pair: Pair::new(&self.config.target_pair)?,
                            open_order_id: open_order.id,
                            amount: open_order.pending_amount,
                        }));
                        continue;
                    }
                    // ナンピン？
                    let lower = open_order.rate * self.config.avg_down_rate_ratio;
                    let is_riging = if let Some(v) = analyzer.is_rate_rising() {
                        v
                    } else {
                        false
                    };
                    if analyzer.sell_rate < lower && is_riging {
                        actions.push(ActionType::AvgDown(AvgDownParam {
                            pair: Pair::new(&self.config.target_pair)?,
                            market_buy_amount: self.calc_buy_jpy()?,
                            open_order_id: open_order.id,
                            open_order_rate: open_order.rate,
                            open_order_amount: open_order.pending_amount,
                        }));
                        continue;
                    }
                }
                _ => {}
            }
        }
        Ok(actions)
    }

    fn check_entry_skip(&self, analyzer: &TradeInfo) -> Result<bool, Box<dyn Error>> {
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
            debug!(
                "{}",
                format!(
                    "NOT SKIP entry check (sell rate:{:.3} <= lower:{:.3})",
                    analyzer.sell_rate, lower_rate
                )
                .blue()
            );
        }

        // 短期の売りと買いの出来高差が一定以上ならスキップ
        let mut sell_volume = 0.0;
        for (i, v) in analyzer.sell_volumes.iter().rev().enumerate() {
            if i >= self.config.volume_period_short {
                break;
            }
            sell_volume += v;
        }
        let mut buy_volume = 0.0;
        for (i, v) in analyzer.buy_volumes.iter().rev().enumerate() {
            if i >= self.config.volume_period_short {
                break;
            }
            buy_volume += v;
        }
        let diff = sell_volume - buy_volume;
        if diff >= self.config.over_sell_volume_border {
            info!(
                "{} entry check (diff:{} >= border:{})(sell:{},buy:{})",
                "SKIP".red(),
                format!("{:.3}", diff).yellow(),
                format!("{:.3}", self.config.over_sell_volume_border).yellow(),
                format!("{:.3}", sell_volume).yellow(),
                format!("{:.3}", buy_volume).yellow(),
            );
            return Ok(true);
        }
        debug!(
            "{}",
            format!(
                "NOT SKIP entry check (diff:{:.3} < border:{:.3})(sell:{:.3},buy:{:.3})",
                diff, self.config.over_sell_volume_border, sell_volume, buy_volume,
            )
            .blue()
        );

        Ok(false)
    }

    // レジスタンスラインがブレイクアウトならエントリー
    fn check_resistance_line_breakout(
        &self,
        analyzer: &TradeInfo,
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
        if !analyzer.is_upper_rebound(
            &lines,
            width_upper,
            width_lower,
            self.config.rebound_check_period,
        ) {
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
                    profit_ratio: self.config.profit_ratio_per_order,
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
        analyzer: &TradeInfo,
    ) -> Result<Option<ActionType>, Box<dyn Error>> {
        // サポートライン（長期）関連の情報
        let result = analyzer.support_lines(
            self.config.support_line_period_long,
            self.config.support_line_offset,
        );
        if let Err(err) = result {
            info!(
                "{} not rebounded on the support line long ({})",
                "NG".red(),
                err
            );
            return Ok(None);
        }
        let support_lines_long = result.unwrap();
        let (is_rebounded_long, is_rebounded_long_info) =
            self.is_support_line_rebound(analyzer, &support_lines_long);
        let (on_support_line_long, on_support_line_long_info) =
            self.is_on_support_line(analyzer, &support_lines_long);

        // サポートライン（短期）関連の情報
        let result = analyzer.support_lines(
            self.config.support_line_period_short,
            self.config.support_line_offset,
        );
        if let Err(err) = result {
            info!(
                "{} not rebounded on the support line short ({})",
                "NG".red(),
                err
            );
            return Ok(None);
        }
        let support_lines_short = result.unwrap();
        let (is_rebounded_short, is_rebounded_short_info) =
            self.is_support_line_rebound(analyzer, &support_lines_short);
        let (on_support_line_short, on_support_line_short_info) =
            self.is_on_support_line(analyzer, &support_lines_short);

        // サポートラインのすぐ上でリバウンドしたかチェック
        if !is_rebounded_long && !is_rebounded_short {
            info!(
                "{} not rebounded on the support line ({})({})",
                "NG".red(),
                is_rebounded_long_info,
                is_rebounded_short_info,
            );
            return Ok(None);
        }

        // 現レートがサポートライン近くかをチェック
        if !on_support_line_long && !on_support_line_short {
            info!(
                "{} not rebounded on the support line ({})({})",
                "NG".red(),
                on_support_line_long_info,
                on_support_line_short_info,
            );
            return Ok(None);
        }

        match self.calc_buy_jpy() {
            Ok(buy_jpy) => {
                let long_slope = support_lines_long[1] - support_lines_long[0];
                let short_slope = support_lines_short[1] - support_lines_short[0];
                let profit_ratio = if long_slope > 0.0 && short_slope > 0.0 {
                    self.config.profit_ratio_per_order
                } else {
                    self.config.profit_ratio_per_order_on_down_trend
                };
                info!("{} rebounded on the support line", "OK".green());
                Ok(Some(ActionType::Entry(EntryParam {
                    pair: Pair::new(&self.config.target_pair)?,
                    amount: buy_jpy,
                    profit_ratio: profit_ratio,
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

    fn is_support_line_rebound(&self, analyzer: &TradeInfo, lines: &Vec<f64>) -> (bool, String) {
        let width_upper = analyzer.sell_rate * self.config.support_line_width_ratio_upper;
        let width_lower = analyzer.sell_rate * self.config.support_line_width_ratio_lower;
        let rebounded = analyzer.is_upper_rebound(
            lines,
            width_upper,
            width_lower,
            self.config.rebound_check_period,
        );
        if rebounded {
            (rebounded, "is_upper_rebound: true".to_string())
        } else {
            (rebounded, "is_upper_rebound: false".to_string())
        }
    }

    fn is_on_support_line(&self, analyzer: &TradeInfo, lines: &Vec<f64>) -> (bool, String) {
        let width_upper = analyzer.sell_rate * self.config.support_line_width_ratio_upper;
        let width_lower = analyzer.sell_rate * self.config.support_line_width_ratio_lower;
        let upper = lines.last().unwrap() + width_upper;
        let lower = lines.last().unwrap() - width_lower;
        let result = analyzer.sell_rate >= lower && analyzer.sell_rate <= upper;
        let message = if result {
            format!(
                "sell rate:{} is on support line:{}",
                format!("{:.3}", analyzer.sell_rate),
                format!("{:.3}...{:.3}", lower, upper)
            )
        } else {
            format!(
                "sell rate:{} is not on support line:{}",
                format!("{:.3}", analyzer.sell_rate),
                format!("{:.3}...{:.3}", lower, upper)
            )
        };
        (result, message)
    }

    fn calc_buy_jpy(&self) -> Result<f64, Box<dyn Error>> {
        let total_jpy = self
            .mysql_client
            .select_bot_status(&self.config.bot_name, "total_jpy")?;
        let buy_jpy = total_jpy.value * self.config.funds_ratio_per_order;
        Ok(buy_jpy)
    }

    async fn action(&self, tt: Vec<ActionType>) -> Result<(), Box<dyn Error>> {
        if tt.is_empty() {
            info!("skip action (action is empty)");
            return Ok(());
        }

        for t in tt.iter() {
            let balances = self.coincheck_client.get_accounts_balance().await?;
            let balance_settlement = self.fetch_balance_settlement(&balances)?;
            match t {
                ActionType::Entry(param) => {
                    match self.action_entry(&balance_settlement, param).await {
                        Ok(_) => {
                            info!("{} entry ({:?})", "success".green(), param);
                        }
                        Err(err) => {
                            let message =
                                format!("{} entry, {} ({:?})", "failure".red(), err, param);
                            error!("{}", message);
                            if let Err(err) = self
                                .slack_client
                                .post_message(&TextMessage { text: message })
                                .await
                            {
                                error!("{}", err);
                            }
                            error!("{} entry, {} ({:?})", "failure".red(), err, param);
                        }
                    }
                }
                ActionType::LossCut(param) => match self.action_loss_cut(param).await {
                    Ok(_) => {
                        info!("{} loss cut ({:?})", "success".green(), param);
                    }
                    Err(err) => {
                        let message =
                            format!("{} loss cut, {} ({:?})", "failure".red(), err, param);
                        error!("{}", message);
                        if let Err(err) = self
                            .slack_client
                            .post_message(&TextMessage { text: message })
                            .await
                        {
                            error!("{}", err);
                        }
                        error!("{} loss cut, {} ({:?})", "failure".red(), err, param);
                    }
                },
                ActionType::Sell(param) => match self.action_sell(param).await {
                    Ok(_) => {
                        info!("{} sell ({:?})", "success".green(), param);
                    }
                    Err(err) => {
                        let message = format!("{} sell, {} ({:?})", "failure".red(), err, param);
                        error!("{}", message);
                        if let Err(err) = self
                            .slack_client
                            .post_message(&TextMessage { text: message })
                            .await
                        {
                            error!("{}", err);
                        }
                        error!("{} sell, {} ({:?})", "failure".red(), err, param);
                    }
                },
                ActionType::AvgDown(param) => {
                    match self.action_avg_down(&balance_settlement, param).await {
                        Ok(_) => {
                            info!("{} avg down ({:?})", "success".green(), param);
                        }
                        Err(err) => {
                            let message =
                                format!("{} avg down, {} ({:?})", "failure".red(), err, param);
                            error!("{}", message);
                            if let Err(err) = self
                                .slack_client
                                .post_message(&TextMessage { text: message })
                                .await
                            {
                                error!("{}", err);
                            }
                            error!("{} avg down, {} ({:?})", "failure".red(), err, param);
                        }
                    }
                }
                ActionType::Notify(param) => {
                    info!("{}", param.log_message);
                    if let Err(err) = self.slack_client.post_message(&param.slack_message).await {
                        error!("{}", err);
                    }
                }
            }
        }
        Ok(())
    }

    async fn action_entry(
        &self,
        balance_jpy: &Balance,
        param: &EntryParam,
    ) -> Result<(), Box<dyn Error>> {
        if self.config.demo_mode {
            info!("{}", "skip entry as demo mode".green());
            return Ok(());
        }
        if balance_jpy.amount < param.amount {
            warn!(
                "{}",
                format!(
                    "skip entry, balance jpy is too little ({:.3} < {:.3})",
                    balance_jpy.amount, param.amount
                )
                .yellow()
            );
            return Ok(());
        }

        // 成行買い注文
        let amount_coin = self.market_buy(&param.pair, param.amount).await?;

        // 売り注文
        let used_jpy = param.amount;
        let profit_jpy = used_jpy * param.profit_ratio;
        let rate = (used_jpy + profit_jpy) / amount_coin;

        self.sell(&param.pair, rate, amount_coin).await?;

        if let Err(err) = self
            .slack_client
            .post_message(&TextMessage {
                text: format!("entry completed! `{:?}`", param),
            })
            .await
        {
            warn!(
                "{}",
                format!("failed to send message to slack, {}", err).yellow()
            );
        }

        Ok(())
    }

    async fn action_loss_cut(&self, param: &LossCutParam) -> Result<(), Box<dyn Error>> {
        if self.config.demo_mode {
            info!("{}", "skip loss cut as demo mode".green());
            return Ok(());
        }

        // 注文キャンセル
        self.cancel(param.open_order_id).await?;

        // 成行売り注文
        self.market_sell(&param.pair, param.amount).await?;

        if let Err(err) = self
            .slack_client
            .post_message(&TextMessage {
                text: format!("losscut completed! `{:?}`", param),
            })
            .await
        {
            warn!(
                "{}",
                format!("failed to send message to slack, {}", err).yellow()
            );
        }

        Ok(())
    }

    async fn action_sell(&self, param: &SellParam) -> Result<(), Box<dyn Error>> {
        if self.config.demo_mode {
            info!("{}", "skip sell as demo mode".green());
            return Ok(());
        }

        // 注文キャンセル
        for id in param.open_order_ids.iter() {
            self.cancel(*id).await?;
        }

        self.sell(&param.pair, param.rate, param.amount).await?;

        if let Err(err) = self
            .slack_client
            .post_message(&TextMessage {
                text: format!("sell completed! `{:?}`", param),
            })
            .await
        {
            warn!(
                "{}",
                format!("failed to send message to slack, {}", err).yellow()
            );
        }

        Ok(())
    }

    async fn action_avg_down(
        &self,
        balance_jpy: &Balance,
        param: &AvgDownParam,
    ) -> Result<(), Box<dyn Error>> {
        if self.config.demo_mode {
            info!("{}", "skip avg down as demo mode".green());
            return Ok(());
        }
        if balance_jpy.amount < param.market_buy_amount {
            warn!(
                "{}",
                format!(
                    "skip avg down, balance jpy is too little ({:.3} < {:.3})",
                    balance_jpy.amount, param.market_buy_amount
                )
                .yellow()
            );
            return Ok(());
        }

        // 成行買い注文
        let amount_new_coin = self
            .market_buy(&param.pair, param.market_buy_amount)
            .await?;

        self.cancel(param.open_order_id).await?;

        let amount_coin = param.open_order_amount + amount_new_coin;
        let rate = ((param.open_order_amount * param.open_order_rate) + param.market_buy_amount)
            / amount_coin;
        self.sell(&param.pair, rate, amount_coin).await?;

        if let Err(err) = self
            .slack_client
            .post_message(&TextMessage {
                text: format!("avg down completed! `{:?}`", param),
            })
            .await
        {
            warn!(
                "{}",
                format!("failed to send message to slack, {}", err).yellow()
            );
        }

        Ok(())
    }

    // 成行買い注文
    async fn market_buy(&self, pair: &Pair, amount_jpy: f64) -> Result<f64, Box<dyn Error>> {
        // 買い注文で増加したコイン数を算出するため最初の残高を保存しておく
        let coin_amount_begin = {
            let balances = self.coincheck_client.get_accounts_balance().await?;
            let balance = balances.get(&pair.key).unwrap();
            balance.amount
        };

        debug!("{}", "send market buy order".blue());
        let buy_order = {
            let req = NewOrder::new_market_buy_order(pair, amount_jpy);
            self.coincheck_client.post_exchange_orders(&req).await?
        };

        // 約定待ち
        debug!("{}", "wait contract ...".blue());
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

        let event = Event {
            pair: buy_order.pair,
            event_type: EventType::Buy,
            memo: format!(
                "market buy completed! `{} {}`",
                pair.to_string(),
                match buy_order.amount {
                    Some(v) => format!("{}", v),
                    None => "".to_owned(),
                },
            ),
            recorded_at: buy_order.created_at.naive_utc(),
        };
        if let Err(err) = self.mysql_client.insert_event(&event) {
            warn!(
                "{}",
                format!("failed to insert event, {} event = {:.?}", err, event).yellow()
            );
        }

        // 残高反映待ち
        debug!("{}", "wait update balance ...".blue());
        let amount_coin = loop {
            let balances = self.coincheck_client.get_accounts_balance().await?;
            let balance = balances.get(&pair.key).unwrap();
            let amount = balance.amount - coin_amount_begin;
            if amount > 0.0 {
                break amount;
            }
            // 残高反映待ちのため1秒待つ
            thread::sleep(time::Duration::from_secs(1));
        };

        Ok(amount_coin)
    }

    // 成行売り注文
    async fn market_sell(&self, pair: &Pair, amount_coin: f64) -> Result<(), Box<dyn Error>> {
        debug!("{}", "send market sell order".blue());
        let new_order = NewOrder::new_market_sell_order(pair, amount_coin);
        let order = self
            .coincheck_client
            .post_exchange_orders(&new_order)
            .await?;

        let event = Event {
            pair: order.pair,
            event_type: EventType::Sell,
            memo: format!(
                "market sell completed! `{} {}`",
                pair.to_string(),
                match order.amount {
                    Some(v) => format!("{:.3}", v),
                    None => "".to_owned(),
                },
            ),
            recorded_at: order.created_at.naive_utc(),
        };
        if let Err(err) = self.mysql_client.insert_event(&event) {
            warn!(
                "{}",
                format!("failed to insert event, {} event = {:.?}", err, event).yellow()
            );
        }

        Ok(())
    }

    // 指値売り注文
    async fn sell(&self, pair: &Pair, rate: f64, amount_coin: f64) -> Result<(), Box<dyn Error>> {
        let req = NewOrder::new_sell_order(pair, rate, amount_coin);
        let sell_order = self.coincheck_client.post_exchange_orders(&req).await?;
        debug!(
            "{}",
            format!(
                "send sell order (amount_coin:{:.3}, rate:{:.3})",
                amount_coin, rate
            )
            .blue(),
        );

        let event = Event {
            pair: sell_order.pair,
            event_type: EventType::Sell,
            memo: format!(
                "sell completed! `{} rate:{} amount:{}`",
                pair.to_string(),
                match sell_order.rate {
                    Some(v) => format!("{:.3}", v),
                    None => "".to_owned(),
                },
                match sell_order.amount {
                    Some(v) => format!("{:.3}", v),
                    None => "".to_owned(),
                },
            ),
            recorded_at: sell_order.created_at.naive_utc(),
        };
        if let Err(err) = self.mysql_client.insert_event(&event) {
            warn!(
                "{}",
                format!("failed to insert event, {} event = {:.?}", err, event).yellow()
            );
        }

        Ok(())
    }

    // 注文キャンセル
    async fn cancel(&self, open_order_id: u64) -> Result<(), Box<dyn Error>> {
        debug!("{}", "cancel".blue());
        let cancel_id = self
            .coincheck_client
            .delete_exchange_orders(open_order_id)
            .await?;

        // キャンセル待ち
        debug!("{}", "wait cancel completed ...".blue());
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

        Ok(())
    }
}
