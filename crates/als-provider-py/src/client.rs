//! HTTP client cho acestep-api. Mọi response đi qua wrapper
//! `{ data, code, error, timestamp, extra }` — unwrap ở một chỗ duy nhất.

use als_provider::{ProviderError, Result};
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Deserialize)]
struct Wrapper<T> {
    data: Option<T>,
    code: Option<i64>,
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ReleaseTaskData {
    pub task_id: String,
    #[serde(default)]
    pub queue_position: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    QueuedOrRunning,
    Succeeded,
    Failed,
    Unknown(i64),
}

#[derive(Debug, Deserialize)]
pub struct TaskResult {
    pub task_id: String,
    /// 0 = queued/running, 1 = succeeded, 2 = failed (docs/en/API.md).
    pub status: i64,
    /// Server THẬT (xác nhận máy đo 2026-08-24): payload nằm trong field
    /// `result` — MỘT CHUỖI JSON lồng dạng
    /// `[{"file": "/v1/audio?path=C%3A%5C…mp3", "status": 1, …}]`.
    #[serde(default)]
    pub result: Option<String>,
    /// docs/en/API.md ghi tên là `result_json` — chấp nhận cả hai tên.
    #[serde(default)]
    pub result_json: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

impl TaskResult {
    pub fn status(&self) -> TaskStatus {
        match self.status {
            0 => TaskStatus::QueuedOrRunning,
            1 => TaskStatus::Succeeded,
            2 => TaskStatus::Failed,
            other => TaskStatus::Unknown(other),
        }
    }

    /// Chuỗi JSON lồng chứa kết quả — ưu tiên `result` (server thật),
    /// fallback `result_json` (docs).
    pub fn inner(&self) -> Option<&str> {
        self.result.as_deref().or(self.result_json.as_deref())
    }
}

#[derive(Clone)]
pub struct AcestepApiClient {
    http: reqwest::Client,
    base: String,
    api_key: Option<String>,
}

impl AcestepApiClient {
    pub fn new(base: impl Into<String>, api_key: Option<String>) -> Self {
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .build()
            .expect("reqwest client build không thể fail với config tĩnh");
        Self {
            http,
            base: base.into().trim_end_matches('/').to_owned(),
            api_key,
        }
    }

    fn authed(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.api_key {
            Some(k) => req.header("Authorization", format!("Bearer {k}")),
            None => req,
        }
    }

    /// Unwrap `{ data, code, error }`. code lạ → Worker error.
    async fn unwrap<T: for<'de> Deserialize<'de>>(
        &self,
        res: reqwest::Response,
        endpoint: &str,
    ) -> Result<T> {
        let status = res.status();
        let body = res
            .text()
            .await
            .map_err(|e| ProviderError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(ProviderError::Worker(format!(
                "{endpoint} → HTTP {status}: {}",
                &body[..body.len().min(512)]
            )));
        }
        let w: Wrapper<T> = serde_json::from_str(&body)
            .map_err(|e| ProviderError::InvalidResponse(format!("{endpoint}: {e}")))?;
        if let Some(err) = w.error.filter(|e| !e.is_empty()) {
            return Err(ProviderError::Worker(format!("{endpoint}: {err}")));
        }
        ensure_success_code(w.code, endpoint)?;
        w.data
            .ok_or_else(|| ProviderError::InvalidResponse(format!("{endpoint}: data null")))
    }

    /// POST /release_task — tạo job single-shot (LM + DiT).
    pub async fn release_task(&self, payload: &serde_json::Value) -> Result<ReleaseTaskData> {
        let res = self
            .authed(self.http.post(format!("{}/release_task", self.base)))
            .json(payload)
            .send()
            .await
            .map_err(|e| ProviderError::Http(e.to_string()))?;
        self.unwrap(res, "/release_task").await
    }

    /// POST /query_result — poll trạng thái một task.
    pub async fn query_result(&self, task_id: &str) -> Result<TaskResult> {
        #[derive(serde::Serialize)]
        struct Q<'a> {
            task_id_list: [&'a str; 1],
        }
        let res = self
            .authed(self.http.post(format!("{}/query_result", self.base)))
            .json(&Q {
                task_id_list: [task_id],
            })
            .send()
            .await
            .map_err(|e| ProviderError::Http(e.to_string()))?;
        let list: Vec<TaskResult> = self.unwrap(res, "/query_result").await?;
        // into_iter tiêu thụ Vec — không giữ borrow nào qua expression cuối block.
        list.into_iter()
            .next()
            .ok_or_else(|| ProviderError::InvalidResponse("query_result rỗng".into()))
    }

    /// Tải file kết quả. `file` từ server là ENDPOINT đầy đủ kèm query đã
    /// percent-encoded ("/v1/audio?path=C%3A%5C…mp3") — dùng NGUYÊN VĂN
    /// (server tự decode; decode/re-encode phía client sẽ phá query).
    /// Chuỗi trần không có prefix → coi là path thuần, ghép qua query.
    pub async fn download_audio(&self, file: &str) -> Result<Vec<u8>> {
        let url = if file.starts_with('/') {
            format!("{}{}", self.base, file)
        } else {
            format!("{}/v1/audio?path={}", self.base, file)
        };
        let res = self
            .authed(self.http.get(url))
            .send()
            .await
            .map_err(|e| ProviderError::Http(e.to_string()))?;
        if !res.status().is_success() {
            return Err(ProviderError::Worker(format!(
                "/v1/audio → HTTP {}",
                res.status()
            )));
        }
        Ok(res
            .bytes()
            .await
            .map_err(|e| ProviderError::Http(e.to_string()))?
            .to_vec())
    }

    /// GET /v1/models — danh sách model server nhận diện được.
    pub async fn models(&self) -> Result<serde_json::Value> {
        let res = self
            .authed(self.http.get(format!("{}/v1/models", self.base)))
            .send()
            .await
            .map_err(|e| ProviderError::Http(e.to_string()))?;
        self.unwrap(res, "/v1/models").await
    }

    /// POST /v1/init — hot-swap model vào slot (1..=3).
    pub async fn init_model(&self, model: &str, slot: u8) -> Result<serde_json::Value> {
        let res = self
            .authed(self.http.post(format!("{}/v1/init", self.base)))
            .json(&serde_json::json!({ "model": model, "slot": slot }))
            .send()
            .await
            .map_err(|e| ProviderError::Http(e.to_string()))?;
        self.unwrap(res, "/v1/init").await
    }
}

/// Server THẬT trả `code: 200` khi thành công (xác nhận trên máy đo
/// 2026-08-24 — trước đây unwrap đòi code=0 khiến MỌI response hợp lệ bị
/// từ chối, chặn warm + generate: issue #14). Docs ghi 0 — chấp nhận cả hai.
fn ensure_success_code(code: Option<i64>, endpoint: &str) -> Result<()> {
    match code {
        None => Ok(()),
        Some(c) if c == 0 || c == 200 => Ok(()),
        Some(c) => Err(ProviderError::Worker(format!("{endpoint}: code={c}"))),
    }
}

#[cfg(test)]
mod code_tests {
    use super::*;

    #[test]
    fn accepts_zero_and_200_and_missing() {
        assert!(ensure_success_code(Some(0), "/x").is_ok());
        assert!(ensure_success_code(Some(200), "/x").is_ok());
        assert!(ensure_success_code(None, "/x").is_ok());
    }

    #[test]
    fn rejects_other_codes_with_endpoint_context() {
        let e = ensure_success_code(Some(404), "/release_task").unwrap_err();
        assert!(e.to_string().contains("/release_task"));
        assert!(e.to_string().contains("code=404"));
    }
}
