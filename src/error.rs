use std::error::Error;

#[derive(thiserror::Error, Debug)]
pub enum MyError {
    #[error("failed to parse from {0}")]
    ParseError(String),

    #[error("record not found in {} [{}]", table, param)]
    RecordNotFound { table: String, param: String },

    #[error("{} is too short, len:{} < required:{}", name, len, required)]
    TooShort {
        name: String,
        len: usize,
        required: usize,
    },

    #[error("response is error, {}, url:{}, request:{}", message, url, request)]
    ResponseError {
        message: String,
        url: String,
        request: String,
    },

    #[error("{} not found in {}", key, collection_name)]
    KeyNotFound {
        key: String,
        collection_name: String,
    },

    #[error("{0} is empty")]
    EmptyCollection(String),
}

pub type MyResult<T> = Result<T, Box<dyn Error>>;
