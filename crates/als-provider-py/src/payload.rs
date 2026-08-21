//! Map GenerationRecipe → payload /release_task.
//!
//! Tham số theo docs/en/API.md. `task_type` chỉ có 6 giá trị hợp lệ:
//! text2music, cover, repaint, lego, extract, complete — enum TaskType của
//! als-core serialize khớp nguyên xi.

use als_core::{GenerationRecipe, InferMethod, ModelTier, TaskType};
use std::path::PathBuf;

/// Tier → model id mà acestep-api nhận (tham số `model`).
pub fn model_id_for_tier(tier: ModelTier) -> &'static str {
    match tier {
        ModelTier::Turbo => "acestep-v15-turbo",
        ModelTier::Sft => "acestep-v15-sft",
        ModelTier::Base => "acestep-v15-base",
        ModelTier::XlTurbo => "acestep-v15-xl-turbo",
        ModelTier::XlSft => "acestep-v15-xl-sft",
        ModelTier::XlBase => "acestep-v15-xl-base",
    }
}

/// Resolver AssetId → path thật trên đĩa (server Python đọc file trực tiếp).
pub type AssetResolver = dyn Fn(&als_core::AssetId) -> Option<PathBuf> + Send + Sync;

pub fn build_release_payload(
    recipe: &GenerationRecipe,
    model_id: &str,
    audio_codes: Option<&str>,
    resolve: &AssetResolver,
) -> Result<serde_json::Value, als_provider::ProviderError> {
    let r = recipe;
    let s = &r.sampling;
    let ov = &r.provider_overrides;

    // thinking=true chỉ có nghĩa cho text2music/lego/complete; bị bỏ qua cho
    // cover/repaint/extract (docs API.md). Gửi đúng ngữ nghĩa để log đọc được.
    let thinking = matches!(
        r.task,
        TaskType::Text2Music | TaskType::Lego | TaskType::Complete
    );

    let mut p = serde_json::json!({
        "task_type": r.task,
        "model": model_id,
        "prompt": r.prompt,
        "lyrics": r.lyrics,
        "thinking": thinking,
        "audio_format": "wav",
        "audio_duration": r.duration_s,
        "inference_steps": s.inference_steps,
        "infer_method": match s.infer_method {
            InferMethod::Ode => "ode",
            InferMethod::Sde => "sde",
        },
        "batch_size": s.batch_size,
        "use_random_seed": s.seed.is_none(),
        // LM params — server bỏ qua khi thinking=false.
        "lm_temperature": 0.85,
        "lm_cfg_scale": 2.5,
        "lm_top_p": 0.9,
        "use_cot_caption": true,
        "use_cot_language": true,
        "constrained_decoding": true,
    });

    let obj = p.as_object_mut().expect("payload là object");

    if let Some(seed) = s.seed {
        obj.insert("seed".into(), seed.into());
    }
    if let Some(bpm) = r.bpm {
        obj.insert("bpm".into(), bpm.into());
    }
    if let Some(key) = &r.key_scale {
        obj.insert("key_scale".into(), key.clone().into());
    }
    if let Some(ts) = r.time_signature {
        obj.insert("time_signature".into(), ts.into());
    }
    if let Some(lang) = &r.vocal_language {
        obj.insert("vocal_language".into(), lang.clone().into());
    }
    // guidance_scale/shift chỉ có tác dụng trên base (Model Zoo).
    if matches!(r.model_tier, ModelTier::Base | ModelTier::XlBase) {
        if let Some(g) = s.guidance_scale {
            obj.insert("guidance_scale".into(), (g as f64).into());
        }
        if let Some(sh) = s.shift {
            obj.insert("shift".into(), (sh as f64).into());
        }
    }
    // audio_code_string: có sẵn → server BỎ QUA pha LM (đây là cơ chế làm
    // plan cache hữu dụng cả với provider single-shot).
    if let Some(codes) = audio_codes.filter(|c| !c.is_empty()) {
        obj.insert("audio_code_string".into(), codes.to_owned().into());
    }
    // Overrides có kiểm soát: chỉ whitelist 2 key đã biết, phần còn lại qua extra.
    if let Some(backend) = &ov.lm_backend {
        obj.insert("lm_backend".into(), backend.clone().into());
    }
    if let Some(lm) = &ov.lm_model {
        obj.insert("lm_model_path".into(), lm.clone().into());
    }
    for (k, v) in &ov.extra {
        obj.entry(k.clone()).or_insert_with(|| v.clone());
    }

    // Audio nguồn / tham chiếu — resolve AssetId → path qua orchestrator cung cấp.
    if let Some(src) = &r.source_audio {
        let path = resolve(src).ok_or_else(|| {
            als_provider::ProviderError::InvalidResponse(format!(
                "không resolve được source_audio asset {src}"
            ))
        })?;
        obj.insert(
            "src_audio_path".into(),
            path.to_string_lossy().into_owned().into(),
        );
    }
    if let Some(ref_audio) = &r.reference_audio {
        let path = resolve(ref_audio).ok_or_else(|| {
            als_provider::ProviderError::InvalidResponse(format!(
                "không resolve được reference_audio asset {ref_audio}"
            ))
        })?;
        obj.insert(
            "reference_audio_path".into(),
            path.to_string_lossy().into_owned().into(),
        );
    }
    // repaint: API dùng giây, recipe dùng ms.
    if let Some((start_ms, end_ms)) = r.repaint_range_ms {
        obj.insert("repainting_start".into(), (start_ms as f64 / 1000.0).into());
        obj.insert("repainting_end".into(), (end_ms as f64 / 1000.0).into());
    }

    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use als_core::{ProviderOverrides, SamplingParams};

    fn base_recipe() -> GenerationRecipe {
        GenerationRecipe {
            prompt: "lofi".into(),
            lyrics: "[Instrumental]".into(),
            duration_s: 30,
            bpm: Some(80),
            key_scale: None,
            time_signature: Some(4),
            vocal_language: None,
            task: TaskType::Text2Music,
            model_tier: ModelTier::Turbo,
            reference_audio: None,
            source_audio: None,
            repaint_range_ms: None,
            sampling: SamplingParams {
                seed: Some(1),
                ..Default::default()
            },
            provider_overrides: ProviderOverrides::default(),
        }
    }

    /// `AssetResolver` là type alias UNSIZED (`dyn Fn`), nên KHÔNG dùng được làm
    /// kiểu trả về: return type của hàm buộc phải `Sized` (E0277). Trả closure
    /// cụ thể qua `impl Fn` — tại call site `&no_resolve()` vẫn tự coerce thành
    /// `&AssetResolver`, nên 4 chỗ gọi bên dưới không phải sửa.
    ///
    /// Không dùng `Box<AssetResolver>` như rustc gợi ý: thêm một lần cấp phát vô
    /// ích, và lệch với pattern đã có trong chính file này (xem
    /// `repaint_ms_converted_to_seconds` — closure local rồi truyền `&resolve`).
    fn no_resolve() -> impl Fn(&als_core::AssetId) -> Option<PathBuf> + Send + Sync {
        |_: &als_core::AssetId| None
    }

    #[test]
    fn turbo_omits_guidance_and_shift() {
        // Turbo không dùng guidance_scale/shift — gửi lên là nhiễu log.
        let mut r = base_recipe();
        r.sampling.guidance_scale = Some(7.0);
        r.sampling.shift = Some(3.0);
        let p = build_release_payload(&r, "acestep-v15-turbo", None, &no_resolve()).unwrap();
        assert!(p.get("guidance_scale").is_none());
        assert!(p.get("shift").is_none());
    }

    #[test]
    fn base_includes_guidance_and_shift() {
        let mut r = base_recipe();
        r.model_tier = ModelTier::Base;
        r.sampling.guidance_scale = Some(7.0);
        r.sampling.shift = Some(3.0);
        let p = build_release_payload(&r, "acestep-v15-base", None, &no_resolve()).unwrap();
        assert_eq!(p["guidance_scale"], serde_json::json!(7.0));
        assert_eq!(p["shift"], serde_json::json!(3.0));
    }

    #[test]
    fn repaint_ms_converted_to_seconds() {
        let mut r = base_recipe();
        r.task = TaskType::Repaint;
        r.model_tier = ModelTier::Sft;
        r.repaint_range_ms = Some((10_000, 25_000));
        r.source_audio = Some(als_core::AssetId::from("x".repeat(64)));
        let resolve = |_: &als_core::AssetId| Some(PathBuf::from("/tmp/a.wav"));
        let p = build_release_payload(&r, "acestep-v15-sft", None, &resolve).unwrap();
        assert_eq!(p["repainting_start"], serde_json::json!(10.0));
        assert_eq!(p["repainting_end"], serde_json::json!(25.0));
        assert_eq!(p["src_audio_path"], serde_json::json!("/tmp/a.wav"));
        // Repaint bị server bỏ qua thinking.
        assert_eq!(p["thinking"], serde_json::json!(false));
    }

    #[test]
    fn cached_codes_skip_lm() {
        let r = base_recipe();
        let p = build_release_payload(&r, "m", Some("FSQ:cached"), &no_resolve()).unwrap();
        assert_eq!(p["audio_code_string"], serde_json::json!("FSQ:cached"));
    }

    #[test]
    fn no_seed_means_random() {
        let mut r = base_recipe();
        r.sampling.seed = None;
        let p = build_release_payload(&r, "m", None, &no_resolve()).unwrap();
        assert_eq!(p["use_random_seed"], serde_json::json!(true));
        assert!(p.get("seed").is_none());
    }
}
