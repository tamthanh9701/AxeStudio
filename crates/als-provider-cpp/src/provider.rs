//! CppProvider — map trait RenderProvider → ace-server endpoints.

use crate::client::AceServerClient;
use als_core::{GenerationRecipe, JobId, ModelTier, ProviderId};
use als_provider::{
    AudioAnalysis, AudioBlob, AudioFormat, CancelOutcome, Capability, Health, JobCtx,
    ModelDescriptor, ModelId, PlanInput, PlanOutput, ProgressStage, ProviderError, RenderInput,
    RenderOutput, RenderProvider, Result, Slot, UnderstandInput,
};
use async_trait::async_trait;
use std::time::Duration;

/// Model mặc định của bộ GGUF Q8_0 tối thiểu (~7.7GB, xem plan §Model Zoo).
const DEFAULT_MODELS: &[(&str, ModelTier)] = &[
    ("acestep-v15-turbo-Q8_0.gguf", ModelTier::Turbo),
    ("acestep-v15-sft-Q8_0.gguf", ModelTier::Sft),
    ("acestep-v15-base-Q8_0.gguf", ModelTier::Base),
];

pub struct CppProvider {
    client: AceServerClient,
    models: Vec<ModelDescriptor>,
    caps: Vec<Capability>,
}

impl CppProvider {
    /// `base_url`: vd http://127.0.0.1:8080 — orchestrator spawn ace-server
    /// làm sidecar và truyền port vào đây.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            client: AceServerClient::new(base_url),
            models: DEFAULT_MODELS
                .iter()
                .map(|(id, tier)| ModelDescriptor {
                    id: ModelId((*id).to_owned()),
                    tier: *tier,
                    // TODO(S-01): đọc checksum thật từ thư mục model — checksum
                    // đi vào render_hash, dùng placeholder sẽ làm cache sai khi
                    // user đổi file weights. Blocked bởi model store (WS-G).
                    checksum: format!("todo-spike-{id}"),
                    vram_estimate_mb: None,
                    warm: true,
                })
                .collect(),
            caps: vec![
                Capability::Text2Music,
                Capability::Understand,
                Capability::SplitPlanRender,
            ],
        }
    }

    /// Chờ server ready — orchestrator gọi sau khi spawn sidecar.
    pub async fn wait_ready(&self, timeout: Duration) -> Result<()> {
        let start = std::time::Instant::now();
        loop {
            match self.client.props().await {
                Ok(_) => return Ok(()),
                Err(_) if start.elapsed() < timeout => {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
                Err(e) => return Err(e),
            }
        }
    }
}

/// Trích audio_codes từ response /lm.
///
/// Shape SERVER THẬT (capture máy đo 2026-08-26, issue #14): **MẢNG**
/// `[{"audio_codes":"58919,53780,…"}]` — batch_size=1 vẫn trả mảng
/// (PowerShell tự unwrap single-element nên script spike không dính; Rust
/// serde thì có). Chấp nhận CẢ object lẫn array + vài tên field cũ.
fn extract_audio_codes(v: &serde_json::Value) -> Result<String> {
    // Mảng → lấy phần tử đầu (batch_size=1 trong v1).
    let item = if v.is_array() {
        v.as_array()
            .and_then(|a| a.first())
            .ok_or_else(|| ProviderError::InvalidResponse("/lm trả mảng rỗng".into()))?
    } else {
        v
    };
    for key in ["audio_codes", "audio_code_string", "codes"] {
        if let Some(s) = item.get(key).and_then(|x| x.as_str()) {
            if !s.is_empty() {
                return Ok(s.to_owned());
            }
        }
    }
    Err(ProviderError::InvalidResponse(format!(
        "/lm không trả audio_codes: {}",
        &v.to_string()[..v.to_string().len().min(256)]
    )))
}

#[async_trait]
impl RenderProvider for CppProvider {
    fn id(&self) -> ProviderId {
        ProviderId(ProviderId::CPP.to_owned())
    }

    fn capabilities(&self) -> &[Capability] {
        &self.caps
    }

    async fn health(&self) -> Result<Health> {
        match self.client.props().await {
            Ok(v) => Ok(Health {
                ready: true,
                vram_free_mb: v.get("vram_free_mb").and_then(|x| x.as_u64()),
                loaded_models: self.models.iter().map(|m| m.id.clone()).collect(),
                detail: Some(v.to_string()),
            }),
            Err(e) => Ok(Health {
                ready: false,
                vram_free_mb: None,
                loaded_models: vec![],
                detail: Some(format!("ace-server không trả lời: {e}")),
            }),
        }
    }

    async fn models(&self) -> Result<Vec<ModelDescriptor>> {
        Ok(self.models.clone())
    }

    async fn plan(&self, input: PlanInput, cx: JobCtx) -> Result<PlanOutput> {
        if cx.cancel.is_cancelled() {
            return Err(ProviderError::Cancelled);
        }
        cx.report(5, ProgressStage::Planning).await;
        let payload = lm_body(&input.recipe);
        let res = self.client.lm(&payload).await?;
        if cx.cancel.is_cancelled() {
            return Err(ProviderError::Cancelled);
        }
        cx.report(100, ProgressStage::Planning).await;
        Ok(PlanOutput {
            audio_codes: extract_audio_codes(&res)?,
            lyrics: res
                .get("lyrics")
                .and_then(|x| x.as_str())
                .map(str::to_owned),
            metas: res.get("metas").cloned().unwrap_or(serde_json::Value::Null),
        })
    }

    async fn render(&self, input: RenderInput, cx: JobCtx) -> Result<RenderOutput> {
        if cx.cancel.is_cancelled() {
            return Err(ProviderError::Cancelled);
        }
        cx.report(0, ProgressStage::Rendering).await;
        let r = &input.recipe;
        let seed = r.sampling.seed.unwrap_or_else(random_seed);
        let mut payload = lm_body(r);
        payload["seed"] = serde_json::json!(seed);
        payload["audio_codes"] = serde_json::json!(input.plan.audio_codes);

        // Huỷ giữa chừng = bỏ chờ response (server vẫn tính tiếp — xem cancel()).
        let bytes = tokio::select! {
            res = self.client.synth_wav(&payload) => res?,
            _ = cx.cancel.cancelled() => return Err(ProviderError::Cancelled),
        };
        if !bytes.starts_with(b"RIFF") {
            return Err(ProviderError::InvalidResponse(
                "/synth?wav=1 không trả WAV RIFF".into(),
            ));
        }
        cx.report(100, ProgressStage::Rendering).await;
        Ok(RenderOutput {
            audio: AudioBlob {
                format: AudioFormat::WavF32,
                bytes,
            },
            seed_used: seed,
            duration_ms: u64::from(r.duration_s) * 1000,
            audio_codes: if input.plan.audio_codes.is_empty() {
                None
            } else {
                Some(input.plan.audio_codes)
            },
        })
    }

    async fn understand(&self, input: UnderstandInput, cx: JobCtx) -> Result<AudioAnalysis> {
        if cx.cancel.is_cancelled() {
            return Err(ProviderError::Cancelled);
        }
        let res = self.client.understand(&input.audio_path).await?;
        Ok(AudioAnalysis {
            caption: res
                .get("caption")
                .and_then(|x| x.as_str())
                .unwrap_or_default()
                .to_owned(),
            bpm: res.get("bpm").and_then(|x| x.as_f64()).map(|x| x as f32),
            key_scale: res
                .get("key_scale")
                .or_else(|| res.get("key"))
                .and_then(|x| x.as_str())
                .map(str::to_owned),
            duration_ms: res.get("duration_ms").and_then(|x| x.as_u64()),
            extra: res,
        })
    }

    async fn warmup(&self, _model: &ModelId, _slot: Slot, cx: JobCtx) -> Result<()> {
        // ace-server nạp model lúc spawn — warmup là no-op thực sự, nên báo
        // thẳng 100% (xác nhận "load xong" là thật: model đã resident).
        cx.report(100, ProgressStage::Planning).await;
        Ok(())
    }

    async fn cancel(&self, _job: &JobId) -> Result<CancelOutcome> {
        // ace-server không có endpoint cancel. Request phía client bị abort qua
        // CancellationToken ở render(), nhưng GPU vẫn tính tiếp → TooLate là
        // câu trả lời TRUNG THỰC duy nhất (AGENTS.md §6).
        Ok(CancelOutcome::TooLate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use als_core::TaskType;

    #[test]
    fn extract_codes_tolerates_field_variants() {
        let v = serde_json::json!({"audio_code_string": "FSQ:abc"});
        assert_eq!(extract_audio_codes(&v).unwrap(), "FSQ:abc");
        let v = serde_json::json!({"audio_codes": "FSQ:xyz"});
        assert_eq!(extract_audio_codes(&v).unwrap(), "FSQ:xyz");
        let v = serde_json::json!({"nothing": 1});
        assert!(extract_audio_codes(&v).is_err());
    }

    #[test]
    fn cpp_declares_no_repaint_capability() {
        // acestep.cpp không có repaint/lego/extract/complete (ADR-001).
        let p = CppProvider::new("http://127.0.0.1:8080");
        assert!(!p.capabilities().contains(&Capability::Repaint));
        assert!(!p
            .capabilities()
            .contains(&Capability::for_task(TaskType::Extract)));
        assert!(p.capabilities().contains(&Capability::Text2Music));
        assert!(p.capabilities().contains(&Capability::SplitPlanRender));
    }
}

/// Body chuẩn cho `/lm` — contract thật của ace-server (`src/request.cpp`,
/// xác nhận S-01, khớp Bench-Matrix.ps1): ĐÚNG 6 field này. Field lạ
/// (model/task_type/bpm/…) làm parser server bỏ payload → lỗi
/// "caption is empty" dù caption có mặt (BUG integration 2026-08-25).
fn lm_body(r: &GenerationRecipe) -> serde_json::Value {
    serde_json::json!({
        "caption": r.prompt,
        "lyrics": r.lyrics,
        "duration": r.duration_s,
        "inference_steps": r.sampling.inference_steps,
        "batch_size": r.sampling.batch_size,
        "seed": r.sampling.seed.unwrap_or_else(random_seed),
    })
}

/// Seed ngẫu nhiên khi recipe để seed=None — không dùng crate rand
/// (deps của cpp provider bị khoá theo AGENTS §2); nanos hệ thống là đủ
/// cho mục đích "mỗi lần một biến thể".
fn random_seed() -> u64 {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    d.as_nanos() as u64 % 2_147_483_647
}

#[cfg(test)]
mod payload_tests {
    use super::*;
    use als_core::{GenerationRecipe, ModelTier, SamplingParams, TaskType};

    fn recipe() -> GenerationRecipe {
        GenerationRecipe {
            prompt: "lofi chill".into(),
            lyrics: "[Instrumental]".into(),
            duration_s: 30,
            bpm: Some(100),
            key_scale: None,
            time_signature: Some(4),
            vocal_language: None,
            task: TaskType::Text2Music,
            model_tier: ModelTier::Sft,
            reference_audio: None,
            source_audio: None,
            repaint_range_ms: None,
            sampling: SamplingParams {
                seed: Some(7),
                inference_steps: 8,
                ..Default::default()
            },
            provider_overrides: Default::default(),
        }
    }

    #[test]
    fn lm_body_matches_ace_server_contract_exactly() {
        let body = lm_body(&recipe());
        let obj = body.as_object().unwrap();
        let mut keys: Vec<_> = obj.keys().map(String::as_str).collect();
        keys.sort();
        assert_eq!(
            keys,
            [
                "batch_size",
                "caption",
                "duration",
                "inference_steps",
                "lyrics",
                "seed"
            ],
            "payload /lm phải đúng 6 field của src/request.cpp"
        );
        assert_eq!(obj["caption"], "lofi chill");
        assert_eq!(obj["duration"], 30);
        assert_eq!(obj["seed"], 7);
        assert_eq!(obj["inference_steps"], 8);
    }

    #[test]
    fn synth_body_is_lm_body_plus_audio_codes() {
        let mut body = lm_body(&recipe());
        body["seed"] = serde_json::json!(99);
        body["audio_codes"] = serde_json::json!("FSQ:1,2,3");
        let obj = body.as_object().unwrap();
        assert_eq!(obj["audio_codes"], "FSQ:1,2,3");
        assert_eq!(obj["seed"], 99);
        assert!(obj.get("model").is_none(), "KHÔNG được gửi field model");
    }

    #[test]
    fn random_seed_in_positive_i32_range() {
        let s = random_seed();
        assert!(s < 2_147_483_647);
    }
}

#[cfg(test)]
mod response_tests {
    use super::*;

    #[test]
    fn parses_real_server_array_shape() {
        // Capture thật từ máy đo 2026-08-26 (issue #14) — batch_size=1
        // vẫn trả MẢNG một phần tử.
        let raw = r#"[{"audio_codes":"58919,53780,2070,12345"}]"#;
        let v: serde_json::Value = serde_json::from_str(raw).unwrap();
        let codes = extract_audio_codes(&v).unwrap();
        assert_eq!(codes, "58919,53780,2070,12345");
    }

    #[test]
    fn still_accepts_plain_object_shape() {
        let v: serde_json::Value = serde_json::from_str(r#"{"audio_codes":"1,2"}"#).unwrap();
        assert_eq!(extract_audio_codes(&v).unwrap(), "1,2");
    }

    #[test]
    fn empty_array_is_clear_error_not_panic() {
        let v: serde_json::Value = serde_json::from_str("[]").unwrap();
        let err = extract_audio_codes(&v).unwrap_err();
        assert!(err.to_string().contains("mảng rỗng"));
    }
}
