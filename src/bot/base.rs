use crate::bot::analyze::{SignalChecker, TradeInfo};
use crate::bot::model::{
    ActionType, AvgDownParam, EntryParam, LossCutParam, NotifyParam, SellParam,
};
use crate::coincheck;
use crate::coincheck::model::{Balance, NewOrder, OpenOrder, OrderType, Pair};
use crate::error::MyResult;
use crate::mysql;
use crate::mysql::model::{BotStatus, Event, EventType, MarketsMethods};
use crate::slack;
use crate::slack::client::TextMessage;

use chrono::{DateTime, Duration, Timelike, Utc};
use colored::Colorize;
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::{thread, time};

#[derive(Debug)]
pub struct Bot<'a, T, U, V>
where
    T: slack::client::Client,
    U: mysql::client::Client,
    V: coincheck::client::Client,
{
    pub config: &'a crate::config::Config,
    pub coincheck_client: &'a V,
    pub mysql_client: &'a U,
    pub slack_client: &'a T,
    pub signal_checker: &'a SignalChecker<'a>,
}

impl<T, U, V> Bot<'_, T, U, V>
where
    T: slack::client::Client,
    U: mysql::client::Client,
    V: coincheck::client::Client,
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

        self.upsert(&info)?;
        let params = self.make_params(now, &info)?;
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

    fn make_params(&self, now: &DateTime<Utc>, info: &TradeInfo) -> MyResult<Vec<ActionType>> {
        let mut params: Vec<ActionType> = Vec::new();

        debug!("========== check unused coin ==========");
        if let Some(action_type) = self.check_unused_coin(now, info)? {
            params.push(action_type);
            return Ok(params);
        }

        debug!("========== check loss cut or avd down ==========");
        let mut action_types = self.check_loss_cut_or_avg_down(now, info)?;
        if !action_types.is_empty() {
            params.append(&mut action_types);
        }

        debug!("========== check entry ==========");
        let skip = self.check_entry_skip(info)?;
        if skip {
            return Ok(params);
        }

        if let Some(action_type) = self.check_resistance_line_breakout(info)? {
            params.push(action_type);
        } else if let Some(action_type) = self.check_support_line_rebound(info)? {
            params.push(action_type);
        }

        Ok(params)
    }

    // 未使用コインが一定以上なら通知
    fn check_unused_coin(
        &self,
        now: &DateTime<Utc>,
        info: &TradeInfo,
    ) -> MyResult<Option<ActionType>> {
        let border = 1.0;
        if info.get_balance_key()?.amount < border {
            debug!(
                "{}",
                format!(
                    "NONE <= has not unused coin (coin:{:.3} < border:{:.3})",
                    info.get_balance_key()?.amount,
                    border
                )
                .blue(),
            );
            return Ok(None);
        }
        let minute = now.minute();
        if minute % 5 != 0 {
            debug!(
                "NONE <= has unused coin, but it is not notification timing now (coin:{} > border:{})(minute:{} % 5 != 0)",
                format!("{:.3}", info.get_balance_key()?.amount).yellow(),
                format!("{:.3}", border).yellow(),
                format!("{}", minute)
            );
            return Ok(None);
        }

        debug!(
            "Notify <= has unused coin (coin:{} > border:{})",
            format!("{:.3}", info.get_balance_key()?.amount).yellow(),
            format!("{:.3}", border).yellow(),
        );

        let message = format!(
            "unused coin exist ({} {})",
            self.config.key_currency(),
            info.get_balance_key()?.amount
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
        now: &DateTime<Utc>,
        info: &TradeInfo,
    ) -> MyResult<Vec<ActionType>> {
        let mut actions = Vec::new();
        if info.open_orders.is_empty() {
            debug!("{}", "NONE <= open orders is empty".blue());
        }
        for open_order in &info.open_orders {
            match open_order.order_type {
                OrderType::Sell => {
                    if let Some(a) = self.check_loss_cut(info, open_order)? {
                        actions.push(a);
                        continue;
                    }
                    if let Some(a) = self.check_avg_down(now, info, open_order)? {
                        actions.push(a);
                        continue;
                    }
                }
                _ => {}
            }
        }
        Ok(actions)
    }

    // ロスカット？
    fn check_loss_cut(
        &self,
        info: &TradeInfo,
        open_order: &OpenOrder,
    ) -> MyResult<Option<ActionType>> {
        let lower = open_order.rate * self.config.loss_cut_rate_ratio;
        if info.get_sell_rate()? < lower {
            info!(
                "{} <= (lower:{:.3} > sell rate:{:.3})",
                "Loss Cut".red(),
                lower,
                info.get_sell_rate()?
            );
            let action = ActionType::LossCut(LossCutParam {
                pair: Pair::new(&self.config.target_pair)?,
                open_order_id: open_order.id,
                amount: open_order.pending_amount,
            });
            Ok(Some(action))
        } else {
            Ok(None)
        }
    }

    // ナンピン？
    fn check_avg_down(
        &self,
        now: &DateTime<Utc>,
        info: &TradeInfo,
        open_order: &OpenOrder,
    ) -> MyResult<Option<ActionType>> {
        let lower = open_order.rate * self.config.avg_down_rate_ratio;
        let is_riging = if let Some(v) = info.is_rate_rising() {
            v
        } else {
            false
        };
        let holding_term = *now - open_order.created_at.with_timezone(&Utc);

        let (should, memo) = if info.buy_rate < lower && is_riging {
            (
                true,
                format!("(buy rate:{:.3} < lower:{:.3})", info.buy_rate, lower,),
            )
        } else if holding_term > Duration::minutes(self.config.hold_limit_minutes) && is_riging {
            (
                true,
                format!(
                    "({} > {}min)(created_at:{})",
                    holding_term, self.config.hold_limit_minutes, open_order.created_at
                ),
            )
        } else {
            (false, "".to_owned())
        };

        if should {
            let buy_jpy = self.calc_buy_jpy()?;
            let used_jpy = open_order.rate * open_order.pending_amount;

            let (market_buy_amount, action_memo) = {
                let mut used_jpy_tmp = 0.0;
                let mut lot = 1.0;
                while used_jpy_tmp <= used_jpy * 0.8 {
                    used_jpy_tmp += lot * buy_jpy;
                    // 1, 2, 4, 8, 16, 32 ... の割合でナンピンする
                    lot *= 2.0;
                }
                (
                    lot * buy_jpy,
                    format! {"lot: {}, buy_jpy: {}, used_jpy: {}", lot, buy_jpy, used_jpy},
                )
            };

            info!("{} <= {}", "AVG Down".red(), memo);

            let action = ActionType::AvgDown(AvgDownParam {
                pair: Pair::new(&self.config.target_pair)?,
                market_buy_amount: market_buy_amount,
                open_order_id: open_order.id,
                open_order_rate: open_order.rate,
                open_order_amount: open_order.pending_amount,
                memo: action_memo,
            });
            Ok(Some(action))
        } else {
            Ok(None)
        }
    }

    fn check_entry_skip(&self, info: &TradeInfo) -> MyResult<bool> {
        let mut skip = false;

        // 長期トレンドが下降トレンドならスキップ
        // 移動平均の短期が長期より下なら下降トレンドと判断
        let wma_short = info.wma(self.config.wma_period_short)?;
        let wma_long = info.wma(self.config.wma_period_long)?;
        if wma_short < wma_long {
            info!(
                "{} <= down trend (wma short:{} < wma long:{})(period short:{},long:{})",
                "SKIP".red(),
                format!("{:.3}", wma_short).yellow(),
                format!("{:.3}", wma_long).yellow(),
                format!("{}", self.config.wma_period_short).yellow(),
                format!("{}", self.config.wma_period_long).yellow(),
            );
            skip = true;
        } else {
            debug!(
                "{}",
                format!(
                "NOT SKIP <= up trend (wma short:{:.3} >= wma long:{:.3})(period short:{},long:{})",
                wma_short, wma_long, self.config.wma_period_short, self.config.wma_period_long,
            )
                .blue()
            );
        }

        // 未決済注文のレートが現レートとあまり離れてないならスキップ
        if !info.open_orders.is_empty() {
            let mut lower_rate = 0.0;
            for (i, o) in info.open_orders.iter().enumerate() {
                if i == 0 || lower_rate > o.rate {
                    lower_rate = o.rate;
                }
            }
            lower_rate *= self.config.entry_skip_rate_ratio;

            if info.get_sell_rate()? > lower_rate {
                info!(
                    "{} <= the diff between sell rate and open order rate is too wmall (sell rate:{} > lower_rate:{})",
                    "SKIP".red(),
                    format!("{:.3}", info.get_sell_rate()?).yellow(),
                    format!("{:.3}", lower_rate).yellow(),
                );
                skip = true;
            } else {
                debug!(
                    "{}",
                    format!(
                        "NOT SKIP <= the diff between sell rate and open order rate is enough (sell rate:{:.3} <= lower_rate:{:.3})",
                        info.get_sell_rate()?,
                        lower_rate
                    )
                    .blue()
                );
            }
        }

        // 直近の取引頻度が一定以上ならスキップ
        if info.market_summary.trade_frequency_ratio < self.config.required_trade_frequency_ratio {
            info!(
                "{} <= trade frequency is too low (frequency:{} < required:{})",
                "SKIP".red(),
                format!("{:.3}", info.market_summary.trade_frequency_ratio).yellow(),
                format!("{:.3}", self.config.required_trade_frequency_ratio).yellow(),
            );
            skip = true;
        } else {
            debug!(
                "{}",
                format!(
                    "NOT SKIP <= trade frequency is enough (frequency:{:.3} >= required:{:.3})",
                    info.market_summary.trade_frequency_ratio,
                    self.config.required_trade_frequency_ratio,
                )
                .blue()
            );
        }

        // 短期の売りと買いの出来高差が一定以上ならスキップ
        let mut sell_volume = 0.0;
        for (i, v) in info.sell_volumes.iter().rev().enumerate() {
            if i >= self.config.volume_period_short {
                break;
            }
            sell_volume += v;
        }
        let mut buy_volume = 0.0;
        for (i, v) in info.buy_volumes.iter().rev().enumerate() {
            if i >= self.config.volume_period_short {
                break;
            }
            buy_volume += v;
        }
        let diff = sell_volume - buy_volume;
        let over_sell_volume_border =
            info.market_summary.ex_volume_sell_total * self.config.over_sell_volume_ratio;
        if diff >= over_sell_volume_border {
            info!(
                "{} <= over sell (volume diff:{} >= border:{})(sell:{},buy:{})",
                "SKIP".red(),
                format!("{:.3}", diff).yellow(),
                format!("{:.3}", over_sell_volume_border).yellow(),
                format!("{:.3}", sell_volume).yellow(),
                format!("{:.3}", buy_volume).yellow(),
            );
            skip = true;
        } else {
            debug!(
                "{}",
                format!(
                    "NOT SKIP <= not over sell (volume diff:{:.3} < border:{:.3})(sell:{:.3},buy:{:.3})",
                    diff, over_sell_volume_border, sell_volume, buy_volume,
                )
                .blue()
            );
        }

        // 目標レートまでの板の厚さが短期売り出来高未満ならスキップ
        let sell_rate =
            self.estimate_sell_rate(info, self.config.profit_ratio_per_order_on_down_trend)?;
        let mut ask_total = 0.0;
        for ask in info.order_books.asks.iter() {
            if ask.rate < sell_rate {
                ask_total += ask.amount;
            }
        }
        let ask_total_upper = sell_volume * self.config.order_books_size_ratio;
        if ask_total > ask_total_upper {
            info!(
                "{} <= board is too heavy (ask_total:{} > upper:{})(sell_volume:{})",
                "SKIP".red(),
                format!("{:.3}", ask_total).yellow(),
                format!("{:.3}", ask_total_upper).yellow(),
                format!("{:.3}", sell_volume).yellow(),
            );
            skip = true;
        } else {
            debug!(
                "{}",
                format!(
                    "NOT SKIP <= board is not heavy (ask_total:{:.3} <= upper:{:.3})(sell_volume:{:.3})",
                    ask_total, ask_total_upper, sell_volume
                )
                .blue()
            );
        }

        Ok(skip)
    }

    // レジスタンスラインがブレイクアウトならエントリー
    fn check_resistance_line_breakout(&self, info: &TradeInfo) -> MyResult<Option<ActionType>> {
        let signal = self.signal_checker.check_resistance_line_breakout(info)?;

        if !signal.turned_on {
            info!(
                "NONE <= registance line not breakout, {}",
                signal.to_string()
            );
            return Ok(None);
        }

        match self.calc_buy_jpy() {
            Ok(buy_jpy) => {
                info!(
                    "{} <= resistance line breakout (roll reversal)",
                    "ENTRY".green()
                );
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
    fn check_support_line_rebound(&self, info: &TradeInfo) -> MyResult<Option<ActionType>> {
        let signal = self.signal_checker.check_support_line_rebound(info)?;
        if !signal.turned_on {
            info!(
                "NONE <= not rebounded on the support line, {}",
                signal.to_string()
            );
            return Ok(None);
        }

        match self.calc_buy_jpy() {
            Ok(buy_jpy) => {
                let long_slope = info.support_lines_long[1] - info.support_lines_long[0];
                let short_slope = info.support_lines_short[1] - info.support_lines_short[0];
                let profit_ratio = if long_slope > 0.0 && short_slope > 0.0 {
                    self.config.profit_ratio_per_order
                } else {
                    self.config.profit_ratio_per_order_on_down_trend
                };
                info!("{} <= rebounded on the support line", "ENTRY".green());
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

    fn calc_buy_jpy(&self) -> MyResult<f64> {
        let total_jpy =
            self.mysql_client
                .select_bot_status(&self.config.bot_name, "all", "total_jpy")?;
        let buy_jpy = total_jpy.value * self.config.funds_ratio_per_order;
        Ok(buy_jpy)
    }

    fn estimate_sell_rate(&self, info: &TradeInfo, profit_ratio: f64) -> MyResult<f64> {
        let buy_jpy = self.calc_buy_jpy()?;
        let amount = buy_jpy / info.buy_rate;
        let profit = buy_jpy * profit_ratio;
        Ok((buy_jpy + profit) / amount)
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
        let rate = (used_jpy + profit_jpy) / amount_coin;

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
        let required = self.calc_buy_jpy()? * self.config.keep_lot;
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
        let rate = ((param.open_order_amount * param.open_order_rate) + param.market_buy_amount)
            / (amount_coin * 2.0);
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
