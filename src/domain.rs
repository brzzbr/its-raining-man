use std::fmt;
use std::fmt::Formatter;
use std::future::Future;
use std::pin::Pin;

use serde::Deserialize;

pub type Async<T> = Pin<Box<dyn Future<Output = T> + Send>>;

pub type AsyncUnit = Async<()>;

#[derive(Debug, Clone, Copy)]
pub struct Location(f64, f64);

impl Location {
    pub fn new(lat: f64, lon: f64) -> Self {
        Self(lat, lon)
    }

    pub fn lat(&self) -> f64 {
        self.0
    }

    pub fn lon(&self) -> f64 {
        self.1
    }
}

#[derive(Deserialize, Debug)]
pub struct AlertResponse {
    #[serde(rename = "type")]
    pub typ: String,
    pub title: String,
}

#[derive(Deserialize, Debug)]
pub struct WeatherResponse {
    pub alert: AlertResponse,
}

#[derive(Debug)]
pub enum CheckError {
    UrlParser(String),
    Request(String),
    Bot(String),
}

impl From<url::ParseError> for CheckError {
    fn from(value: url::ParseError) -> Self {
        CheckError::UrlParser(value.to_string())
    }
}

impl From<reqwest::Error> for CheckError {
    fn from(value: reqwest::Error) -> Self {
        CheckError::Request(value.to_string())
    }
}

impl From<teloxide::errors::RequestError> for CheckError {
    fn from(value: teloxide::errors::RequestError) -> Self {
        CheckError::Bot(value.to_string())
    }
}

impl From<fantoccini::error::NewSessionError> for CheckError {
    fn from(value: fantoccini::error::NewSessionError) -> Self {
        CheckError::Request(value.to_string())
    }
}

impl From<fantoccini::error::CmdError> for CheckError {
    fn from(value: fantoccini::error::CmdError) -> Self {
        CheckError::Request(value.to_string())
    }
}

impl fmt::Display for CheckError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            CheckError::UrlParser(err) => {
                write!(f, "Url parsing failed: {}", err)
            }
            CheckError::Request(err) => {
                write!(f, "Error requesting weather: {}", err)
            }
            CheckError::Bot(err) => {
                write!(f, "Bot error: {}", err)
            }
        }
    }
}
