use super::model;

use std::collections::HashMap;
use std::error::Error;

use chrono::DateTime;
use serde::Deserialize;

// レート取得
// GET /api/exchange/orders/rate
#[derive(Deserialize, Debug)]
pub struct OrdersRateGetResponse {
    pub success: bool,
    pub error: Option<String>,
    pub rate: String,
    pub price: String,
    pub amount: String,
}

// 新規注文
// POST /api/exchange/orders
#[derive(Deserialize, Debug)]
pub struct OrdersPostResponse {
    pub id: u64,
    pub rate: String,
    pub amount: String,
    pub order_type: String,
    pub stop_loss_rate: Option<String>,
    pub pair: String,
    pub created_at: String,
}

impl OrdersPostResponse {
    pub fn to_model(&self) -> Result<model::Order, Box<dyn Error>> {
        Ok(model::Order {
            id: self.id,
            rate: self.rate.parse()?,
            amount: self.amount.parse()?,
            order_type: model::OrderType::parse(&self.order_type)?,
            pair: model::Pair::new(&self.pair)?,
            created_at: DateTime::parse_from_rfc3339(&self.created_at)?,
        })
    }
}

// 未決済の注文一覧
// GET /api/exchange/orders/opens
#[derive(Deserialize, Debug)]
pub struct OrdersOpensGetResponse {
    pub success: bool,
    pub error: Option<String>,
    pub orders: Vec<OpenOrder>,
}

#[derive(Deserialize, Debug)]
pub struct OpenOrder {
    pub id: u64,
    pub rate: String,
    pub pending_amount: String,
    pub pending_market_buy_amount: Option<String>,
    pub order_type: String,
    pub stop_loss_rate: Option<String>,
    pub pair: String,
    pub created_at: String,
}

impl OpenOrder {
    pub fn to_model(&self) -> Result<model::OpenOrder, Box<dyn Error>> {
        Ok(model::OpenOrder {
            id: self.id,
            rate: self.rate.parse()?,
            pending_amount: self.pending_amount.parse()?,
            pending_market_buy_amount: if let Some(amount) = &self.pending_market_buy_amount {
                Some(amount.parse()?)
            } else {
                None
            },
            order_type: model::OrderType::parse(&self.order_type)?,
            pair: self.pair.to_owned(),
            created_at: DateTime::parse_from_rfc3339(&self.created_at)?,
        })
    }
}

// 注文のキャンセル
// DELETE /api/exchange/orders/[id]
#[derive(Deserialize, Debug)]
pub struct OrdersDeleteResponse {
    pub success: bool,
    pub id: u64,
}

// 注文のキャンセルステータス
// GET /api/exchange/orders/cancel_status?id=[id]
#[derive(Deserialize, Debug)]
pub struct OrdersCancelStatusGetResponse {
    pub success: bool,
    pub id: u64,
    pub cancel: bool,
    pub created_at: String,
}

// 残高
// GET /api/accounts/balance
#[derive(Deserialize, Debug)]
pub struct BalanceGetResponse {
    pub success: bool,
    pub error: Option<String>,
    pub jpy: String,
    pub btc: String,
    pub etc: String,
    pub fct: String,
    pub mona: String,
    pub jpy_reserved: String,
    pub btc_reserved: String,
    pub etc_reserved: String,
    pub fct_reserved: String,
    pub mona_reserved: String,
}

impl BalanceGetResponse {
    pub fn to_map(&self) -> Result<HashMap<String, model::Balance>, Box<dyn Error>> {
        let mut map: HashMap<String, model::Balance> = HashMap::new();

        map.insert(
            "jpy".to_owned(),
            model::Balance {
                amount: self.jpy.parse()?,
                reserved: self.jpy_reserved.parse()?,
            },
        );

        map.insert(
            "btc".to_owned(),
            model::Balance {
                amount: self.btc.parse()?,
                reserved: self.btc_reserved.parse()?,
            },
        );

        map.insert(
            "etc".to_owned(),
            model::Balance {
                amount: self.etc.parse()?,
                reserved: self.etc_reserved.parse()?,
            },
        );

        map.insert(
            "fct".to_owned(),
            model::Balance {
                amount: self.fct.parse()?,
                reserved: self.fct_reserved.parse()?,
            },
        );

        map.insert(
            "mona".to_owned(),
            model::Balance {
                amount: self.mona.parse()?,
                reserved: self.mona_reserved.parse()?,
            },
        );

        Ok(map)
    }
}
