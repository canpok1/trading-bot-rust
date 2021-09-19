use crate::bot::analyze::TradeInfo;
use crate::bot::model::ActionType;
use crate::bot::model::NotifyParam;
use crate::coincheck::model::{OpenOrder, OrderType};
use crate::error::MyResult;
use crate::slack::client::TextMessage;
use crate::strategy::base::Strategy;
use chrono::{DateTime, Timelike, Utc};
use colored::Colorize;
use log::debug;

pub struct ScalpingStrategy<'a> {
    pub config: &'a crate::config::Config,
}

impl Strategy for ScalpingStrategy<'_> {
    fn judge(&self, now: &DateTime<Utc>, info: &TradeInfo) -> MyResult<Vec<ActionType>> {
        let mut actions: Vec<ActionType> = Vec::new();

        debug!("========== check unused coin ==========");
        if let Some(action_type) = self.check_unused_coin(now, info)? {
            actions.push(action_type);
            return Ok(actions);
        }

        debug!("========== check loss cut or avd down ==========");
        let mut action_types = self.check_loss_cut_or_avg_down(now, info)?;
        if !action_types.is_empty() {
            actions.append(&mut action_types);
        }

        debug!("========== check entry ==========");
        let skip = self.check_entry_skip(info)?;
        if skip {
            return Ok(actions);
        }

        if let Some(action_type) = self.check_resistance_line_breakout(info)? {
            actions.push(action_type);
        } else if let Some(action_type) = self.check_support_line_rebound(info)? {
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
        _info: &TradeInfo,
        _open_order: &OpenOrder,
    ) -> MyResult<Option<ActionType>> {
        todo!();
    }

    // ナンピン？
    fn check_avg_down(
        &self,
        _now: &DateTime<Utc>,
        _info: &TradeInfo,
        _open_order: &OpenOrder,
    ) -> MyResult<Option<ActionType>> {
        todo!();
    }

    fn check_entry_skip(&self, _info: &TradeInfo) -> MyResult<bool> {
        todo!();
    }

    // レジスタンスラインがブレイクアウトならエントリー
    fn check_resistance_line_breakout(&self, _info: &TradeInfo) -> MyResult<Option<ActionType>> {
        todo!();
    }

    // サポートラインがリバウンドしてるならエントリー
    fn check_support_line_rebound(&self, _info: &TradeInfo) -> MyResult<Option<ActionType>> {
        todo!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::model::NotifyParam;
    use crate::coincheck::model::{Balance, OrderBooks, Pair};
    use crate::config::Config;
    use crate::mysql::model::MarketSummary;
    use crate::slack::client::TextMessage;
    use std::collections::HashMap;
    use std::mem;

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

    fn make_config() -> Config {
        Config {
            bot_name: "dummy_bot_name".to_string(),
            target_pair: "btc_jpy".to_string(),
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
            profit_ratio_per_order_on_down_trend: 0.0015,
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
        sell_rates.insert("btc_jpy".to_string(), 5000000.0);

        let mut balances = HashMap::new();
        balances.insert(
            "jpy".to_string(),
            Balance {
                amount: 100000.0,
                reserved: 0.0,
            },
        );
        balances.insert(
            "btc".to_string(),
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
                key: "btc".to_string(),
                settlement: "jpy".to_string(),
            },
            sell_rates: sell_rates,
            buy_rate: 5000000.0,
            balances: balances,
            open_orders: vec![],
            rate_histories: vec![],
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
