use crate::coincheck::model;
use crate::error::MyResult;
use crate::util::to_request_string;
use serde::Serialize;

// 新規注文
// POST /api/exchange/orders
#[derive(Serialize, Debug)]
pub struct OrdersPostRequest {
    pub pair: String,
    pub order_type: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub amount: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub market_buy_amount: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_loss_rate: Option<String>,
}

impl OrdersPostRequest {
    pub fn new(order: &model::NewOrder) -> MyResult<OrdersPostRequest> {
        let rate = if let Some(v) = order.rate {
            Some(to_request_string(v))
        } else {
            None
        };
        let amount = if let Some(v) = order.amount {
            Some(to_request_string(v))
        } else {
            None
        };
        let market_buy_amount = if let Some(v) = order.market_buy_amount {
            Some(to_request_string(v))
        } else {
            None
        };

        Ok(OrdersPostRequest {
            pair: order.pair.to_string(),
            order_type: order.order_type.to_str().to_owned(),
            rate: rate,
            amount: amount,
            market_buy_amount: market_buy_amount,
            stop_loss_rate: None,
        })
    }
}
