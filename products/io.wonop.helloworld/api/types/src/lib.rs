use core::fmt;
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Validate, Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiResponse<T> {
    pub status: String,
    pub data: Option<T>,
}

#[derive(Validate, Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExampleApiType {
    pub payload: String,
}

impl ExampleApiType {
    pub fn new(payload: &str) -> Self {
        ExampleApiType {
            payload: payload.to_string(),
        }
    }
}

pub type ChatResponse = ApiResponse<ExampleApiType>;

#[derive(Validate, Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErrorResponse {
    pub status: String,
    pub message: String,
}

impl ErrorResponse {
    pub fn unauthorized() -> Self {
        Self {
            status: "fail".to_string(),
            message: "You are not logged in, please provide token".to_string(),
        }
    }

    pub fn insufficient_permissions() -> Self {
        Self {
            status: "fail".to_string(),
            message: "Insufficient permissions".to_string(),
        }
    }

    pub fn internal_error() -> Self {
        Self {
            status: "fail".to_string(),
            message: "Internal error".to_string(),
        }
    }
}

impl fmt::Display for ErrorResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", serde_json::to_string(&self).unwrap())
    }
}
