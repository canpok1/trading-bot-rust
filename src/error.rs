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

    #[error("response is error, {}, request:{}", message, request)]
    ErrorResponse { message: String, request: String },
}

pub type MyResult<T> = Result<T, Box<dyn Error>>;
