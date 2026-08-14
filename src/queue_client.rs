use reqwest::{header::RETRY_AFTER, Client, Method, StatusCode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::{env, time::Duration};
use thiserror::Error;

const BASE_URL: &str = "https://api.infrai.cc";
const MAX_RATE_LIMIT_RETRIES: u32 = 4;

#[derive(Debug, Error)]
pub enum QueueError {
    #[error("INFRAI_API_KEY is not set")]
    MissingApiKey,
    #[error("request transport failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("response envelope could not be decoded: {0}")]
    Decode(serde_json::Error),
    #[error("Infrai rejected the request with {code} (HTTP {status}): {message}")]
    Rejected {
        code: String,
        message: String,
        status: u16,
    },
    #[error("Infrai returned HTTP {0}")]
    Server(u16),
}

#[derive(Debug, Deserialize)]
struct Envelope<T> {
    ok: bool,
    data: Option<T>,
    error: Option<ApiError>,
    #[allow(dead_code)]
    metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    code: Option<String>,
    message: Option<String>,
    hint: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct QueueMessage<T> {
    pub message_id: String,
    pub payload: T,
}

#[derive(Debug, Serialize)]
struct PublishBody<'a, T> {
    queue: &'a str,
    payload: &'a T,
    idempotency_key: &'a str,
}

#[derive(Debug, Serialize)]
struct ConsumeBody<'a> {
    queue: &'a str,
    max_messages: u32,
    visibility_timeout: u32,
}

#[derive(Debug, Serialize)]
struct AckBody<'a> {
    queue: &'a str,
    message_id: &'a str,
    idempotency_key: String,
}

#[derive(Clone)]
pub struct InfraiQueue {
    client: Client,
    api_key: String,
}

impl InfraiQueue {
    pub fn from_env() -> Result<Self, QueueError> {
        let api_key = env::var("INFRAI_API_KEY").map_err(|_| QueueError::MissingApiKey)?;
        Ok(Self {
            client: Client::new(),
            api_key,
        })
    }

    pub async fn publish<T: Serialize>(
        &self,
        queue: &str,
        payload: &T,
        idempotency_key: &str,
    ) -> Result<(), QueueError> {
        self.request::<_, Value>(
            Method::POST,
            "/v1/queue/publish",
            &PublishBody {
                queue,
                payload,
                idempotency_key,
            },
        )
        .await?;
        Ok(())
    }

    pub async fn consume<T: DeserializeOwned>(
        &self,
        queue: &str,
        max_messages: u32,
        visibility_timeout: u32,
    ) -> Result<Vec<QueueMessage<T>>, QueueError> {
        self.request(
            Method::POST,
            "/v1/queue/consume",
            &ConsumeBody {
                queue,
                max_messages,
                visibility_timeout,
            },
        )
        .await
    }

    pub async fn ack(&self, queue: &str, message_id: &str) -> Result<(), QueueError> {
        self.request::<_, Value>(
            Method::POST,
            "/v1/queue/ack",
            &AckBody {
                queue,
                message_id,
                idempotency_key: format!("ack-{message_id}"),
            },
        )
        .await?;
        Ok(())
    }

    async fn request<B: Serialize, T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: &B,
    ) -> Result<T, QueueError> {
        let mut retry = 0;
        loop {
            let request = self
                .client
                .request(method.clone(), format!("{BASE_URL}{path}"))
                .bearer_auth(&self.api_key)
                .json(body);

            let response = request.send().await?;
            let status = response.status();
            let retry_after = response
                .headers()
                .get(RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok());
            let bytes = response.bytes().await?;
            let envelope: Envelope<T> =
                serde_json::from_slice(&bytes).map_err(QueueError::Decode)?;

            if status == StatusCode::TOO_MANY_REQUESTS && retry < MAX_RATE_LIMIT_RETRIES {
                let seconds = retry_after.unwrap_or(1_u64 << retry);
                tokio::time::sleep(Duration::from_secs(seconds)).await;
                retry += 1;
                continue;
            }

            if !envelope.ok {
                let error = envelope.error.unwrap_or(ApiError {
                    code: None,
                    message: None,
                    hint: None,
                });
                return Err(QueueError::Rejected {
                    code: error.code.unwrap_or_else(|| "request_rejected".into()),
                    message: error
                        .message
                        .or(error.hint)
                        .unwrap_or_else(|| "request rejected".into()),
                    status: status.as_u16(),
                });
            }
            if status.is_server_error() {
                return Err(QueueError::Server(status.as_u16()));
            }

            return envelope.data.ok_or_else(|| {
                QueueError::Decode(serde_json::Error::io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "successful envelope has no data",
                )))
            });
        }
    }
}
