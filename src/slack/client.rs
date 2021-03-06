use crate::error::MyResult;

use async_trait::async_trait;
use log::debug;
use serde::Serialize;

#[async_trait]
pub trait Client {
    async fn post_message(&self, message: &TextMessage) -> MyResult<()>;
}

#[derive(Debug)]
pub struct DefaultClient {
    client: reqwest::Client,
    url: String,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct TextMessage {
    pub text: String,
}

impl DefaultClient {
    pub fn new(url: &str) -> MyResult<DefaultClient> {
        let client = reqwest::Client::builder().build()?;
        Ok(DefaultClient {
            client: client,
            url: url.to_owned(),
        })
    }
}

#[async_trait]
impl Client for DefaultClient {
    async fn post_message(&self, message: &TextMessage) -> MyResult<()> {
        let res = self
            .client
            .post(&self.url)
            .json(message)
            .send()
            .await?
            .text()
            .await?;
        debug!("post message response ... {}", res);
        Ok(())
    }
}
