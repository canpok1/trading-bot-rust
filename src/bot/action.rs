use crate::bot::model::ActionType;
use crate::bot::model::AvgDownParam;
use crate::bot::model::EntryParam;
use crate::bot::model::LossCutParam;
use crate::bot::model::SellParam;
use crate::bot::model::SetProfitParam;
use crate::coincheck::model::Balance;
use crate::coincheck::model::NewOrder;
use crate::coincheck::model::Pair;
use crate::config::Config;
use crate::error::MyResult;
use crate::mysql::model::{Event, EventType};
use crate::slack::client::TextMessage;
use crate::{coincheck, mysql, slack};

use colored::Colorize;
use log::{debug, error, info, warn};
use std::{thread, time};

pub struct ActionBehavior<'a, T, U, V>
where
    T: slack::client::Client,
    U: mysql::client::Client,
    V: coincheck::client::Client,
{
    pub config: &'a Config,
    pub slack_client: &'a T,
    pub mysql_client: &'a U,
    pub coincheck_client: &'a V,
}

impl<T, U, V> ActionBehavior<'_, T, U, V>
where
    T: slack::client::Client,
    U: mysql::client::Client,
    V: coincheck::client::Client,
{
    pub async fn action(&self, t: &ActionType, balance: &Balance) -> MyResult<()> {
        match t {
            ActionType::Entry(param) => match self.action_entry(&balance, &param).await {
                Ok(_) => {
                    info!("{} entry ({:?})", "success".green(), param);
                }
                Err(err) => {
                    let message = format!("{} entry, {} ({:?})", "failure".red(), err, param);
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
            },
            ActionType::LossCut(param) => match self.action_loss_cut(&param).await {
                Ok(_) => {
                    info!("{} loss cut ({:?})", "success".green(), param);
                }
                Err(err) => {
                    let message = format!("{} loss cut, {} ({:?})", "failure".red(), err, param);
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
            ActionType::SetProfit(param) => match self.action_set_profit(&param).await {
                Ok(_) => {
                    info!("{} set profit ({:?})", "success".green(), param);
                }
                Err(err) => {
                    let message = format!("{} set profit , {} ({:?})", "failure".red(), err, param);
                    error!("{}", message);
                    if let Err(err) = self
                        .slack_client
                        .post_message(&TextMessage { text: message })
                        .await
                    {
                        error!("{}", err);
                    }
                    error!("{} set profit , {} ({:?})", "failure".red(), err, param);
                }
            },
            ActionType::Sell(param) => match self.action_sell(&param).await {
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
            ActionType::AvgDown(param) => match self.action_avg_down(&balance, &param).await {
                Ok(_) => {
                    info!("{} avg down ({:?})", "success".green(), param);
                }
                Err(err) => {
                    let message = format!("{} avg down, {} ({:?})", "failure".red(), err, param);
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
            },
            ActionType::Notify(param) => {
                info!("{}", param.log_message);
                if let Err(err) = self.slack_client.post_message(&param.slack_message).await {
                    error!("{}", err);
                }
            }
        }
        Ok(())
    }

    async fn action_entry(&self, balance_jpy: &Balance, param: &EntryParam) -> MyResult<()> {
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
        let rate = (used_jpy + profit_jpy) / amount_coin * (1.0 + param.offset_sell_rate_ratio);

        self.sell(&param.pair, rate, amount_coin).await?;

        if let Err(err) = self
            .slack_client
            .post_message(&TextMessage {
                text: format!(
                    "entry completed!\npair:`{}`\nrate:`{:.3}`\namount:`{:.3}`\nparam:`{:?}`",
                    self.config.target_pair, rate, amount_coin, param
                ),
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

    async fn action_loss_cut(&self, param: &LossCutParam) -> MyResult<()> {
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
                text: format!(
                    "losscut completed!\npair:`{}`\nparam:`{:?}`",
                    self.config.target_pair, param
                ),
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

    async fn action_sell(&self, param: &SellParam) -> MyResult<()> {
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
                text: format!(
                    "sell completed!\npair:`{}`\nparam:`{:?}`",
                    self.config.target_pair, param
                ),
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

    async fn action_avg_down(&self, balance_jpy: &Balance, param: &AvgDownParam) -> MyResult<()> {
        if self.config.demo_mode {
            info!("{}", "skip avg down as demo mode".green());
            return Ok(());
        }
        // ナンピンすると余裕なくなるならスキップする
        let required = param.buy_jpy_per_lot * self.config.keep_lot;
        if balance_jpy.amount - param.market_buy_amount < required {
            warn!(
                "{}",
                format!(
                    "skip avg down, balance jpy is too little ({:.3} < {:.3})",
                    balance_jpy.amount - param.market_buy_amount,
                    required
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

        // ナンピン後の注文は二分割する（ナンピンのための買注文の金額を肥大化させないため）
        let amount_coin = (param.open_order_amount + amount_new_coin) / 2.0;
        let rate = {
            let ratio = 1.0 + param.offset_sell_rate_ratio;
            let rate_without_offset = param.open_order_rate / ratio;
            ((param.open_order_amount * rate_without_offset) + param.market_buy_amount)
                / (amount_coin * 2.0)
                * ratio
        };
        self.sell(&param.pair, rate, amount_coin).await?;
        self.sell(&param.pair, rate, amount_coin).await?;

        if let Err(err) = self
            .slack_client
            .post_message(&TextMessage {
                text: format!(
                    "avg down completed!\npair:`{}`,rate:`{:.3}`\namount:`{:.3} * 2`\nparam:`{:?}`",
                    self.config.target_pair, rate, amount_coin, param
                ),
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

    async fn action_set_profit(&self, param: &SetProfitParam) -> MyResult<()> {
        if self.config.demo_mode {
            info!("{}", "skip set profit as demo mode".green());
            return Ok(());
        }

        // 注文キャンセル
        self.cancel(param.open_order_id).await?;

        // 成行売り注文
        self.market_sell(&param.pair, param.amount).await?;

        if let Err(err) = self
            .slack_client
            .post_message(&TextMessage {
                text: format!(
                    "set profit completed!\npair:`{}`\nparam:`{:?}`",
                    self.config.target_pair, param
                ),
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
    async fn market_buy(&self, pair: &Pair, amount_jpy: f64) -> MyResult<f64> {
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
            // 約定待ち
            thread::sleep(time::Duration::from_secs(
                self.config.external_service_wait_interval_sec,
            ));
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
            // 残高反映待ち
            thread::sleep(time::Duration::from_secs(
                self.config.external_service_wait_interval_sec,
            ));
        };

        Ok(amount_coin)
    }

    // 成行売り注文
    async fn market_sell(&self, pair: &Pair, amount_coin: f64) -> MyResult<()> {
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
    async fn sell(&self, pair: &Pair, rate: f64, amount_coin: f64) -> MyResult<()> {
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
    async fn cancel(&self, open_order_id: u64) -> MyResult<()> {
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
            // キャンセル待ち
            thread::sleep(time::Duration::from_secs(
                self.config.external_service_wait_interval_sec,
            ));
        }

        Ok(())
    }
}
