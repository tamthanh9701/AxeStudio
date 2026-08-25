//! PyProvider — single-shot: render() = release_task + poll + download.

use crate::client::{AcestepApiClient, TaskStatus};
use crate::payload::{build_release_payload, model_id_for_tier, AssetResolver};
use als_core::{JobId, ModelTier, ProviderId};
use als_provider::{
    AudioAnalysis, AudioBlob, AudioFormat, CancelOutcome, Capability, Health, JobCtx,
    ModelDescriptor, ModelId, PlanInput, PlanOutput, ProgressStage, ProviderError, RenderInput,
    RenderOutput, RenderProvider, Result, Slot, UnderstandInput,
};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct PyProvider {
    client: AcestepApiClient,
    resolve_asset: Arc<AssetResolver>,
    caps: Vec<Capability>,
    poll_interval: Duration,
    job_timeout: Duration,
}

impl PyProvider {
    pub fn new(
        base_url: impl Into<String>,
        api_key: Option<String>,
        resolve_asset: Arc<AssetResolver>,
    ) -> Self {
        Self {
            client: AcestepApiClient::new(base_url, api_key),
            resolve_asset,
            caps: vec![
                Capability::Text2Music,
                Capability::Cover,
                Capability::Repaint,
                Capability::Lego,
                Capability::Extract,
                Capability::Complete,
                Capability::HotSwapModel,
                // KHÔNG có CancelRunningJob (không có endpoint) và
                // KHÔNG có SplitPlanRender (single-shot). Đây là hai điểm
                // khác biệt bản chất so với cpp — xem ADR-001.
            ],
            poll_interval: Duration::from_millis(1000),
            // Trần trần cho MỘT job render/warm. Lần đo 2026-08-25 (issue
            // #14): VAE decode rơi CPU-tiled khi free VRAM < 2.6GB — mất
            // >20 phút cho 30s audio (và không ổn định: phiên trước cùng
            // đường chỉ ~90s). 1200s cũ GIẾT task đang tính đúng; server
            // lại không có endpoint huỷ → compute tiếp ngầm = lãng phí kép.
            // 3600s là headroom cho đường chậm hợp lệ; timeout chỉ nên
            // bắn khi task THẬT SỰ kẹt.
            job_timeout: Duration::from_secs(60 * 60),
        }
    }

    /// Poll một warm task tới khi xong — cùng hình dạng với loop render(),
    /// nhưng stage là Planning và không tải file kết quả.
    async fn poll_warm_task(&self, task_id: &str, cx: JobCtx) -> Result<()> {
        let started = Instant::now();
        loop {
            if cx.cancel.is_cancelled() {
                return Err(ProviderError::Cancelled);
            }
            if started.elapsed() > self.job_timeout {
                return Err(ProviderError::Timeout);
            }
            tokio::time::sleep(self.poll_interval).await;
            let res = self.client.query_result(task_id).await?;
            match res.status() {
                TaskStatus::QueuedOrRunning => {
                    // Không biết tổng thời gian load — leo dần tới 95% theo
                    // mốc WARM_ESTIMATE (median S-05), giống render() làm.
                    let pct = ((started.elapsed().as_millis() as f64
                        / WARM_ESTIMATE.as_millis() as f64)
                        * 95.0)
                        .min(95.0) as u8;
                    cx.report(pct.max(5), ProgressStage::Planning).await;
                }
                TaskStatus::Succeeded => {
                    cx.report(100, ProgressStage::Planning).await;
                    return Ok(());
                }
                TaskStatus::Failed => {
                    return Err(ProviderError::Worker(
                        res.error.unwrap_or_else(|| "warm task failed".into()),
                    ));
                }
                TaskStatus::Unknown(s) => {
                    return Err(ProviderError::InvalidResponse(format!("status lạ: {s}")));
                }
            }
        }
    }

    #[cfg(test)]
    pub fn with_poll_interval(mut self, d: Duration) -> Self {
        self.poll_interval = d;
        self
    }
}

/// Load model mất 25–37s trên máy đo (S-05, RTX 3070) — dùng median làm
/// mốc ước lượng tiến độ khi server không trả task handle cho /v1/init.
const WARM_ESTIMATE: Duration = Duration::from_secs(30);

/// Trích đường dẫn file audio + audio_codes từ chuỗi JSON lồng của
/// `TaskResult::inner()`.
///
/// Shape SERVER THẬT (xác nhận máy đo 2026-08-24) là MẢNG:
/// `[{"file": "/v1/audio?path=C%3A%5C…mp3", "status": 1, …}]`.
/// Vẫn chấp nhận object đơn + các key cũ (file/audio/audio_path/path/
/// files/outputs) để không vỡ với docs.
fn parse_result(inner: &str) -> Result<(String, Option<String>)> {
    let v: serde_json::Value = serde_json::from_str(inner)
        .map_err(|e| ProviderError::InvalidResponse(format!("result: {e}")))?;
    // Mảng → lấy phần tử đầu; object → dùng nguyên.
    let owned;
    let item = if v.is_array() {
        owned = v
            .as_array()
            .and_then(|a| a.first())
            .cloned()
            .ok_or_else(|| {
                ProviderError::InvalidResponse("result là mảng rỗng — không có output".into())
            })?;
        &owned
    } else {
        &v
    };

    // "file" trả dạng ENDPOINT đầy đủ kèm query đã encode:
    // "/v1/audio?path=C%3A%5C…mp3" — dùng NGUYÊN VĂN làm path request
    // (server tự decode query của nó; decode phía client sẽ phá query).
    let path = ["file", "audio", "audio_path", "path"]
        .iter()
        .find_map(|k| item.get(k).and_then(|x| x.as_str()))
        .map(str::to_owned)
        .or_else(|| {
            item.get("files")
                .or_else(|| item.get("outputs"))
                .and_then(|x| x.as_array())
                .and_then(|a| a.first())
                .and_then(|x| x.as_str())
                .map(str::to_owned)
        })
        .ok_or_else(|| {
            ProviderError::InvalidResponse(format!(
                "result không có đường dẫn audio: {}",
                &inner[..inner.len().min(256)]
            ))
        })?;
    let codes = item
        .get("audio_code_string")
        .or_else(|| item.get("audio_codes"))
        .and_then(|x| x.as_str())
        .map(str::to_owned);
    Ok((path, codes))
}

#[async_trait]
impl RenderProvider for PyProvider {
    fn id(&self) -> ProviderId {
        ProviderId(ProviderId::PY.to_owned())
    }

    fn capabilities(&self) -> &[Capability] {
        &self.caps
    }

    async fn health(&self) -> Result<Health> {
        match self.client.models().await {
            Ok(v) => Ok(Health {
                ready: true,
                vram_free_mb: None,
                loaded_models: vec![],
                detail: Some(v.to_string()),
            }),
            Err(e) => Ok(Health {
                ready: false,
                vram_free_mb: None,
                loaded_models: vec![],
                detail: Some(format!("acestep-api không trả lời: {e}")),
            }),
        }
    }

    async fn models(&self) -> Result<Vec<ModelDescriptor>> {
        // TODO(S-02): parse /v1/models thật. Tạm trả danh sách tĩnh theo tier
        // với checksum placeholder — cùng vấn đề checksum như CppProvider.
        Ok([
            ("acestep-v15-turbo", ModelTier::Turbo),
            ("acestep-v15-sft", ModelTier::Sft),
            ("acestep-v15-base", ModelTier::Base),
        ]
        .iter()
        .map(|(id, tier)| ModelDescriptor {
            id: ModelId((*id).to_owned()),
            tier: *tier,
            checksum: format!("todo-spike-{id}"),
            vram_estimate_mb: None,
            warm: false,
        })
        .collect())
    }

    async fn plan(&self, _input: PlanInput, _cx: JobCtx) -> Result<PlanOutput> {
        // Single-shot provider: không có endpoint plan riêng.
        Err(ProviderError::CapabilityNotSupported(
            Capability::SplitPlanRender,
        ))
    }

    async fn render(&self, input: RenderInput, cx: JobCtx) -> Result<RenderOutput> {
        if cx.cancel.is_cancelled() {
            return Err(ProviderError::Cancelled);
        }
        let model_id = model_id_for_tier(input.recipe.model_tier);
        let payload = build_release_payload(
            &input.recipe,
            model_id,
            Some(input.plan.audio_codes.as_str()),
            &*self.resolve_asset,
        )?;

        cx.report(2, ProgressStage::Queued).await;
        let task = self.client.release_task(&payload).await?;
        let started = Instant::now();

        // Poll query_result. Không có SSE/WebSocket — đây là lựa chọn duy nhất.
        let (path, codes) = loop {
            if cx.cancel.is_cancelled() {
                // Task phía server vẫn chạy tiếp — UI biết qua TooLate ở cancel().
                return Err(ProviderError::Cancelled);
            }
            if started.elapsed() > self.job_timeout {
                return Err(ProviderError::Timeout);
            }
            tokio::time::sleep(self.poll_interval).await;
            let res = self.client.query_result(&task.task_id).await?;
            match res.status() {
                TaskStatus::QueuedOrRunning => {
                    // Ước lượng thô: leo dần tới 95%, không bao giờ chạm 100
                    // cho tới khi thật sự xong — progress bar câm là lỗi UX.
                    let est = 60_000f64 + f64::from(input.recipe.duration_s) * 500.0;
                    let pct = ((started.elapsed().as_millis() as f64 / est) * 95.0).min(95.0) as u8;
                    cx.report(pct, ProgressStage::Rendering).await;
                }
                TaskStatus::Succeeded => {
                    let inner = res.inner().ok_or_else(|| {
                        ProviderError::InvalidResponse(
                            "succeeded nhưng thiếu result/result_json".into(),
                        )
                    })?;
                    break parse_result(inner)?;
                }
                TaskStatus::Failed => {
                    return Err(ProviderError::Worker(
                        res.error.unwrap_or_else(|| "task failed".into()),
                    ));
                }
                TaskStatus::Unknown(s) => {
                    return Err(ProviderError::InvalidResponse(format!("status lạ: {s}")));
                }
            }
        };

        let bytes = self.client.download_audio(&path).await?;
        if !bytes.starts_with(b"RIFF") {
            return Err(ProviderError::InvalidResponse(
                "file tải về không phải WAV RIFF".into(),
            ));
        }
        cx.report(100, ProgressStage::Rendering).await;
        Ok(RenderOutput {
            audio: AudioBlob {
                format: AudioFormat::WavF32,
                bytes,
            },
            seed_used: input.recipe.sampling.seed.unwrap_or(0),
            duration_ms: u64::from(input.recipe.duration_s) * 1000,
            audio_codes: codes,
        })
    }

    async fn understand(&self, _input: UnderstandInput, _cx: JobCtx) -> Result<AudioAnalysis> {
        // acestep-api 1.5 không expose understand endpoint riêng trong API.md —
        // understand là bài của cpp (`ace-understand`) hoặc LM 4B. Trung thực.
        Err(ProviderError::CapabilityNotSupported(
            Capability::Understand,
        ))
    }

    async fn warmup(&self, model: &ModelId, slot: Slot, cx: JobCtx) -> Result<()> {
        if cx.cancel.is_cancelled() {
            return Err(ProviderError::Cancelled);
        }
        cx.report(2, ProgressStage::Queued).await;
        let init = self.client.init_model(&model.0, slot.0).await?;
        // Xác nhận máy đo 2026-08-24: /v1/init ĐỒNG BỘ — HTTP response chỉ
        // trả khi model load xong (7.1s model nóng), KHÔNG có task_id.
        // → khi await dưới đây hoàn tất, "load xong" là SỰ THẬT: báo 100%
        // ngay. (Nhánh task_id giữ cho server tương lai trả handle async.)
        match init.get("task_id").and_then(|v| v.as_str()) {
            Some(task_id) => self.poll_warm_task(task_id, cx).await,
            None => {
                cx.report(100, ProgressStage::Planning).await;
                Ok(())
            }
        }
    }

    async fn cancel(&self, _job: &JobId) -> Result<CancelOutcome> {
        // Không có endpoint cancel → TooLate. Job đang poll sẽ dừng qua token.
        Ok(CancelOutcome::TooLate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::TaskResult;

    #[test]
    fn py_declares_no_split_and_no_cancel() {
        // Ghi RÕ kiểu: `AssetResolver` là `dyn Fn` (unsized), suy kiểu closure
        // từ `Arc::new(|_| None)` ngay tại vị trí tham số là không chắc chắn.
        // Annotate biến trước rồi truyền vào — cùng lớp lỗi với no_resolve().
        let resolve: Arc<AssetResolver> = Arc::new(|_: &als_core::AssetId| None);
        let p = PyProvider::new("http://127.0.0.1:8001", None, resolve);
        assert!(!p.capabilities().contains(&Capability::SplitPlanRender));
        assert!(!p.capabilities().contains(&Capability::CancelRunningJob));
        assert!(p.capabilities().contains(&Capability::Repaint));
        assert!(p.capabilities().contains(&Capability::Extract));
    }

    #[test]
    fn parse_result_tolerates_shapes() {
        let (path, codes) =
            parse_result(r#"{"file": "/out/a.wav", "audio_code_string": "FSQ:1"}"#).unwrap();
        assert_eq!(path, "/out/a.wav");
        assert_eq!(codes.as_deref(), Some("FSQ:1"));
        let (path, _) = parse_result(r#"{"files": ["/out/b.wav"]}"#).unwrap();
        assert_eq!(path, "/out/b.wav");
        assert!(parse_result(r#"{"nothing": 1}"#).is_err());
    }

    #[test]
    fn parses_real_server_array_shape() {
        // Shape thật từ máy đo 2026-08-24 (issue #14): MẢNG + file là
        // ENDPOINT đầy đủ kèm query percent-encoded — phải giữ nguyên văn.
        let raw = r#"[{"file": "/v1/audio?path=C%3A%5Ctmp%5Cout.mp3", "status": 1}] "#;
        let (path, _) = parse_result(raw).unwrap();
        assert_eq!(path, "/v1/audio?path=C%3A%5Ctmp%5Cout.mp3");
    }

    #[test]
    fn empty_result_array_is_error_not_panic() {
        // Server trả mảng rỗng khi không có output nào — lỗi rõ, không panic.
        assert!(parse_result("[]").is_err());
    }

    #[test]
    fn taskresult_inner_prefers_result_over_docs_name() {
        let with_both: TaskResult = serde_json::from_str(
            r#"{"task_id":"t","status":1,"result":"[1]","result_json":"[2]"}"#,
        )
        .unwrap();
        assert_eq!(with_both.inner(), Some("[1]"));
        let docs_only: TaskResult =
            serde_json::from_str(r#"{"task_id":"t","status":1,"result_json":"[2]"}"#).unwrap();
        assert_eq!(docs_only.inner(), Some("[2]"));
    }
}
