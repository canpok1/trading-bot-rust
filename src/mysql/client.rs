use crate::mysql::model::BotStatus;
use crate::mysql::model::{Market, Markets};
use chrono::DateTime;
use chrono::Utc;
use mysql::prelude::*;
use mysql::Pool;
use std::error::Error;

#[derive(Debug)]
pub struct Client {
    pool: Pool,
}

impl Client {
    pub fn new(
        user: &str,
        password: &str,
        host: &str,
        port: u64,
        database: &str,
    ) -> std::result::Result<Client, Box<dyn Error>> {
        let url = format!(
            "mysql://{user}:{password}@{host}:{port}/{database}",
            user = user,
            password = password,
            host = host,
            port = port,
            database = database,
        );

        Ok(Client {
            pool: Pool::new(url)?,
        })
    }

    pub fn select_markets(
        &self,
        pair: &str,
        begin: DateTime<Utc>,
    ) -> std::result::Result<Markets, Box<dyn Error>> {
        let mut conn = self.pool.get_conn()?;

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

    pub fn upsert_bot_status(&self, s: &BotStatus) -> Result<(), Box<dyn Error>> {
        let mut conn = self.pool.get_conn()?;
        let sql = format!(
            "INSERT INTO bot_statuses (bot_name, type, value, memo) VALUES ('{}', '{}', {}, '{}') ON DUPLICATE KEY UPDATE value = {};",
            s.bot_name, s.r#type, s.value, s.memo, s.value
        );
        conn.query_drop(sql)?;
        Ok(())
    }

    pub fn select_bot_status(
        &self,
        bot_name: &str,
        r#type: &str,
    ) -> Result<BotStatus, Box<dyn Error>> {
        let mut conn = self.pool.get_conn()?;

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
            Err(Box::new(crate::error::Error::RecordNotFound {
                table: "bot_statuses".to_owned(),
                param: format!("bot_name:{}, type:{}", bot_name, r#type),
            }))
        }
    }
}
