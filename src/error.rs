use serde::{de, ser};
use std::fmt::Display;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Custom Error: {0}")]
    Custom(String),
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid UTF-8: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error("Format Error: {0}")]
    Fmt(#[from] std::fmt::Error),
    #[error("Invalid RTON Header")]
    InvalidHeader,
    #[error("Reference index out of bounds")]
    RefIndexOutOfBounds,
    #[error("RTON arrays require a known length in advance")]
    UnknownLength,
    #[error("Unknown Identifier Byte: {0:#04x}")]
    UnknownTag(u8),
    #[error("Unknown RTID sub-identifier: {0:#04x}")]
    UnknownRtidSubId(u8),
    #[error("Expected array end marker 0xfe")]
    ArrayEndMismatch,
    #[error("Expected array start marker 0xfd")]
    ArrayStartMismatch,
    #[error("Unexpected marker tag in value position: {0}")]
    UnexpectedMarker(String),
    #[error("Encountered unsupported extended tag: {0}")]
    UnsupportedExtendedTag(String),
}

impl ser::Error for Error {
    fn custom<T: Display>(msg: T) -> Self {
        Error::Custom(msg.to_string())
    }
}

impl de::Error for Error {
    fn custom<T: Display>(msg: T) -> Self {
        Error::Custom(msg.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
