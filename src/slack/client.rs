use std::error::Error;

use log::debug;
use serde::Serialize;

#[derive(Debug)]
pub struct Client {
    client: reqwest::Client,
    url: String,
}

#[derive(Serialize, Debug)]
pub struct TextMessage {
    pub text: String,
}

impl Client {
    pub fn new(url: &str) -> Result<Client, Box<dyn Error>> {
        let client = reqwest::Client::builder().build()?;
        Ok(Client {
            client: client,
            url: url.to_owned(),
        })
    }

    pub async fn post_message(&self, message: &TextMessage) -> Result<(), Box<dyn Error>> {
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
