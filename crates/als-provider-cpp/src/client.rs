//! HTTP client mỏng cho ace-server. Tách riêng để provider.rs chỉ lo mapping
//! sang trait; khi spike S-01 xác nhận field, sửa tập trung ở đây.

use als_provider::{ProviderError, Result};
use std::path::Path;
use std::time::Duration;

#[derive(Clone)]
pub struct AceServerClient {
    http: reqwest::Client,
    base: String,
}

impl AceServerClient {
    pub fn new(base: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            // Render dài phút — không dùng timeout tổng của reqwest;
            // timeout do provider đặt theo từng pha.
            .connect_timeout(Duration::from_secs(5))
            .build()
            .expect("reqwest client build không thể fail với config tĩnh");
        Self {
            http,
            base: base.into().trim_end_matches('/').to_owned(),
        }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    /// GET /props — health probe. Trả raw JSON; provider tự rút field.
    pub async fn props(&self) -> Result<serde_json::Value> {
        let res = self
            .http
            .get(format!("{}/props", self.base))
            .send()
            .await
            .map_err(|e| ProviderError::Http(e.to_string()))?;
        if !res.status().is_success() {
            return Err(ProviderError::Unavailable(format!(
                "/props → HTTP {}",
                res.status()
            )));
        }
        res.json::<serde_json::Value>()
            .await
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))
    }

    /// POST /lm — pha LM. Trả raw JSON chứa audio_codes.
    pub async fn lm(&self, payload: &serde_json::Value) -> Result<serde_json::Value> {
        let res = self
            .http
            .post(format!("{}/lm", self.base))
            .json(payload)
            .send()
            .await
            .map_err(|e| ProviderError::Http(e.to_string()))?;
        let status = res.status();
        let body = res
            .text()
            .await
            .map_err(|e| ProviderError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(ProviderError::Worker(format!("/lm → HTTP {status}: {body}")));
        }
        serde_json::from_str(&body).map_err(|e| ProviderError::InvalidResponse(e.to_string()))
    }

    /// POST /synth?wav=1 — pha DiT+VAE, trả WAV bytes trực tiếp.
    pub async fn synth_wav(&self, payload: &serde_json::Value) -> Result<Vec<u8>> {
        let res = self
            .http
            .post(format!("{}/synth?wav=1", self.base))
            .json(payload)
            .send()
            .await
            .map_err(|e| ProviderError::Http(e.to_string()))?;
        let status = res.status();
        let bytes = res
            .bytes()
            .await
            .map_err(|e| ProviderError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(ProviderError::Worker(format!(
                "/synth → HTTP {status}: {}",
                String::from_utf8_lossy(&bytes[..bytes.len().min(512)])
            )));
        }
        Ok(bytes.to_vec())
    }

    /// POST /understand — multipart upload file audio.
    pub async fn understand(&self, audio_path: &Path) -> Result<serde_json::Value> {
        let bytes = std::fs::read(audio_path)?;
        let file_name = audio_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("audio.wav")
            .to_owned();
        let part = reqwest::multipart::Part::bytes(bytes).file_name(file_name);
        let form = reqwest::multipart::Form::new().part("audio", part);
        let res = self
            .http
            .post(format!("{}/understand", self.base))
            .multipart(form)
            .send()
            .await
            .map_err(|e| ProviderError::Http(e.to_string()))?;
        let status = res.status();
        let body = res
            .text()
            .await
            .map_err(|e| ProviderError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(ProviderError::Worker(format!(
                "/understand → HTTP {status}: {body}"
            )));
        }
        serde_json::from_str(&body).map_err(|e| ProviderError::InvalidResponse(e.to_string()))
    }
}
