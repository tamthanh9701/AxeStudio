//! BỘ TEST HỢP ĐỒNG DÙNG CHUNG.
//!
//! Mọi provider (cpp, py, mock, và cả provider tương lai) phải gọi `run_all`
//! trong test của chính nó. Fail ở đây = lỗi ở provider, KHÔNG phải orchestrator.
//!
//! Cách dùng:
//! ```ignore
//! #[tokio::test]
//! async fn my_provider_passes_contract() {
//!     let p = MyProvider::connect(...);
//!     als_provider::contract::run_all(&p).await;
//! }
//! ```

use crate::provider::{JobCtx, RenderProvider};
use crate::types::{Capability, PlanInput, PlanOutput, Progress, RenderInput, UnderstandInput};
use als_core::{GenerationRecipe, JobId, ModelTier, SamplingParams, TaskType};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

fn test_recipe() -> GenerationRecipe {
    GenerationRecipe {
        prompt: "contract test: soft piano".into(),
        lyrics: "[Verse]\nhello contract".into(),
        duration_s: 10, // tối thiểu theo validate() — đừng tăng, test phải nhanh
        bpm: Some(100),
        key_scale: None,
        time_signature: Some(4),
        vocal_language: None,
        task: TaskType::Text2Music,
        model_tier: ModelTier::Turbo,
        reference_audio: None,
        source_audio: None,
        repaint_range_ms: None,
        sampling: SamplingParams {
            seed: Some(7),
            ..Default::default()
        },
        provider_overrides: Default::default(),
    }
}

fn ctx(cancel: CancellationToken) -> (JobCtx, mpsc::Receiver<Progress>) {
    let (tx, rx) = mpsc::channel(64);
    (
        JobCtx {
            job_id: JobId::new(),
            cancel,
            progress: tx,
        },
        rx,
    )
}

async fn first_model(p: &dyn RenderProvider) -> crate::types::ModelId {
    let models = p.models().await.expect("models() phải Ok khi health.ready");
    assert!(
        !models.is_empty(),
        "models() rỗng — provider không có model nào?"
    );
    for m in &models {
        assert!(
            !m.checksum.is_empty(),
            "model {} thiếu checksum (đi vào render_hash)",
            m.id
        );
    }
    models[0].id.clone()
}

pub async fn check_id_and_caps(p: &dyn RenderProvider) {
    assert!(!p.id().as_str().is_empty(), "provider id rỗng");
    assert!(!p.capabilities().is_empty(), "capabilities rỗng");
}

pub async fn check_health(p: &dyn RenderProvider) {
    let h = p
        .health()
        .await
        .expect("health() phải trả lời được, kể cả khi chưa ready");
    if h.ready {
        // ready mà không có model nào là config sai.
        let models = p.models().await.expect("models() lỗi khi health.ready");
        assert!(!models.is_empty(), "health.ready nhưng models() rỗng");
    }
}

/// plan() → render() roundtrip.
///
/// Provider có SplitPlanRender (cpp): plan() phải trả audio_codes không rỗng,
/// render() nhận codes đó.
/// Provider non-split (py): bỏ qua plan(), render() với PlanOutput::opaque()
/// phải tự chạy cả hai pha và trả WAV hợp lệ.
pub async fn check_plan_render_roundtrip(p: &dyn RenderProvider) {
    let recipe = test_recipe();
    let model = first_model(p).await;

    let plan = if p.capabilities().contains(&Capability::SplitPlanRender) {
        let (cx, _rx) = ctx(CancellationToken::new());
        let plan = p
            .plan(
                PlanInput {
                    recipe: recipe.clone(),
                    model: model.clone(),
                },
                cx,
            )
            .await
            .expect("plan() thất bại với recipe tối thiểu");
        assert!(!plan.audio_codes.is_empty(), "plan() trả audio_codes rỗng");
        plan
    } else {
        PlanOutput::opaque()
    };

    let (cx, _rx) = ctx(CancellationToken::new());
    let out = p
        .render(
            RenderInput {
                recipe,
                plan,
                model,
            },
            cx,
        )
        .await
        .expect("render() thất bại");
    assert!(!out.audio.bytes.is_empty(), "render() trả audio rỗng");
    assert!(
        out.audio.bytes.starts_with(b"RIFF"),
        "render() output không phải WAV RIFF — orchestrator kỳ vọng WAV 48k stereo f32"
    );
    assert!(out.duration_ms > 0, "duration_ms phải > 0");
}

/// Token đã huỷ TRƯỚC khi gọi → provider phải Err(Cancelled), không được chạy tiếp.
pub async fn check_cancel_before_dispatch(p: &dyn RenderProvider) {
    let recipe = test_recipe();
    let model = first_model(p).await;

    let cancel = CancellationToken::new();
    cancel.cancel();
    let (cx, _rx) = ctx(cancel);
    let result = p
        .render(
            RenderInput {
                recipe,
                plan: PlanOutput::opaque(),
                model,
            },
            cx,
        )
        .await;
    match result {
        Err(crate::error::ProviderError::Cancelled) => {}
        other => panic!("render() với token đã huỷ phải Err(Cancelled), nhận: {other:?}"),
    }
}

/// cancel() job không tồn tại: được trả TooLate hoặc Err, CẤM panic / treo.
pub async fn check_cancel_unknown_job(p: &dyn RenderProvider) {
    let unknown = JobId::new();
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), p.cancel(&unknown))
        .await
        .expect("cancel() job lạ bị treo");
}

/// understand(): chỉ kiểm khi provider tuyên bố có capability Understand.
pub async fn check_understand_if_supported(p: &dyn RenderProvider) {
    if !p.capabilities().contains(&Capability::Understand) {
        return;
    }
    // Render 10s audio thật để làm input.
    let recipe = test_recipe();
    let model = first_model(p).await;
    let (cx, _rx) = ctx(CancellationToken::new());
    let out = p
        .render(
            RenderInput {
                recipe,
                plan: PlanOutput::opaque(),
                model,
            },
            cx,
        )
        .await
        .expect("render cho understand");

    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("contract.wav");
    std::fs::write(&path, &out.audio.bytes).expect("ghi wav tạm");

    let (cx, _rx) = ctx(CancellationToken::new());
    let analysis = p
        .understand(
            UnderstandInput {
                audio_path: path,
                question: None,
            },
            cx,
        )
        .await
        .expect("understand() thất bại dù tuyên bố capability");
    assert!(
        !analysis.caption.is_empty(),
        "understand() trả caption rỗng"
    );
}

/// Chạy toàn bộ suite. Thứ tự có chủ đích: check rẻ trước, check đắt sau.
pub async fn run_all(p: &dyn RenderProvider) {
    check_id_and_caps(p).await;
    check_health(p).await;
    check_plan_render_roundtrip(p).await;
    check_cancel_before_dispatch(p).await;
    check_cancel_unknown_job(p).await;
    check_understand_if_supported(p).await;
}
