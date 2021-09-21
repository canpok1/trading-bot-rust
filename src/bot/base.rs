use crate::bot::action::ActionBehavior;
use crate::bot::analyze::TradeInfo;
use crate::bot::model::ActionType;
use crate::coincheck::model::{Balance, OpenOrder, OrderType, Pair};
use crate::config::Config;
use crate::error::MyResult;
use crate::mysql::model::{BotStatus, MarketsMethods};
use crate::{coincheck, mysql, slack, strategy};

use chrono::{DateTime, Duration, Utc};
use colored::Colorize;
use log::{debug, info};
use std::collections::HashMap;
use std::{thread, time};

pub struct Bot<'a, T, U, V, W>
where
    T: slack::client::Client,
    U: mysql::client::Client,
    V: coincheck::client::Client,
    W: strategy::base::Strategy,
{
    pub config: &'a Config,
    pub slack_client: &'a T,
    pub mysql_client: &'a U,
    pub coincheck_client: &'a V,
    pub strategy: &'a W,
    pub action_behavior: &'a ActionBehavior<'a, T, U, V>,
}

impl<T, U, V, W> Bot<'_, T, U, V, W>
where
    T: slack::client::Client,
    U: mysql::client::Client,
    V: coincheck::client::Client,
    W: strategy::base::Strategy,
{
    pub fn wait(&self) -> MyResult<()> {
        let d = time::Duration::from_secs(self.config.interval_sec);
        debug!("wait ... [{:?}]", d);
        thread::sleep(d);
        Ok(())
    }

    pub async fn trade(&self, now: &DateTime<Utc>) -> MyResult<()> {
        let info = self.fetch(now).await?;
        info!(
            "{}",
            format!(
                "{} sell:{:.3} buy:{:.3} {}[{}] {}[{}]",
                info.pair.to_string(),
                info.get_sell_rate()?,
                info.buy_rate,
                info.pair.key,
                info.get_balance_key()?,
                info.pair.settlement,
                info.get_balance_settlement()?,
            )
            .yellow(),
        );

        let buy_jpy_per_lot = self.calc_buy_jpy()?;

        self.upsert(&info)?;
        let params = self.strategy.judge(now, &info, buy_jpy_per_lot)?;
        self.action(params).await?;
        Ok(())
    }

    async fn fetch(&self, now: &DateTime<Utc>) -> MyResult<TradeInfo> {
        let pair = Pair::new(&self.config.target_pair)?;
        let buy_rate = self
            .coincheck_client
            .get_exchange_orders_rate(OrderType::Buy, &self.config.target_pair)
            .await?;

        let balances = self.coincheck_client.get_accounts_balance().await?;
        let mut sell_rates: HashMap<String, f64> = HashMap::new();
        for (k, _v) in balances.iter() {
            if k == &pair.settlement {
                continue;
            }
            let p = format!("{}_{}", k, &pair.settlement);
            let r = self
                .coincheck_client
                .get_exchange_orders_rate(OrderType::Sell, &p)
                .await?;
            sell_rates.insert(p, r);
        }

        let begin = *now - Duration::minutes(self.config.rate_period_minutes);
        let markets = self
            .mysql_client
            .select_markets(&self.config.target_pair, begin)?;
        let rate_histories = markets.rate_histories();
        let sell_volumes = markets.sell_volumes();
        let buy_volumes = markets.buy_volumes();

        let mut open_orders: Vec<OpenOrder> = vec![];
        for o in self.coincheck_client.get_exchange_orders_opens().await? {
            if o.pair == self.config.target_pair {
                open_orders.push(o);
            }
        }

        let support_lines_long = TradeInfo::support_lines(
            &rate_histories,
            self.config.support_line_period_long,
            self.config.support_line_offset,
        )?;
        let support_lines_short = TradeInfo::support_lines(
            &rate_histories,
            self.config.support_line_period_short,
            self.config.support_line_offset,
        )?;
        let resistance_lines = TradeInfo::resistance_lines(
            &rate_histories,
            self.config.resistance_line_period,
            self.config.resistance_line_offset,
        )?;

        let order_books = self
            .coincheck_client
            .get_order_books(&self.config.target_pair)
            .await?;

        let market_summary = self
            .mysql_client
            .select_market_summary(&self.config.target_pair, 1)?;

        Ok(TradeInfo {
            pair: pair,
            sell_rates: sell_rates,
            buy_rate: buy_rate,
            balances: balances,
            open_orders: open_orders,
            rate_histories: rate_histories,
            sell_volumes: sell_volumes,
            buy_volumes: buy_volumes,
            support_lines_long: support_lines_long,
            support_lines_short: support_lines_short,
            resistance_lines: resistance_lines,
            order_books: order_books,
            market_summary: market_summary,
        })
    }

    // fn fetch_balance_key(&self, balances: &HashMap<String, Balance>) -> MyResult<Balance> {
    //     let key = self.config.key_currency();
    //     let balance = balances
    //         .get(&key)
    //         .ok_or(format!("balance {} is empty", key))?;
    //     Ok(Balance {
    //         amount: balance.amount,
    //         reserved: balance.reserved,
    //     })
    // }

    fn fetch_balance_settlement(&self, balances: &HashMap<String, Balance>) -> MyResult<Balance> {
        let settlement = self.config.settlement_currency();
        let balance = balances
            .get(&settlement)
            .ok_or(format!("balance {} is empty", settlement))?;
        Ok(Balance {
            amount: balance.amount,
            reserved: balance.reserved,
        })
    }

    fn upsert(&self, info: &TradeInfo) -> MyResult<()> {
        let open_orders: Vec<&OpenOrder> = info
            .open_orders
            .iter()
            .filter(|o| {
                o.pair == info.pair.to_string()
                    && (o.order_type == OrderType::Sell || o.order_type == OrderType::MarketSell)
            })
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
            pair: info.pair.to_string(),
            r#type: "sell_rate".to_owned(),
            value: v,
            memo: "約定待ちの売注文レート".to_owned(),
        })?;

        let rates_size = info.rate_histories.len();

        let resistance_line = info.resistance_lines.last().unwrap();
        self.mysql_client.upsert_bot_status(&BotStatus {
            bot_name: self.config.bot_name.to_owned(),
            pair: info.pair.to_string(),
            r#type: "resistance_line_value".to_owned(),
            value: resistance_line.to_owned(),
            memo: "レジスタンスラインの現在値".to_owned(),
        })?;

        let resistance_lines_before = info.resistance_lines.get(rates_size - 2).unwrap();
        self.mysql_client.upsert_bot_status(&BotStatus {
            bot_name: self.config.bot_name.to_owned(),
            pair: info.pair.to_string(),
            r#type: "resistance_line_slope".to_owned(),
            value: resistance_line - resistance_lines_before,
            memo: "レジスタンスラインの傾き".to_owned(),
        })?;

        let support_line = info.support_lines_long.last().unwrap();
        self.mysql_client.upsert_bot_status(&BotStatus {
            bot_name: self.config.bot_name.to_owned(),
            pair: info.pair.to_string(),
            r#type: "support_line_value".to_owned(),
            value: support_line.to_owned(),
            memo: "サポートライン（長期）の現在値".to_owned(),
        })?;

        let support_lines_before = info.support_lines_long.get(rates_size - 2).unwrap();
        self.mysql_client.upsert_bot_status(&BotStatus {
            bot_name: self.config.bot_name.to_owned(),
            pair: info.pair.to_string(),
            r#type: "support_line_slope".to_owned(),
            value: support_line - support_lines_before,
            memo: "サポートライン（長期）の傾き".to_owned(),
        })?;

        let support_line = info.support_lines_short.last().unwrap();
        self.mysql_client.upsert_bot_status(&BotStatus {
            bot_name: self.config.bot_name.to_owned(),
            pair: info.pair.to_string(),
            r#type: "support_line_short_value".to_owned(),
            value: support_line.to_owned(),
            memo: "サポートライン（短期）の現在値".to_owned(),
        })?;

        let support_lines_before = info.support_lines_short.get(rates_size - 2).unwrap();
        self.mysql_client.upsert_bot_status(&BotStatus {
            bot_name: self.config.bot_name.to_owned(),
            pair: info.pair.to_string(),
            r#type: "support_line_short_slope".to_owned(),
            value: support_line - support_lines_before,
            memo: "サポートライン（短期）の傾き".to_owned(),
        })?;

        let long_trend = if info
            .is_up_trend(self.config.wma_period_short, self.config.wma_period_long)?
        {
            1.0
        } else if info.is_down_trend(self.config.wma_period_short, self.config.wma_period_long)? {
            2.0
        } else {
            0.0
        };
        self.mysql_client.upsert_bot_status(&BotStatus {
            bot_name: self.config.bot_name.to_owned(),
            pair: info.pair.to_string(),
            r#type: "long_trend".to_owned(),
            value: long_trend,
            memo: "長期トレンド（1:上昇, 2:下降）".to_owned(),
        })?;

        let total_balance_jpy = info.calc_total_balance_jpy();
        let total_jpy =
            match self
                .mysql_client
                .select_bot_status(&self.config.bot_name, "all", "total_jpy")
            {
                Ok(v) => v.value,
                Err(_) => 0.0,
            };

        if !info.has_position()? || total_jpy < total_balance_jpy {
            self.mysql_client.upsert_bot_status(&BotStatus {
                bot_name: self.config.bot_name.to_owned(),
                pair: "all".to_owned(),
                r#type: "total_jpy".to_owned(),
                value: total_balance_jpy,
                memo: "残高（JPY）".to_owned(),
            })?;
        }

        Ok(())
    }

    fn calc_buy_jpy(&self) -> MyResult<f64> {
        let total_jpy =
            self.mysql_client
                .select_bot_status(&self.config.bot_name, "all", "total_jpy")?;
        let buy_jpy = total_jpy.value * self.config.funds_ratio_per_order;
        Ok(buy_jpy)
    }

    async fn action(&self, tt: Vec<ActionType>) -> MyResult<()> {
        debug!("========== action ==========");
        if tt.is_empty() {
            info!("skip action (action is empty)");
            return Ok(());
        }

        for t in tt.iter() {
            let balances = self.coincheck_client.get_accounts_balance().await?;
            let balance_settlement = self.fetch_balance_settlement(&balances)?;
            self.action_behavior.action(t, &balance_settlement).await?;
        }
        Ok(())
    }
}
