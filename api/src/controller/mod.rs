use serde::{Deserialize, Serialize};
use std::fmt;
use std::fmt::Formatter;
use volo_http::http::StatusCode;
use volo_http::response::Response;
use volo_http::server::extract::Json;
use volo_http::server::IntoResponse;

pub mod order_controller;
pub mod random_controller;
pub mod user_controller;

/// 业务响应数据 `R`
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(bound(serialize = "T: Serialize", deserialize = "T: Deserialize<'de>"))]
pub struct R<T = serde_json::Value> {
    pub code: i64,
    pub msg: Option<String>,
    pub data: Option<T>,
}

impl<T> R<T> {
    pub fn ok(data: T) -> Self {
        Self {
            code: 200,
            msg: Some("success".to_string()),
            data: Some(data),
        }
    }

    pub fn error(code: i64, msg: &str) -> R<T> {
        R {
            code,
            msg: Some(msg.to_string()),
            data: None,
        }
    }

    pub fn error_status_code(status_code: StatusCode, msg: &str) -> R<T> {
        R {
            code: status_code.as_u16() as i64,
            msg: Some(msg.to_string()),
            data: None,
        }
    }

    pub fn server_error(msg: &str) -> R<T> {
        R {
            code: 500,
            msg: Some(msg.to_string()),
            data: None,
        }
    }

    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> R<U> {
        R {
            code: self.code,
            msg: self.msg,
            data: self.data.map(f),
        }
    }
}

impl<T> From<anyhow::Error> for R<T> {
    fn from(e: anyhow::Error) -> Self {
        R {
            code: 500,
            msg: Some(e.to_string()),
            data: None,
        }
    }
}
impl<T> From<anyhow::Result<T>> for R<T> {
    fn from(r: anyhow::Result<T>) -> Self {
        match r {
            Ok(data) => Self::ok(data),
            Err(err) => Self::error(500, &err.to_string()),
        }
    }
}

impl<T: fmt::Debug> fmt::Display for R<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "R{{ code: {}, msg: {:?}, data: {:?} }}",
            self.code, self.msg, self.data
        )
    }
}

impl<T: Serialize> IntoResponse for R<T> {
    fn into_response(self) -> Response {
        let status = match self.code {
            200..=299 => StatusCode::OK,
            400..=499 => StatusCode::BAD_REQUEST,
            500..=599 => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::OK,
        };

        let body = Json(self);
        (status, body).into_response()
    }
}
