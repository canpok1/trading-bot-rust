use serde::Deserialize;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("failed to parse from {0}")]
    ParseError(String),

    #[error("record not found in {} [{}]", table, param)]
    RecordNotFound { table: String, param: String },

    #[error("too short, len:{} < required:{}", len, required)]
    TooShort { len: usize, required: usize },
}

#[derive(Deserialize, Debug)]
pub enum OrderType {
    Sell,
    Buy,
}
