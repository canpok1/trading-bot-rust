use crate::coincheck::model::{Balance, NewOrder, OpenOrder, Order, OrderBooks, OrderType};
use crate::coincheck::request::OrdersPostRequest;
use crate::coincheck::response::OrdersCancelStatusGetResponse;
use crate::coincheck::response::OrdersDeleteResponse;
use crate::coincheck::response::*;
use crate::error::MyError::{ErrorResponse, ParseError};
use crate::error::MyResult;

use std::collections::HashMap;
use std::time::SystemTime;

use async_trait::async_trait;
use openssl::hash::MessageDigest;
use openssl::pkey::PKey;
use openssl::sign::Signer;
use serde::de::DeserializeOwned;
use serde::Serialize;

const BASE_URL: &str = "https://coincheck.com";

#[async_trait]
pub trait Client {
    async fn get_order_books(&self, pair: &str) -> MyResult<OrderBooks>;

    async fn get_exchange_orders_rate(&self, t: OrderType, pair: &str) -> MyResult<f64>;

    async fn post_exchange_orders(&self, req: &NewOrder) -> MyResult<Order>;

    async fn get_exchange_orders_opens(&self) -> MyResult<Vec<OpenOrder>>;

    async fn delete_exchange_orders(&self, id: u64) -> MyResult<u64>;

    async fn get_exchange_orders_cancel_status(&self, id: u64) -> MyResult<bool>;

    async fn get_accounts_balance(&self) -> MyResult<HashMap<String, Balance>>;
}

#[derive(Debug)]
pub struct DefaultClient {
    client: reqwest::Client,
    access_key: String,
    secret_key: String,
}

#[async_trait]
impl Client for DefaultClient {
    async fn get_order_books(&self, pair: &str) -> MyResult<OrderBooks> {
        let url = format!("{}{}", BASE_URL, "/api/order_books");
        let params = [("pair", pair)];
        let body = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await?
            .json::<OrdersBooksGetResponse>()
            .await?;
        body.to_model()
    }

    async fn get_exchange_orders_rate(&self, t: OrderType, pair: &str) -> MyResult<f64> {
        let url = format!("{}{}", BASE_URL, "/api/exchange/orders/rate");
        let params = [("order_type", t.to_str()), ("pair", pair), ("amount", "1")];
        let body = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await?
            .json::<OrdersRateGetResponse>()
            .await?;
        let rate = body.rate.parse::<f64>()?;
        Ok(rate)
    }

    async fn post_exchange_orders(&self, req: &NewOrder) -> MyResult<Order> {
        let url = format!("{}{}", BASE_URL, "/api/exchange/orders");
        let req_body = OrdersPostRequest::new(req)?;

        let res = self
            .post_request_with_auth::<OrdersPostRequest, OrdersPostResponse>(&url, req_body)
            .await?;
        if res.success {
            Ok(res.to_model()?)
        } else {
            Err(Box::new(ErrorResponse {
                message: res.error.unwrap(),
                request: format!("{:?}", *req),
            }))
        }
    }

    async fn get_exchange_orders_opens(&self) -> MyResult<Vec<OpenOrder>> {
        let url = format!("{}{}", BASE_URL, "/api/exchange/orders/opens");
        let body = self
            .get_request_with_auth::<OrdersOpensGetResponse>(&url)
            .await?;
        let mut res: Vec<OpenOrder> = Vec::new();
        for o in body.orders {
            res.push(o.to_model()?);
        }

        Ok(res)
    }

    async fn delete_exchange_orders(&self, id: u64) -> MyResult<u64> {
        let url = format!("{}{}{}", BASE_URL, "/api/exchange/orders/", id);
        let body = self
            .delete_request_with_auth::<OrdersDeleteResponse>(&url)
            .await?;
        Ok(body.id)
    }

    async fn get_exchange_orders_cancel_status(&self, id: u64) -> MyResult<bool> {
        let url: String = format!(
            "{}{}{}",
            BASE_URL, "/api/exchange/orders/cancel_status?id=", id
        );
        let body = self
            .get_request_with_auth::<OrdersCancelStatusGetResponse>(&url)
            .await?;
        Ok(body.cancel)
    }

    async fn get_accounts_balance(&self) -> MyResult<HashMap<String, Balance>> {
        let url: String = format!("{}{}", BASE_URL, "/api/accounts/balance");
        let body = self
            .get_request_with_auth::<BalanceGetResponse>(&url)
            .await?;
        Ok(body.to_map()?)
    }
}

impl DefaultClient {
    pub fn new(access_key: &str, secret_key: &str) -> MyResult<DefaultClient> {
        let client = reqwest::Client::builder().build()?;
        Ok(DefaultClient {
            client: client,
            access_key: access_key.to_string(),
            secret_key: secret_key.to_string(),
        })
    }

    async fn get_request_with_auth<T: DeserializeOwned>(&self, url: &str) -> MyResult<T> {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_millis();
        let signature = make_signature(nonce, &url, "", &self.secret_key);

        let res_text = self
            .client
            .get(url)
            .header("ACCESS-KEY", &self.access_key)
            .header("ACCESS-NONCE", format!("{}", nonce))
            .header("ACCESS-SIGNATURE", signature)
            .send()
            .await?
            .text()
            .await?;

        match serde_json::from_str::<T>(&res_text) {
            Ok(res) => Ok(res),
            Err(_) => Err(Box::new(ParseError(res_text))),
        }
    }

    async fn post_request_with_auth<T, U>(&self, url: &str, body: T) -> MyResult<U>
    where
        T: Serialize,
        U: DeserializeOwned,
    {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_millis();
        let json = serde_json::to_string(&body)?;
        let signature = make_signature(nonce, &url, &json, &self.secret_key);

        let res_text = self
            .client
            .post(url)
            .header("Content-Type", "application/json")
            .header("ACCESS-KEY", &self.access_key)
            .header("ACCESS-NONCE", format!("{}", nonce))
            .header("ACCESS-SIGNATURE", signature)
            .body(json)
            .send()
            .await?
            .text()
            .await?;

        match serde_json::from_str::<U>(&res_text) {
            Ok(res) => Ok(res),
            Err(_) => Err(Box::new(ParseError(res_text))),
        }
    }

    async fn delete_request_with_auth<T: DeserializeOwned>(&self, url: &str) -> MyResult<T> {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_millis();
        let signature = make_signature(nonce, &url, "", &self.secret_key);

        let res_text = self
            .client
            .delete(url)
            .header("ACCESS-KEY", &self.access_key)
            .header("ACCESS-NONCE", format!("{}", nonce))
            .header("ACCESS-SIGNATURE", signature)
            .send()
            .await?
            .text()
            .await?;

        match serde_json::from_str::<T>(&res_text) {
            Ok(res) => Ok(res),
            Err(_) => Err(Box::new(ParseError(res_text))),
        }
    }
}

fn make_signature(nonce: u128, url: &str, body: &str, secret_key: &str) -> String {
    let key = PKey::hmac(secret_key.as_bytes()).unwrap();
    let mut signer = Signer::new(MessageDigest::sha256(), &key).unwrap();
    let v = format!("{}{}{}", nonce, url, body);
    signer.update(&v.as_bytes()).unwrap();
    let bb = signer.sign_to_vec().unwrap();
    bb.iter()
        .fold("".to_owned(), |s, b| format!("{}{:02x}", s, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_signature() {
        assert_eq!(
            make_signature(12345, "https://example.com", "hoge=foo", "abcdefg"),
            "65a5d4bf76d4266e2f56582c31ca3e9ac163c80745e84357ead5a2899a37e218"
        );
    }
}
