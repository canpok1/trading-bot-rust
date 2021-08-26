use crate::error::MyError::RecordNotFound;
use crate::error::MyResult;
use crate::mysql::model::MarketSummary;
use crate::mysql::model::{BotStatus, Event, EventType, Market, Markets};

use chrono::DateTime;
use chrono::Utc;
use indoc::indoc;
use mysql::prelude::Queryable;
use mysql::OptsBuilder;
use mysql::Pool;
use mysql::PooledConn;

pub trait Client {
    fn select_markets(&self, pair: &str, begin: DateTime<Utc>) -> MyResult<Markets>;

    fn upsert_bot_status(&self, s: &BotStatus) -> MyResult<()>;

    fn select_bot_status(&self, bot_name: &str, pair: &str, r#type: &str) -> MyResult<BotStatus>;

    fn insert_event(&self, event: &Event) -> MyResult<()>;

    fn select_market_summary(&self, pair: &str, offset_hour: u64) -> MyResult<MarketSummary>;
}

#[derive(Debug)]
pub struct DefaultClient {
    pool: Pool,
}

impl DefaultClient {
    pub fn new(
        user: &str,
        password: &str,
        host: &str,
        port: u16,
        database: &str,
    ) -> MyResult<DefaultClient> {
        let opts = OptsBuilder::new()
            .user(Some(user))
            .pass(Some(password))
            .ip_or_hostname(Some(host))
            .tcp_port(port)
            .db_name(Some(database));

        Ok(DefaultClient {
            pool: Pool::new(opts)?,
        })
    }

    fn get_conn(&self) -> MyResult<PooledConn> {
        match self.pool.get_conn() {
            Ok(v) => Ok(v),
            Err(e) => Err(Box::new(e)),
        }
    }
}

impl Client for DefaultClient {
    fn select_markets(&self, pair: &str, begin: DateTime<Utc>) -> MyResult<Markets> {
        let mut conn = self.get_conn()?;

        let sql = format!(
            "SELECT pair, store_rate_avg, ex_rate_sell, ex_rate_buy, ex_volume_sell, ex_volume_buy, recorded_at FROM markets WHERE pair = '{}' AND recorded_at > '{}' ORDER BY recorded_at",
            pair,
            begin.format("%Y-%m-%d %H:%M:%S"),
        );
        let markets = conn.query_map(
            sql,
            |(
                pair,
                store_rate_avg,
                ex_rate_sell,
                ex_rate_buy,
                ex_volume_sell,
                ex_volume_buy,
                recorded_at,
            )| {
                Market {
                    pair: pair,
                    store_rate_avg: store_rate_avg,
                    ex_rate_sell: ex_rate_sell,
                    ex_rate_buy: ex_rate_buy,
                    ex_volume_sell: ex_volume_sell,
                    ex_volume_buy: ex_volume_buy,
                    recorded_at: recorded_at,
                }
            },
        )?;
        Ok(markets)
    }

    fn upsert_bot_status(&self, s: &BotStatus) -> MyResult<()> {
        let mut conn = self.get_conn()?;
        let sql = format!(
                "INSERT INTO bot_statuses (bot_name, pair, type, value, memo) VALUES ('{}', '{}', '{}', {}, '{}') ON DUPLICATE KEY UPDATE value = {};",
                s.bot_name, s.pair, s.r#type, s.value, s.memo, s.value
        );

        conn.query_drop(sql)?;
        Ok(())
    }

    fn select_bot_status(&self, bot_name: &str, pair: &str, r#type: &str) -> MyResult<BotStatus> {
        let mut conn = self.get_conn()?;

        let sql = format!(
                "SELECT bot_name, pair, type, value, memo FROM bot_statuses WHERE bot_name = '{}' AND pair = '{}' AND type = '{}'",
                bot_name, pair, r#type,
            );
        if let Some((bot_name, pair, r#type, value, memo)) = conn.query_first(sql)? {
            Ok(BotStatus {
                bot_name: bot_name,
                pair: pair,
                r#type: r#type,
                value: value,
                memo: memo,
            })
        } else {
            Err(Box::new(RecordNotFound {
                table: "bot_statuses".to_owned(),
                param: format!("bot_name:{}, type:{}", bot_name, r#type),
            }))
        }
    }

    fn insert_event(&self, event: &Event) -> MyResult<()> {
        let mut conn = self.get_conn()?;
        let event_type = match event.event_type {
            EventType::Buy => 0,
            EventType::Sell => 1,
        };
        let sql = format!(
            "INSERT INTO events (pair, event_type, memo, recorded_at) VALUES ('{}', {}, '{}', '{}');",
            event.pair.to_string(), event_type, event.memo, event.recorded_at.format("%Y-%m-%d %H:%M:%S"),
        );
        conn.query_drop(sql)?;
        Ok(())
    }

    fn select_market_summary(&self, pair: &str, offset_hour: u64) -> MyResult<MarketSummary> {
        let mut conn = self.get_conn()?;

        let sql = format!(
            indoc!(
                "
                SELECT
                    COUNT(1) count,
                    MIN(m.recorded_at) recorded_at_begin,
                    MAX(m.recorded_at) recorded_at_end,
                    MAX(m.ex_rate_sell) ex_rate_sell_max,
                    MIN(m.ex_rate_sell) ex_rate_sell_min,
                    MAX(m.ex_rate_buy) ex_rate_buy_max,
                    MIN(m.ex_rate_buy) ex_rate_buy_min,
                    SUM(m.ex_volume_sell) ex_volume_sell_total,
                    SUM(m.ex_volume_buy) ex_volume_buy_total
                FROM markets m 
                WHERE m.pair = '{}'
                    AND m.recorded_at <= DATE_SUB(NOW(), INTERVAL {} HOUR)
                    AND m.recorded_at >= DATE_SUB(NOW(), INTERVAL 24 + {} HOUR)
            "
            ),
            pair, offset_hour, offset_hour
        );
        if let Some((
            count,
            recorded_at_begin,
            recorded_at_end,
            ex_rate_sell_max,
            ex_rate_sell_min,
            ex_rate_buy_max,
            ex_rate_buy_min,
            ex_volume_sell_total,
            ex_volume_buy_total,
        )) = conn.query_first(sql)?
        {
            if count > 0 {
                return Ok(MarketSummary {
                    count: count,
                    recorded_at_begin: recorded_at_begin,
                    recorded_at_end: recorded_at_end,
                    ex_rate_sell_max: ex_rate_sell_max,
                    ex_rate_sell_min: ex_rate_sell_min,
                    ex_rate_buy_max: ex_rate_buy_max,
                    ex_rate_buy_min: ex_rate_buy_min,
                    ex_volume_sell_total: ex_volume_sell_total,
                    ex_volume_buy_total: ex_volume_buy_total,
                });
            }
        }
        Err(Box::new(RecordNotFound {
            table: "markets".to_owned(),
            param: format!("pair:{}, offset_hour:{}", pair, offset_hour),
        }))
    }
}
