use crate::error::CliError;
use reqwest::{
    header::{self, HeaderMap},
    Client, Method, RequestBuilder, Url,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{collections::HashMap, time::Duration};

const USER_AGENT: &str = concat!("reclaim-cli/", env!("CARGO_PKG_VERSION"));
const DEBUG_BODY_LIMIT: usize = 8_192;
const DEBUG_SUMMARY_LIMIT: usize = 512;

pub trait ReclaimApi {
    async fn list_tasks(&self, filter: TaskFilter) -> Result<Vec<Task>, CliError>;
    async fn get_task(&self, task_id: u64) -> Result<Task, CliError>;
    async fn create_task(&self, request: CreateTaskRequest) -> Result<Task, CliError>;
    async fn list_events(&self, query: EventListQuery) -> Result<Vec<serde_json::Value>, CliError>;
    async fn get_event(
        &self,
        calendar_id: u64,
        event_id: &str,
        source_details: Option<bool>,
        thin: Option<bool>,
    ) -> Result<serde_json::Value, CliError>;
    async fn apply_schedule_actions(
        &self,
        request: serde_json::Value,
    ) -> Result<serde_json::Value, CliError>;
    async fn put_task(
        &self,
        task_id: u64,
        request: serde_json::Value,
        notification_key: Option<&str>,
    ) -> Result<Task, CliError>;
    async fn patch_task(
        &self,
        task_id: u64,
        request: serde_json::Value,
        notification_key: Option<&str>,
    ) -> Result<Task, CliError>;
    async fn delete_task(
        &self,
        task_id: u64,
        notification_key: Option<&str>,
    ) -> Result<serde_json::Value, CliError>;
}

#[derive(Debug, Clone, Copy)]
pub enum TaskFilter {
    Active,
    All,
}

#[derive(Debug, Clone, Default)]
pub struct EventListQuery {
    pub calendar_ids: Vec<u64>,
    pub all_connected: Option<bool>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub source_details: Option<bool>,
    pub thin: Option<bool>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_chunk_size: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_chunk_size: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub always_private: Option<bool>,
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

#[derive(Debug, Clone)]
struct RequestDebugInfo {
    method: String,
    url: String,
    body: Option<String>,
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

    fn request_with_notification_key(
        &self,
        method: Method,
        path: &str,
        notification_key: Option<&str>,
    ) -> RequestBuilder {
        let request = self.request(method, path);
        if let Some(notification_key) = notification_key
            .map(str::trim)
            .filter(|notification_key| !notification_key.is_empty())
        {
            request.query(&[("notificationKey", notification_key)])
        } else {
            request
        }
    }

    async fn send_json<T: DeserializeOwned>(&self, request: RequestBuilder) -> Result<T, CliError> {
        let request_debug = capture_request_debug(&request);
        let response = request
            .send()
            .await
            .map_err(|error| map_transport_error(error, request_debug.as_ref()))?;
        let status = response.status();
        let response_url = response.url().to_string();
        let response_headers = response.headers().clone();
        let response_body = response.text().await.map_err(|error| CliError::Transport {
            message: format!(
                "Could not read Reclaim API response body: {error}\n{}",
                format_request_context(request_debug.as_ref())
            ),
            hint: Some(
                "Retry the command. If this repeats, capture the output and file a bug."
                    .to_string(),
            ),
        })?;

        if status.is_success() {
            serde_json::from_str::<T>(&response_body).map_err(|error| {
                let mut lines = vec![format!(
                    "Reclaim API returned a non-JSON success response: {error}"
                )];

                if let Some(request_debug) = request_debug.as_ref() {
                    lines.push(format!(
                        "Request: {} {}",
                        request_debug.method, request_debug.url
                    ));
                } else {
                    lines.push(format!("Response URL: {response_url}"));
                }

                let body = response_body.trim();
                if !body.is_empty() {
                    lines.push(format!(
                        "Raw response body: {}",
                        truncate_debug_text(&pretty_json_or_raw(body), DEBUG_BODY_LIMIT)
                    ));
                } else {
                    lines.push("Raw response body: <empty>".to_string());
                }

                CliError::ResponseParse {
                    message: lines.join("\n"),
                    hint: Some(
                        "Keep the raw response body above when reporting this issue.".to_string(),
                    ),
                }
            })
        } else {
            Err(parse_api_error(
                status.as_u16(),
                &response_body,
                &response_url,
                &response_headers,
                request_debug.as_ref(),
            ))
        }
    }

    async fn send_json_value_or_null(
        &self,
        request: RequestBuilder,
    ) -> Result<serde_json::Value, CliError> {
        let request_debug = capture_request_debug(&request);
        let response = request
            .send()
            .await
            .map_err(|error| map_transport_error(error, request_debug.as_ref()))?;
        let status = response.status();
        let response_url = response.url().to_string();
        let response_headers = response.headers().clone();
        let response_body = response.text().await.map_err(|error| CliError::Transport {
            message: format!(
                "Could not read Reclaim API response body: {error}\n{}",
                format_request_context(request_debug.as_ref())
            ),
            hint: Some(
                "Retry the command. If this repeats, capture the output and file a bug."
                    .to_string(),
            ),
        })?;

        if status.is_success() {
            let body = response_body.trim();
            if body.is_empty() {
                return Ok(serde_json::Value::Null);
            }

            serde_json::from_str::<serde_json::Value>(body).map_err(|error| {
                let mut lines = vec![format!(
                    "Reclaim API returned a non-JSON success response: {error}"
                )];

                if let Some(request_debug) = request_debug.as_ref() {
                    lines.push(format!(
                        "Request: {} {}",
                        request_debug.method, request_debug.url
                    ));
                } else {
                    lines.push(format!("Response URL: {response_url}"));
                }

                lines.push(format!(
                    "Raw response body: {}",
                    truncate_debug_text(&pretty_json_or_raw(body), DEBUG_BODY_LIMIT)
                ));

                CliError::ResponseParse {
                    message: lines.join("\n"),
                    hint: Some(
                        "Keep the raw response body above when reporting this issue.".to_string(),
                    ),
                }
            })
        } else {
            Err(parse_api_error(
                status.as_u16(),
                &response_body,
                &response_url,
                &response_headers,
                request_debug.as_ref(),
            ))
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

    async fn list_events(&self, query: EventListQuery) -> Result<Vec<serde_json::Value>, CliError> {
        let mut request = self.request(Method::GET, "events");
        let mut query_pairs: Vec<(String, String)> = Vec::new();

        for calendar_id in query.calendar_ids {
            query_pairs.push(("calendarIds".to_string(), calendar_id.to_string()));
        }

        if let Some(all_connected) = query.all_connected {
            query_pairs.push(("allConnected".to_string(), all_connected.to_string()));
        }

        if let Some(start) = query
            .start
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            query_pairs.push(("start".to_string(), start.to_string()));
        }

        if let Some(end) = query
            .end
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            query_pairs.push(("end".to_string(), end.to_string()));
        }

        if let Some(source_details) = query.source_details {
            query_pairs.push(("sourceDetails".to_string(), source_details.to_string()));
        }

        if let Some(thin) = query.thin {
            query_pairs.push(("thin".to_string(), thin.to_string()));
        }

        if !query_pairs.is_empty() {
            request = request.query(&query_pairs);
        }

        self.send_json(request).await
    }

    async fn get_event(
        &self,
        calendar_id: u64,
        event_id: &str,
        source_details: Option<bool>,
        thin: Option<bool>,
    ) -> Result<serde_json::Value, CliError> {
        let mut request = self.request(Method::GET, &format!("events/{calendar_id}/{event_id}"));
        let mut query_pairs: Vec<(String, String)> = Vec::new();

        if let Some(source_details) = source_details {
            query_pairs.push(("sourceDetails".to_string(), source_details.to_string()));
        }
        if let Some(thin) = thin {
            query_pairs.push(("thin".to_string(), thin.to_string()));
        }

        if !query_pairs.is_empty() {
            request = request.query(&query_pairs);
        }

        self.send_json(request).await
    }

    async fn apply_schedule_actions(
        &self,
        request: serde_json::Value,
    ) -> Result<serde_json::Value, CliError> {
        self.send_json(
            self.request(Method::POST, "schedule-actions/apply-actions")
                .json(&request),
        )
        .await
    }

    async fn put_task(
        &self,
        task_id: u64,
        request: serde_json::Value,
        notification_key: Option<&str>,
    ) -> Result<Task, CliError> {
        self.send_json(
            self.request_with_notification_key(
                Method::PUT,
                &format!("tasks/{task_id}"),
                notification_key,
            )
            .json(&request),
        )
        .await
    }

    async fn patch_task(
        &self,
        task_id: u64,
        request: serde_json::Value,
        notification_key: Option<&str>,
    ) -> Result<Task, CliError> {
        self.send_json(
            self.request_with_notification_key(
                Method::PATCH,
                &format!("tasks/{task_id}"),
                notification_key,
            )
            .json(&request),
        )
        .await
    }

    async fn delete_task(
        &self,
        task_id: u64,
        notification_key: Option<&str>,
    ) -> Result<serde_json::Value, CliError> {
        self.send_json_value_or_null(self.request_with_notification_key(
            Method::DELETE,
            &format!("tasks/{task_id}"),
            notification_key,
        ))
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

fn capture_request_debug(request: &RequestBuilder) -> Option<RequestDebugInfo> {
    let request = request.try_clone()?.build().ok()?;
    let body = request
        .body()
        .and_then(|body| body.as_bytes())
        .map(|bytes| {
            std::str::from_utf8(bytes)
                .map(|value| value.trim().to_string())
                .unwrap_or_else(|_| format!("<{} bytes binary request body>", bytes.len()))
        })
        .filter(|payload| !payload.is_empty());

    Some(RequestDebugInfo {
        method: request.method().to_string(),
        url: request.url().to_string(),
        body,
    })
}

fn parse_api_error(
    status: u16,
    response_body: &str,
    response_url: &str,
    response_headers: &HeaderMap,
    request_debug: Option<&RequestDebugInfo>,
) -> CliError {
    let body = response_body.trim();
    let parsed_json = if body.is_empty() {
        None
    } else {
        serde_json::from_str::<serde_json::Value>(body).ok()
    };

    let message = parsed_json
        .as_ref()
        .and_then(extract_api_message)
        .or_else(|| {
            if body.is_empty() {
                None
            } else {
                Some(truncate_debug_text(body, DEBUG_SUMMARY_LIMIT))
            }
        })
        .unwrap_or_else(|| format!("Request failed with HTTP {status}."));

    let (request_method, request_url) = request_debug
        .map(|request| (request.method.as_str(), request.url.as_str()))
        .unwrap_or(("UNKNOWN", response_url));
    let mut lines = vec![
        format!("Request: {request_method} {request_url}"),
        format!("API message: {message}"),
    ];

    if request_url != response_url {
        lines.push(format!("Response URL: {response_url}"));
    }

    if let Some(request_id) = extract_request_id(response_headers) {
        lines.push(format!("Reclaim request id: {request_id}"));
    }

    if let Some(parsed_json) = parsed_json {
        lines.push(format!(
            "Raw response JSON: {}",
            truncate_debug_text(
                &pretty_json_or_raw(&parsed_json.to_string()),
                DEBUG_BODY_LIMIT
            )
        ));
    } else if body.is_empty() {
        lines.push("Raw response body: <empty>".to_string());
    } else {
        lines.push(format!(
            "Raw response body: {}",
            truncate_debug_text(body, DEBUG_BODY_LIMIT)
        ));
    }

    if let Some(request_debug) = request_debug {
        if let Some(payload) = request_debug.body.as_deref() {
            lines.push(format!(
                "Request payload: {}",
                truncate_debug_text(&pretty_json_or_raw(payload), DEBUG_BODY_LIMIT)
            ));
        }
    }

    CliError::Api {
        status,
        message: lines.join("\n"),
        hint: hint_for_status(status),
    }
}

fn extract_api_message(value: &serde_json::Value) -> Option<String> {
    for field in ["message", "title", "error", "detail"] {
        let candidate = value
            .get(field)
            .and_then(|entry| entry.as_str())
            .map(str::trim)
            .filter(|entry| !entry.is_empty());
        if let Some(candidate) = candidate {
            return Some(candidate.to_string());
        }
    }

    value
        .get("errors")
        .and_then(extract_errors_message)
        .map(|message| message.trim().to_string())
        .filter(|message| !message.is_empty())
}

fn extract_errors_message(errors: &serde_json::Value) -> Option<String> {
    if let Some(message) = errors.as_str() {
        return Some(message.to_string());
    }

    if let Some(array) = errors.as_array() {
        for item in array {
            if let Some(message) = item.as_str() {
                return Some(message.to_string());
            }
            if let Some(message) = item.get("message").and_then(|entry| entry.as_str()) {
                return Some(message.to_string());
            }
        }
    }

    if let Some(object) = errors.as_object() {
        for (field, value) in object {
            if let Some(message) = value.as_str() {
                return Some(format!("{field}: {message}"));
            }
            if let Some(array) = value.as_array() {
                for entry in array {
                    if let Some(message) = entry.as_str() {
                        return Some(format!("{field}: {message}"));
                    }
                    if let Some(message) = entry.get("message").and_then(|item| item.as_str()) {
                        return Some(format!("{field}: {message}"));
                    }
                }
            }
        }
    }

    None
}

fn extract_request_id(headers: &HeaderMap) -> Option<String> {
    for name in ["x-request-id", "x-correlation-id", "x-amzn-trace-id"] {
        if let Some(value) = headers.get(name) {
            let value = value.to_str().ok().map(str::trim).unwrap_or_default();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }

    None
}

fn pretty_json_or_raw(input: &str) -> String {
    serde_json::from_str::<serde_json::Value>(input)
        .and_then(|json| serde_json::to_string_pretty(&json))
        .unwrap_or_else(|_| input.to_string())
}

fn truncate_debug_text(input: &str, max_chars: usize) -> String {
    let mut truncated = String::new();
    for (count, ch) in input.chars().enumerate() {
        if count >= max_chars {
            truncated.push_str("... <truncated>");
            return truncated;
        }
        truncated.push(ch);
    }
    truncated
}

fn format_request_context(request_debug: Option<&RequestDebugInfo>) -> String {
    let Some(request_debug) = request_debug else {
        return String::new();
    };

    let mut lines = vec![format!(
        "Request: {} {}",
        request_debug.method, request_debug.url
    )];

    if let Some(payload) = request_debug.body.as_deref() {
        lines.push(format!(
            "Request payload: {}",
            truncate_debug_text(&pretty_json_or_raw(payload), DEBUG_SUMMARY_LIMIT)
        ));
    }

    format!("\n{}", lines.join("\n"))
}

fn hint_for_status(status: u16) -> Option<String> {
    match status {
        400 | 422 => Some(
            "Check command arguments and inspect the raw response JSON above for field-level validation details."
                .to_string(),
        ),
        401 | 403 => {
            Some("Set a valid API key with RECLAIM_API_KEY or --api-key, then retry.".to_string())
        }
        404 => Some("Verify the task ID exists in your Reclaim account.".to_string()),
        429 => Some("Rate limited by Reclaim. Wait a few seconds and retry.".to_string()),
        500..=599 => Some(
            "Reclaim returned a 5xx. This can be an outage OR a rejected payload surfaced as internal_error. Compare the request payload above with a known-good request."
                .to_string(),
        ),
        _ => None,
    }
}

fn map_transport_error(
    error: reqwest::Error,
    request_debug: Option<&RequestDebugInfo>,
) -> CliError {
    let request_context = format_request_context(request_debug);

    if error.is_timeout() {
        return CliError::Transport {
            message: format!(
                "Request to Reclaim timed out before receiving a response. Source error: {error}{request_context}"
            ),
            hint: Some("Try again or raise --timeout-secs.".to_string()),
        };
    }

    if error.is_connect() {
        return CliError::Transport {
            message: format!(
                "Could not connect to the Reclaim API. Source error: {error}{request_context}"
            ),
            hint: Some("Check network access and confirm --base-url is correct.".to_string()),
        };
    }

    CliError::Transport {
        message: format!(
            "Request failed before receiving a usable API response. Source error: {error}{request_context}"
        ),
        hint: Some("Retry. If this keeps happening, verify your network and API key.".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderValue;

    #[test]
    fn parse_api_error_includes_request_and_response_context() {
        let mut headers = HeaderMap::new();
        headers.insert("x-request-id", HeaderValue::from_static("req-123"));
        let request_debug = RequestDebugInfo {
            method: "POST".to_string(),
            url: "https://api.app.reclaim.ai/api/tasks".to_string(),
            body: Some(r#"{"title":"fix??","priority":"P4"}"#.to_string()),
        };

        let error = parse_api_error(
            500,
            r#"{"error":"internal_error","detail":"validation failed"}"#,
            "https://api.app.reclaim.ai/api/tasks",
            &headers,
            Some(&request_debug),
        );
        let rendered = error.to_string();

        assert!(rendered.contains("Reclaim API returned HTTP 500"));
        assert!(rendered.contains("Request: POST https://api.app.reclaim.ai/api/tasks"));
        assert!(rendered.contains("Raw response JSON"));
        assert!(rendered.contains("Request payload"));
        assert!(rendered.contains("Reclaim request id: req-123"));
    }

    #[test]
    fn extract_api_message_reads_nested_errors_object() {
        let payload = serde_json::json!({
            "errors": {
                "priority": ["must be one of P1, P2, P3, P4"]
            }
        });

        assert_eq!(
            extract_api_message(&payload),
            Some("priority: must be one of P1, P2, P3, P4".to_string())
        );
    }

    #[test]
    fn request_with_notification_key_adds_query_param() {
        let api = HttpReclaimApi::new(
            Some("key".to_string()),
            "https://api.app.reclaim.ai/api".to_string(),
            15,
        )
        .expect("api should initialize");

        let request = api
            .request_with_notification_key(Method::PATCH, "tasks/123", Some("notif-123"))
            .build()
            .expect("request should build");

        assert_eq!(
            request.url().as_str(),
            "https://api.app.reclaim.ai/api/tasks/123?notificationKey=notif-123"
        );
    }

    #[test]
    fn request_with_notification_key_ignores_blank_value() {
        let api = HttpReclaimApi::new(
            Some("key".to_string()),
            "https://api.app.reclaim.ai/api".to_string(),
            15,
        )
        .expect("api should initialize");

        let request = api
            .request_with_notification_key(Method::DELETE, "tasks/123", Some("   "))
            .build()
            .expect("request should build");

        assert_eq!(
            request.url().as_str(),
            "https://api.app.reclaim.ai/api/tasks/123"
        );
    }
}
