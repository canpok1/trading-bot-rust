use crate::error::MyError::RecordNotFound;
use crate::error::MyResult;
use crate::mysql::model::{BotStatus, Event, EventType, Market, Markets};

use chrono::DateTime;
use chrono::Utc;
use mysql::prelude::Queryable;
use mysql::OptsBuilder;
use mysql::Pool;
use mysql::PooledConn;

pub trait Client {
    fn select_markets(&self, pair: &str, begin: DateTime<Utc>) -> MyResult<Markets>;

    fn upsert_bot_status(&self, s: &BotStatus) -> MyResult<()>;

    fn select_bot_status(&self, bot_name: &str, r#type: &str) -> MyResult<BotStatus>;

    fn insert_event(&self, event: &Event) -> MyResult<()>;
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
            "INSERT INTO bot_statuses (bot_name, type, value, memo) VALUES ('{}', '{}', {}, '{}') ON DUPLICATE KEY UPDATE value = {};",
            s.bot_name, s.r#type, s.value, s.memo, s.value
        );
        conn.query_drop(sql)?;
        Ok(())
    }

    fn select_bot_status(&self, bot_name: &str, r#type: &str) -> MyResult<BotStatus> {
        let mut conn = self.get_conn()?;

        let sql = format!(
            "SELECT bot_name, type, value, memo FROM bot_statuses WHERE bot_name = '{}' AND type = '{}'",
            bot_name, r#type,
        );
        if let Some((bot_name, r#type, value, memo)) = conn.query_first(sql)? {
            Ok(BotStatus {
                bot_name: bot_name,
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
}
