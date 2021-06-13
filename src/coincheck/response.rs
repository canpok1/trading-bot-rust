use super::model;
use crate::error::MyResult;

use std::collections::HashMap;

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
#[derive(Deserialize, Debug, PartialEq)]
pub struct OrdersPostResponse {
    pub success: bool,
    pub error: Option<String>,
    pub id: Option<u64>,
    pub rate: Option<String>,
    pub amount: Option<String>,
    pub market_buy_amount: Option<String>,
    pub order_type: Option<String>,
    pub stop_loss_rate: Option<String>,
    pub pair: Option<String>,
    pub created_at: Option<String>,
}

impl OrdersPostResponse {
    pub fn to_model(&self) -> MyResult<model::Order> {
        let id = self
            .id
            .ok_or_else(|| "id is nothing, this field is required")?;
        let rate = match &self.rate {
            Some(v) => Some(v.parse()?),
            None => None,
        };
        let amount = match &self.amount {
            Some(v) => Some(v.parse()?),
            None => None,
        };
        let order_type = model::OrderType::parse(
            &self
                .order_type
                .as_ref()
                .ok_or_else(|| "order_type is nothing, this field is required")?,
        )?;
        let pair = model::Pair::new(
            &self
                .pair
                .as_ref()
                .ok_or_else(|| "pair is nothing, this field is required")?,
        )?;
        let created_at = DateTime::parse_from_rfc3339(
            &self
                .created_at
                .as_ref()
                .ok_or_else(|| "created_at is nothing, this field is required")?,
        )?;

        Ok(model::Order {
            id: id,
            rate: rate,
            amount: amount,
            order_type: order_type,
            pair: pair,
            created_at: created_at,
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
    pub fn to_model(&self) -> MyResult<model::OpenOrder> {
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
    pub error: Option<String>,
    pub id: u64,
}

// 注文のキャンセルステータス
// GET /api/exchange/orders/cancel_status?id=[id]
#[derive(Deserialize, Debug)]
pub struct OrdersCancelStatusGetResponse {
    pub success: bool,
    pub error: Option<String>,
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
    pub fn to_map(&self) -> MyResult<HashMap<String, model::Balance>> {
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

#[cfg(test)]
mod tests {
    use crate::coincheck::response::OrdersPostResponse;

    #[test]
    fn test_deserialize_orders_post_response_1() {
        let body = "{\"success\":true,\"id\":3416677623,\"amount\":null,\"rate\":null,\"order_type\":\"market_buy\",\"pair\":\"mona_jpy\",\"created_at\":\"2021-05-16T12:38:07.000Z\",\"market_buy_amount\":\"1547.343\",\"stop_loss_rate\":null}".to_owned();
        let get = serde_json::from_str::<OrdersPostResponse>(&body).unwrap();
        let want = OrdersPostResponse {
            success: true,
            error: None,
            id: Some(3416677623),
            rate: None,
            amount: None,
            market_buy_amount: Some("1547.343".to_owned()),
            order_type: Some("market_buy".to_owned()),
            stop_loss_rate: None,
            pair: Some("mona_jpy".to_owned()),
            created_at: Some("2021-05-16T12:38:07.000Z".to_owned()),
        };
        assert_eq!(get, want);
    }

    #[test]
    fn test_deserialize_orders_post_response_2() {
        let body = "{\"success\":false,\"error\":\"Rate has invalid tick size\"}";
        let get = serde_json::from_str::<OrdersPostResponse>(body).unwrap();
        let want = OrdersPostResponse {
            success: false,
            error: Some("Rate has invalid tick size".to_owned()),
            id: None,
            rate: None,
            amount: None,
            market_buy_amount: None,
            order_type: None,
            stop_loss_rate: None,
            pair: None,
            created_at: None,
        };
        assert_eq!(get, want);
    }
}
