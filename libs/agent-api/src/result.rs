use reqwest::Error as ReqwestError;
use thiserror::Error;
use url::ParseError as UrlParseError;

pub type Result<T> = std::result::Result<T, ClientError>;

#[derive(Error, Debug)]
pub enum ClientError {
    #[error("URL parsing error: {0}")]
    UrlParse(#[from] UrlParseError),

    #[error("Request error: {0}")]
    Reqwest(#[from] ReqwestError),
}
