use serde::Deserialize;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("failed to parse from `{0}`")]
    ParseError(String),
}

#[derive(Deserialize, Debug)]
pub enum OrderType {
    Sell,
    Buy,
}
