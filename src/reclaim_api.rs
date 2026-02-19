use crate::error::CliError;
use reqwest::{header, Client, Method, RequestBuilder, Response, Url};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{collections::HashMap, time::Duration};

const USER_AGENT: &str = concat!("reclaim-cli/", env!("CARGO_PKG_VERSION"));

pub trait ReclaimApi {
    async fn list_tasks(&self, filter: TaskFilter) -> Result<Vec<Task>, CliError>;
    async fn get_task(&self, task_id: u64) -> Result<Task, CliError>;
    async fn create_task(&self, request: CreateTaskRequest) -> Result<Task, CliError>;
}

#[derive(Debug, Clone, Copy)]
pub enum TaskFilter {
    Active,
    All,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskRequest {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_chunks_required: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: u64,
    pub title: String,
    pub status: Option<String>,
    pub due: Option<String>,
    pub priority: Option<String>,
    pub notes: Option<String>,
    #[serde(default)]
    pub deleted: bool,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

pub struct HttpReclaimApi {
    client: Client,
    base_url: Url,
    api_key: String,
}

impl HttpReclaimApi {
    pub fn new(
        api_key: Option<String>,
        base_url: String,
        timeout_secs: u64,
    ) -> Result<Self, CliError> {
        let api_key = api_key
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or(CliError::MissingApiKey)?;

        let base_url = normalize_base_url(&base_url)?;
        let timeout_secs = timeout_secs.max(1);

        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .user_agent(USER_AGENT)
            .build()
            .map_err(|error| CliError::Transport {
                message: format!("Could not create HTTP client: {error}"),
                hint: Some("Check your runtime environment and try again.".to_string()),
            })?;

        Ok(Self {
            client,
            base_url,
            api_key,
        })
    }

    fn request(&self, method: Method, path: &str) -> RequestBuilder {
        let url = self
            .base_url
            .join(path.trim_start_matches('/'))
            .expect("valid base URL should always allow relative joins");

        self.client
            .request(method, url)
            .bearer_auth(&self.api_key)
            .header(header::ACCEPT, "application/json")
            .header(header::CONTENT_TYPE, "application/json")
    }

    async fn send_json<T: DeserializeOwned>(&self, request: RequestBuilder) -> Result<T, CliError> {
        let response = request.send().await.map_err(map_transport_error)?;
        if response.status().is_success() {
            response
                .json::<T>()
                .await
                .map_err(|error| CliError::ResponseParse {
                    message: format!("Reclaim API returned an unexpected response: {error}"),
                    hint: Some(
                        "Retry the command. If it keeps failing, inspect the payload with --format json."
                            .to_string(),
                    ),
                })
        } else {
            Err(parse_api_error(response).await)
        }
    }
}

impl ReclaimApi for HttpReclaimApi {
    async fn list_tasks(&self, filter: TaskFilter) -> Result<Vec<Task>, CliError> {
        let mut tasks: Vec<Task> = self.send_json(self.request(Method::GET, "tasks")).await?;

        if matches!(filter, TaskFilter::Active) {
            tasks.retain(is_active_task);
        }

        Ok(tasks)
    }

    async fn get_task(&self, task_id: u64) -> Result<Task, CliError> {
        self.send_json(self.request(Method::GET, &format!("tasks/{task_id}")))
            .await
    }

    async fn create_task(&self, request: CreateTaskRequest) -> Result<Task, CliError> {
        self.send_json(self.request(Method::POST, "tasks").json(&request))
            .await
    }
}

fn normalize_base_url(raw: &str) -> Result<Url, CliError> {
    let mut url = Url::parse(raw).map_err(|_| CliError::InvalidBaseUrl(raw.to_string()))?;

    if !url.path().ends_with('/') {
        let adjusted_path = format!("{}/", url.path().trim_end_matches('/'));
        url.set_path(&adjusted_path);
    }

    Ok(url)
}

fn is_active_task(task: &Task) -> bool {
    !task.deleted && !matches!(task.status.as_deref(), Some("ARCHIVED" | "CANCELLED"))
}

#[derive(Debug, Deserialize, Default)]
struct ApiErrorPayload {
    message: Option<String>,
    title: Option<String>,
    detail: Option<String>,
    error: Option<String>,
}

async fn parse_api_error(response: Response) -> CliError {
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    let parsed: ApiErrorPayload = serde_json::from_str(&body).unwrap_or_default();

    let mut message = parsed
        .message
        .or(parsed.title)
        .or(parsed.error)
        .unwrap_or_else(|| format!("Request failed with HTTP {status}."));

    if let Some(detail) = parsed.detail {
        let detail = detail.trim();
        if !detail.is_empty() && !message.contains(detail) {
            message = format!("{message} ({detail})");
        }
    }

    CliError::Api {
        status,
        message,
        hint: hint_for_status(status),
    }
}

fn hint_for_status(status: u16) -> Option<String> {
    match status {
        400 | 422 => {
            Some("Check command arguments. Example due format: 2026-02-19T15:00:00Z.".to_string())
        }
        401 | 403 => {
            Some("Set a valid API key with RECLAIM_API_KEY or --api-key, then retry.".to_string())
        }
        404 => Some("Verify the task ID exists in your Reclaim account.".to_string()),
        429 => Some("Rate limited by Reclaim. Wait a few seconds and retry.".to_string()),
        500..=599 => Some("Reclaim API seems unavailable right now. Retry shortly.".to_string()),
        _ => None,
    }
}

fn map_transport_error(error: reqwest::Error) -> CliError {
    if error.is_timeout() {
        return CliError::Transport {
            message: "Request to Reclaim timed out.".to_string(),
            hint: Some("Try again or raise --timeout-secs.".to_string()),
        };
    }

    if error.is_connect() {
        return CliError::Transport {
            message: "Could not connect to the Reclaim API.".to_string(),
            hint: Some("Check network access and confirm --base-url is correct.".to_string()),
        };
    }

    CliError::Transport {
        message: format!("Request failed: {error}"),
        hint: Some("Retry. If this keeps happening, verify your network and API key.".to_string()),
    }
}
