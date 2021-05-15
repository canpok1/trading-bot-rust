use crate::bot::analyze::Analyzer;
use crate::coincheck;
use crate::coincheck::model::Pair;
use crate::coincheck::model::{Balance, OpenOrder, OrderType};
use crate::mysql;
use crate::mysql::model::{BotStatus, MarketsMethods};

use chrono::{Duration, Utc};
use colored::*;
use log::{debug, info, warn};
use std::error::Error;
use std::{thread, time};

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
        for param in params.iter() {
            self.action(param)?;
        }
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

        let buy_jpy = self.calc_buy_jpy()?;
        if buy_jpy == 0.0 {
            return Ok(params);
        }
        if analyzer.balance_settlement.amount < buy_jpy {
            return Ok(params);
        }

        // TODO 長期トレンドが下降気味ならreturn

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

        let action = ActionType::Sell(SellParam {});
        Ok(Some(action))
    }

    // 未決済注文のレートが現レートの一定以下なら損切り
    fn check_loss_cut(&self, analyzer: &Analyzer) -> Result<Vec<ActionType>, Box<dyn Error>> {
        let mut actions = Vec::new();
        for open_order in &analyzer.open_orders {
            let lower = open_order.rate * self.config.loss_cut_rate_ratio;
            if analyzer.sell_rate < lower {
                actions.push(ActionType::LossCut(LossCutParam { id: open_order.id }))
            }
        }
        Ok(actions)
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
        let width = analyzer.sell_rate * self.config.resistance_line_width_ratio;
        if !analyzer.is_upper_rebound(lines, width, self.config.rebound_check_period) {
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
        let lines = result.unwrap();

        // サポートラインのすぐ上でリバウンドしたかチェック
        let width = analyzer.sell_rate * self.config.support_line_width_ratio;
        if !analyzer.is_upper_rebound(lines, width, self.config.rebound_check_period) {
            info!("{} not rebounded on the support line", "NG".red());
            return Ok(None);
        }

        info!("{} rebounded on the support line", "OK".green());
        Ok(Some(ActionType::Entry(EntryParam {
            amount: self.calc_buy_jpy()?,
        })))
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
