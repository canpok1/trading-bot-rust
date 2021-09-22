use crate::bot::model::{
    ActionType, AvgDownParam, EntryParam, LineMethod, LossCutParam, NotifyParam, SetProfitParam,
    TradeInfo,
};
use crate::coincheck::model::Pair;
use crate::coincheck::model::{OpenOrder, OrderType};
use crate::error::MyResult;
use crate::slack::client::TextMessage;
use crate::strategy::base::Strategy;
use crate::util;
use chrono::{DateTime, Utc};
use colored::Colorize;
use log::{debug, info};

pub struct ScalpingStrategy<'a> {
    pub config: &'a crate::config::Config,
}

impl Strategy for ScalpingStrategy<'_> {
    fn judge(
        &self,
        now: &DateTime<Utc>,
        info: &TradeInfo,
        buy_jpy_per_lot: f64,
    ) -> MyResult<Vec<ActionType>> {
        let mut actions: Vec<ActionType> = Vec::new();

        debug!("========== check unused coin ==========");
        if let Some(action_type) = self.check_unused_coin(now, info)? {
            actions.push(action_type);
            return Ok(actions);
        }

        debug!("========== check open orders ==========");
        let mut action_types = self.check_open_orders(now, info, buy_jpy_per_lot)?;
        if !action_types.is_empty() {
            actions.append(&mut action_types);
        }

        debug!("========== check entry ==========");
        let should = self.should_check_entry(info, buy_jpy_per_lot)?;
        if !should {
            return Ok(actions);
        }

        if let Some(action_type) = self.check_resistance_line_breakout(info, buy_jpy_per_lot)? {
            actions.push(action_type);
        } else if let Some(action_type) = self.check_support_line_rebound(info, buy_jpy_per_lot)? {
            actions.push(action_type);
        }

        Ok(actions)
    }
}

impl ScalpingStrategy<'_> {
    // 未使用コインが一定以上なら通知
    fn check_unused_coin(
        &self,
        now: &DateTime<Utc>,
        info: &TradeInfo,
    ) -> MyResult<Option<ActionType>> {
        let (is_notification_timing, memo) = util::is_notification_timing(now);
        if !is_notification_timing {
            debug!("{}", format!("NONE <= {}", memo).blue());
            return Ok(None);
        }

        let (has_unused_coin, memo) = util::has_unused_coin(info.get_balance_key()?, 1.0);
        if has_unused_coin {
            debug!("{}", format!("NONE <= {}", memo).blue());
            return Ok(None);
        }
        debug!("Notify <= {}", memo);

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

    // 未決済注文の確認（損切り, ナンピン, 利確）
    fn check_open_orders(
        &self,
        now: &DateTime<Utc>,
        info: &TradeInfo,
        buy_jpy_per_lot: f64,
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
                    if let Some(a) = self.check_avg_down(now, info, open_order, buy_jpy_per_lot)? {
                        actions.push(a);
                        continue;
                    }
                    if let Some(a) = self.check_set_profit(info, open_order)? {
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
        let (should_loss_cut, memo) = util::should_loss_cut(
            info.get_sell_rate()?,
            open_order,
            self.config.loss_cut_rate_ratio,
            self.config.offset_sell_rate_ratio,
        );
        if should_loss_cut {
            info!("{} <= {}", "Loss Cut".red(), memo,);
            let action = ActionType::LossCut(LossCutParam {
                pair: Pair::new(&self.config.target_pair)?,
                open_order_id: open_order.id,
                amount: open_order.pending_amount,
            });
            Ok(Some(action))
        } else {
            debug!("{}", format!("NONE <= {}", memo).blue(),);
            Ok(None)
        }
    }

    // ナンピン？
    fn check_avg_down(
        &self,
        now: &DateTime<Utc>,
        info: &TradeInfo,
        open_order: &OpenOrder,
        buy_jpy_per_lot: f64,
    ) -> MyResult<Option<ActionType>> {
        let slope = util::calc_slope(&info.support_lines_short)?;
        if slope <= 0.0 {
            debug!(
                "{}",
                format!(
                    "NONE <= should not avg down, support line is falling, slope:{:.3} < 0.0",
                    slope
                )
                .blue()
            );
            return Ok(None);
        }

        let (should, memo) = util::should_avg_down(
            now,
            info.buy_rate,
            open_order,
            self.config.avg_down_rate_ratio,
            self.config.offset_sell_rate_ratio,
            self.config.hold_limit_minutes,
        );
        if !should {
            debug!("{}", format!("NONE <= {}", memo).blue());
            return Ok(None);
        }

        let (market_buy_amount, memo) = util::calc_avg_down_buy_amount(
            buy_jpy_per_lot,
            open_order,
            self.config.offset_sell_rate_ratio,
        );
        info!("{} <= {}", "AVG Down".red(), memo);

        let action = ActionType::AvgDown(AvgDownParam {
            pair: Pair::new(&self.config.target_pair)?,
            buy_jpy_per_lot: buy_jpy_per_lot,
            market_buy_amount: market_buy_amount,
            open_order_id: open_order.id,
            open_order_rate: open_order.rate,
            open_order_amount: open_order.pending_amount,
            offset_sell_rate_ratio: self.config.offset_sell_rate_ratio,
            memo: memo,
        });
        Ok(Some(action))
    }

    // 利確？
    fn check_set_profit(
        &self,
        info: &TradeInfo,
        open_order: &OpenOrder,
    ) -> MyResult<Option<ActionType>> {
        let current = info.get_sell_rate()?;
        if let Some(before) = info.sell_rate_histories.get_current() {
            if current > before {
                debug!("{}", format!("NONE <= should not set profit, rate is rising, current:{:.3} > before:{:.3}", current, before ).blue());
                return Ok(None);
            }
        } else {
            debug!(
                "{}",
                "NONE <= should not set profit, before rate is nothing".blue()
            );
            return Ok(None);
        }

        let (should_set_profit, memo) =
            util::should_set_profit(current, open_order, self.config.offset_sell_rate_ratio);
        if should_set_profit {
            info!("{} <= {}", "Set Profit".green(), memo);
            let action = ActionType::SetProfit(SetProfitParam {
                pair: Pair::new(&self.config.target_pair)?,
                open_order_id: open_order.id,
                amount: open_order.pending_amount,
            });
            Ok(Some(action))
        } else {
            debug!("{}", format!("NONE <= {}", memo).blue());
            Ok(None)
        }
    }

    fn should_check_entry(&self, info: &TradeInfo, buy_jpy_per_lot: f64) -> MyResult<bool> {
        let mut should = true;

        // 長期トレンドが下降トレンドならスキップ
        // 移動平均の短期が長期より下なら下降トレンドと判断
        let wma_short = info.wma(self.config.wma_period_short)?;
        let wma_long = info.wma(self.config.wma_period_long)?;
        let (is_down_trend, memo) = util::is_down_trend(wma_short, wma_long);
        if is_down_trend {
            info!("{} <= {}", "SKIP".red(), memo,);
            should = false;
        } else {
            debug!("{}", format!("NOT SKIP <= {}", memo,).blue());
        }

        // 未決済注文のレートが現レートとあまり離れてないならスキップ
        let (has_near_rate_order, memo) = util::has_near_rate_order(
            info.get_sell_rate()?,
            &info.open_orders,
            self.config.entry_skip_rate_ratio,
        );
        if has_near_rate_order {
            info!("{} <= {}", "SKIP".red(), memo);
            should = false;
        } else {
            debug!("{}", format!("NOT SKIP <= {}", memo).blue());
        }

        // 直近の取引頻度が一定以上ならスキップ
        let (is_trade_frequency_enough, memo) = util::is_trade_frequency_enough(
            info.market_summary.trade_frequency_ratio,
            self.config.required_trade_frequency_ratio,
        );
        if is_trade_frequency_enough {
            debug!("{}", format!("NOT SKIP <= {}", memo,).blue());
        } else {
            info!("{} <= {}", "SKIP".red(), memo);
            should = false;
        }

        // 短期の売りと買いの出来高差が一定以上ならスキップ
        let mut short_volume_sell_total = 0.0;
        for (i, v) in info.sell_volumes.iter().rev().enumerate() {
            if i >= self.config.volume_period_short {
                break;
            }
            short_volume_sell_total += v;
        }
        let mut short_volume_buy_total = 0.0;
        for (i, v) in info.buy_volumes.iter().rev().enumerate() {
            if i >= self.config.volume_period_short {
                break;
            }
            short_volume_buy_total += v;
        }
        let (is_over_sell, memo) = util::is_over_sell(
            short_volume_sell_total,
            short_volume_buy_total,
            info.market_summary.ex_volume_sell_total,
            self.config.over_sell_volume_ratio,
        );

        if is_over_sell {
            info!("{} <= {}", "SKIP".red(), memo);
            should = false;
        } else {
            debug!("{}", format!("NOT SKIP <= {}", memo).blue());
        }

        // 目標レートまでの板の厚さが短期売り出来高未満ならスキップ
        let (is_board_heavy, memo) = util::is_board_heavy(
            util::estimate_sell_rate(
                info.buy_rate,
                buy_jpy_per_lot,
                self.config.profit_ratio_per_order,
            ),
            &info.order_books.asks,
            short_volume_sell_total,
            self.config.order_books_size_ratio,
        );

        if is_board_heavy {
            info!("{} <= {}", "SKIP".red(), memo);
            should = false;
        } else {
            debug!("{}", format!("NOT SKIP <= {}", memo).blue());
        }

        Ok(should)
    }

    // レジスタンスラインがブレイクアウトならエントリー
    fn check_resistance_line_breakout(
        &self,
        info: &TradeInfo,
        buy_jpy_per_lot: f64,
    ) -> MyResult<Option<ActionType>> {
        let sell_rate = info.get_sell_rate()?;

        // レジスタンスライン関連の情報
        let slope = util::calc_slope(&info.resistance_lines)?;
        let width_upper = sell_rate * self.config.resistance_line_width_ratio_upper;
        let width_lower = sell_rate * self.config.resistance_line_width_ratio_lower;
        let upper = info.resistance_lines.last().unwrap() + width_upper;
        let lower = info.resistance_lines.last().unwrap() + width_lower;

        // レジスタンスラインの傾きが負ならエントリーしない
        if slope < 0.0 {
            debug!(
                "{}",
                format!(
                    "NONE <= slope of resistance_line is negative, slope:{:.3} < 0.0",
                    slope
                )
                .blue()
            );
            return Ok(None);
        }

        // レジスタンスラインのすぐ上でリバウンドしてないならエントリーしない
        let (is_rebounded, memo) = util::is_rebounded(
            sell_rate,
            &info.sell_rate_histories,
            &info.resistance_lines,
            self.config.resistance_line_width_ratio_upper,
            self.config.resistance_line_width_ratio_lower,
            self.config.rebound_check_period,
        );
        if !is_rebounded {
            debug!(
                "{}",
                format!(
                    "NONE <= rates is not rebounded on the resistance line, {}",
                    memo,
                )
                .blue()
            );
            return Ok(None);
        }

        // 現レートがレジスタンスラインから離れすぎてるならエントリーしない
        if sell_rate < lower || sell_rate > upper {
            debug!(
                "{}",
                format!(
                    "NONE <= sell_rate:{:.3} is out of range:{:.3}...{:.3}",
                    sell_rate, lower, upper,
                )
            );
            return Ok(None);
        }

        // レート上昇中かチェック
        let before_rate = *info.sell_rate_histories.last().unwrap();
        if sell_rate <= before_rate {
            debug!(
                "{}",
                format!(
                    "NONE <= sell_rate is not rising, sell_rate:{:.3} <= before:{:.3}",
                    sell_rate, before_rate,
                )
            );
            return Ok(None);
        }

        info!(
            "{} <= resistance line breakout (roll reversal)",
            "ENTRY".green()
        );
        Ok(Some(ActionType::Entry(EntryParam {
            pair: Pair::new(&self.config.target_pair)?,
            amount: buy_jpy_per_lot,
            profit_ratio: self.config.profit_ratio_per_order,
            offset_sell_rate_ratio: self.config.offset_sell_rate_ratio,
        })))
    }

    // サポートラインがリバウンドしてるならエントリー
    fn check_support_line_rebound(
        &self,
        info: &TradeInfo,
        buy_jpy_per_lot: f64,
    ) -> MyResult<Option<ActionType>> {
        let sell_rate = info.get_sell_rate()?;

        // サポートラインすぐ上でリバウンドしていないならエントリーしない
        let (is_rebounded_long, memo_long) = util::is_rebounded(
            sell_rate,
            &info.sell_rate_histories,
            &info.support_lines_long,
            self.config.support_line_width_ratio_upper,
            self.config.support_line_width_ratio_lower,
            self.config.rebound_check_period,
        );
        let (is_rebounded_short, memo_short) = util::is_rebounded(
            sell_rate,
            &info.sell_rate_histories,
            &info.support_lines_short,
            self.config.support_line_width_ratio_upper,
            self.config.support_line_width_ratio_lower,
            self.config.rebound_check_period,
        );
        if !is_rebounded_long && !is_rebounded_short {
            debug!("{}", format!("NONE <= {}, {}", memo_long, memo_short),);
            return Ok(None);
        }

        // 現レートがサポートライン近くかをチェック
        let (on_support_line_long, memo_long) = util::is_on_line(
            sell_rate,
            &info.support_lines_long,
            self.config.support_line_width_ratio_upper,
            self.config.support_line_width_ratio_lower,
        );
        let (on_support_line_short, memo_short) = util::is_on_line(
            sell_rate,
            &info.support_lines_short,
            self.config.support_line_width_ratio_upper,
            self.config.support_line_width_ratio_lower,
        );
        if !on_support_line_long && !on_support_line_short {
            debug!("{}", format!("NONE <= {}, {}", memo_long, memo_short),);
            return Ok(None);
        }

        info!("{} <= rebounded on the support line", "ENTRY".green());
        Ok(Some(ActionType::Entry(EntryParam {
            pair: Pair::new(&self.config.target_pair)?,
            amount: buy_jpy_per_lot,
            profit_ratio: self.config.profit_ratio_per_order,
            offset_sell_rate_ratio: self.config.offset_sell_rate_ratio,
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::model::LossCutParam;
    use crate::bot::model::NotifyParam;
    use crate::coincheck::model::{Balance, OrderBooks, Pair};
    use crate::config::Config;
    use crate::mysql::model::MarketSummary;
    use crate::slack::client::TextMessage;
    use crate::strategy::scalping::ActionType::LossCut;
    use std::collections::HashMap;
    use std::mem;

    const COIN_KEY: &str = "btc";
    const COIN_SETTLEMENT: &str = "jpy";

    #[test]
    fn test_check_unused_coin() {
        struct Param {
            key_amount: f64,
            key_reserved: f64,
            now: String,
            want: Option<ActionType>,
        }
        let mut params = HashMap::new();
        params.insert(
            "when balance is too low",
            Param {
                key_amount: 0.9,
                key_reserved: 1.0,
                now: "2018-12-07T19:30:20+09:00".to_string(),
                want: None,
            },
        );
        params.insert(
            "when now is not notification time",
            Param {
                key_amount: 1.0,
                key_reserved: 0.0,
                now: "2018-12-07T19:31:28+09:00".to_string(),
                want: None,
            },
        );
        params.insert(
            "when balance is enough, now is notification time",
            Param {
                key_amount: 1.0,
                key_reserved: 0.0,
                now: "2018-12-07T19:30:20+09:00".to_string(),
                want: Some(ActionType::Notify(NotifyParam {
                    log_message: "".to_string(),
                    slack_message: TextMessage {
                        text: "".to_string(),
                    },
                })),
            },
        );

        for (name, p) in params.iter() {
            let config = make_config();
            let strategy = ScalpingStrategy { config: &config };
            let now = DateTime::parse_from_rfc3339(&p.now)
                .unwrap()
                .with_timezone(&Utc);
            let mut info = make_info();
            info.balances.insert(
                info.pair.key.clone(),
                Balance {
                    amount: p.key_amount,
                    reserved: p.key_reserved,
                },
            );

            let got = strategy.check_unused_coin(&now, &info);
            assert!(got.is_ok(), "{}, failure: want: ok, got: err", name);

            let got = got.unwrap();
            if let Some(want) = &p.want {
                assert!(got.is_some(), "{}, failure: want: some, got: none", name);
                assert_eq!(
                    mem::discriminant(&got.unwrap()),
                    mem::discriminant(want),
                    "{}, failure",
                    name
                );
            } else {
                assert!(
                    got.is_none(),
                    "{}, failure: want: none, got: {:?}",
                    name,
                    got
                );
            }
        }
    }

    #[test]
    fn test_check_loss_cut() {
        struct Param {
            sell_rate: f64,
            loss_cut_rate_ratio: f64,
            offset_sell_rate_ratio: f64,
            open_order: OpenOrder,
            want: Option<ActionType>,
        }
        let mut params = HashMap::new();
        params.insert(
            "sell_rate is border rate or greater",
            Param {
                sell_rate: 97.0,
                loss_cut_rate_ratio: 0.97,
                offset_sell_rate_ratio: 0.01,
                open_order: OpenOrder {
                    id: 0,
                    rate: 101.0,
                    pending_amount: 1.0,
                    pending_market_buy_amount: None,
                    order_type: OrderType::Sell,
                    pair: format!("{}_{}", COIN_KEY, COIN_SETTLEMENT),
                    created_at: DateTime::parse_from_rfc3339("2018-12-07T19:31:28+09:00").unwrap(),
                },
                want: None,
            },
        );
        params.insert(
            "sell_rate is less than border rate",
            Param {
                sell_rate: 96.9,
                loss_cut_rate_ratio: 0.97,
                offset_sell_rate_ratio: 0.01,
                open_order: OpenOrder {
                    id: 100,
                    rate: 101.0,
                    pending_amount: 1.0,
                    pending_market_buy_amount: None,
                    order_type: OrderType::Sell,
                    pair: format!("{}_{}", COIN_KEY, COIN_SETTLEMENT),
                    created_at: DateTime::parse_from_rfc3339("2018-12-07T19:31:28+09:00").unwrap(),
                },
                want: Some(LossCut(LossCutParam {
                    pair: Pair {
                        key: COIN_KEY.to_string(),
                        settlement: COIN_SETTLEMENT.to_string(),
                    },
                    open_order_id: 100,
                    amount: 1.0,
                })),
            },
        );

        for (name, p) in params.iter() {
            let mut config = make_config();
            config.loss_cut_rate_ratio = p.loss_cut_rate_ratio;
            config.offset_sell_rate_ratio = p.offset_sell_rate_ratio;

            let strategy = ScalpingStrategy { config: &config };
            let mut info = make_info();
            info.sell_rates
                .insert(format!("{}_{}", COIN_KEY, COIN_SETTLEMENT), p.sell_rate);

            let got = strategy.check_loss_cut(&info, &p.open_order);
            assert!(got.is_ok(), "{}, failure: want: ok, got: err", name);

            let got = got.unwrap();
            if let Some(want) = &p.want {
                assert!(got.is_some(), "{}, failure: want: some, got: none", name);
                assert_eq!(
                    mem::discriminant(&got.unwrap()),
                    mem::discriminant(want),
                    "{}, failure",
                    name
                );
            } else {
                assert!(
                    got.is_none(),
                    "{}, failure: want: none, got: {:?}",
                    name,
                    got
                );
            }
        }
    }

    fn make_config() -> Config {
        Config {
            bot_name: "dummy_bot_name".to_string(),
            target_pair: format!("{}_{}", COIN_KEY, COIN_SETTLEMENT),
            interval_sec: 0,
            rate_period_minutes: 0,
            external_service_wait_interval_sec: 0,
            demo_mode: false,
            wma_period_short: 5,
            wma_period_long: 10,
            resistance_line_period: 5,
            resistance_line_offset: 1,
            resistance_line_width_ratio_upper: 0.005,
            resistance_line_width_ratio_lower: 0.000,
            support_line_period_long: 5,
            support_line_period_short: 1,
            support_line_offset: 1,
            support_line_width_ratio_upper: 0.003,
            support_line_width_ratio_lower: 0.005,
            volume_period_short: 5,
            order_books_size_ratio: 5.0,
            rebound_check_period: 15,
            funds_ratio_per_order: 0.1,
            profit_ratio_per_order: 0.0015,
            offset_sell_rate_ratio: 0.01,
            hold_limit_minutes: 10,
            avg_down_rate_ratio: 0.97,
            avg_down_rate_ratio_on_holding_expired: 0.98,
            loss_cut_rate_ratio: 0.80,
            entry_skip_rate_ratio: 0.960,
            over_sell_volume_ratio: 0.022,
            required_trade_frequency_ratio: 0.2,
            keep_lot: 1.0,
            exchange_access_key: "dummy_access_key".to_string(),
            exchange_secret_key: "dummy_secret_key".to_string(),
            db_host: "dummy_db_host".to_string(),
            db_port: 100,
            db_name: "dummy_db_name".to_string(),
            db_user_name: "dummy_db_user_name".to_string(),
            db_password: "dummy_db_password".to_string(),
            slack_url: "dummy_slack_url".to_string(),
        }
    }

    fn make_info() -> TradeInfo {
        let mut sell_rates = HashMap::new();
        sell_rates.insert(
            format!("{}_{}", COIN_KEY, COIN_SETTLEMENT).to_string(),
            5000000.0,
        );

        let mut balances = HashMap::new();
        balances.insert(
            COIN_SETTLEMENT.to_string(),
            Balance {
                amount: 100000.0,
                reserved: 0.0,
            },
        );
        balances.insert(
            COIN_KEY.to_string(),
            Balance {
                amount: 0.0,
                reserved: 0.0,
            },
        );

        let recorded_at_begin = DateTime::parse_from_rfc3339("2018-12-07T19:31:28+09:00")
            .unwrap()
            .naive_utc();
        let recorded_at_end = DateTime::parse_from_rfc3339("2018-12-07T19:31:28+09:00")
            .unwrap()
            .naive_utc();

        let market_summary = MarketSummary {
            count: 0,
            recorded_at_begin: recorded_at_begin,
            recorded_at_end: recorded_at_end,
            ex_rate_sell_max: 0.0,
            ex_rate_sell_min: 0.0,
            ex_rate_buy_max: 0.0,
            ex_rate_buy_min: 0.0,
            ex_volume_sell_total: 0.0,
            ex_volume_buy_total: 0.0,
            trade_frequency_ratio: 0.0,
        };

        TradeInfo {
            pair: Pair {
                key: COIN_KEY.to_string(),
                settlement: COIN_SETTLEMENT.to_string(),
            },
            sell_rates: sell_rates,
            buy_rate: 5000000.0,
            balances: balances,
            open_orders: vec![],
            sell_rate_histories: vec![],
            sell_volumes: vec![],
            buy_volumes: vec![],
            support_lines_long: vec![],
            support_lines_short: vec![],
            resistance_lines: vec![],
            order_books: OrderBooks {
                asks: vec![],
                bids: vec![],
            },
            market_summary: market_summary,
        }
    }
}
