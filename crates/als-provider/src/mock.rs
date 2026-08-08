//! MockProvider — trả kết quả deterministic, dev UI không cần GPU.
//!
//! - plan(): audio_codes = blake3(plan_hash) — recipe khác nhau → codes khác nhau.
//! - render(): sine WAV 48kHz stereo f32, tần số suy ra từ seed →
//!   cùng recipe + cùng seed → cùng bytes (test determinism dựa vào đây).

use crate::error::{ProviderError, Result};
use crate::provider::{JobCtx, RenderProvider};
use crate::types::{
    AudioAnalysis, AudioBlob, AudioFormat, CancelOutcome, Capability, Health, ModelDescriptor,
    ModelId, PlanInput, PlanOutput, ProgressStage, RenderInput, RenderOutput, Slot,
    UnderstandInput,
};
use als_core::{plan_hash, JobId, ModelTier, ProviderId};
use async_trait::async_trait;
use std::io::Cursor;
use std::time::Duration;

pub struct MockProvider {
    /// Delay giả lập từng pha — để UI thấy được trạng thái đang chạy.
    pub plan_delay: Duration,
    pub render_delay: Duration,
    caps: Vec<Capability>,
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl MockProvider {
    pub fn new() -> Self {
        Self {
            plan_delay: Duration::from_millis(50),
            render_delay: Duration::from_millis(100),
            caps: vec![
                Capability::Text2Music,
                Capability::Cover,
                Capability::Repaint,
                Capability::Lego,
                Capability::Extract,
                Capability::Complete,
                Capability::Understand,
                Capability::HotSwapModel,
                Capability::CancelRunningJob,
                Capability::SplitPlanRender,
            ],
        }
    }
}

/// Ngủ có thể bị huỷ giữa chừng.
async fn interruptible_sleep(
    d: Duration,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<()> {
    tokio::select! {
        _ = tokio::time::sleep(d) => Ok(()),
        _ = cancel.cancelled() => Err(ProviderError::Cancelled),
    }
}

fn synth_wav(duration_ms: u64, seed: u64) -> Vec<u8> {
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 48_000,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut w = hound::WavWriter::new(&mut cursor, spec).expect("wav writer");
        let frames = (duration_ms * 48) as usize; // 48000 frames/s
        let freq = 220.0 + (seed % 440) as f32;
        for i in 0..frames {
            let t = i as f32 / 48_000.0;
            let env = 1.0 - (i as f32 / frames.max(1) as f32) * 0.5;
            let s = (2.0 * std::f32::consts::PI * freq * t).sin() * 0.2 * env;
            w.write_sample::<f32>(s).expect("sample L");
            w.write_sample::<f32>(s * 0.98).expect("sample R");
        }
        w.finalize().expect("finalize");
    }
    cursor.into_inner()
}

#[async_trait]
impl RenderProvider for MockProvider {
    fn id(&self) -> ProviderId {
        ProviderId(ProviderId::MOCK.to_owned())
    }

    fn capabilities(&self) -> &[Capability] {
        &self.caps
    }

    async fn health(&self) -> Result<Health> {
        Ok(Health {
            ready: true,
            vram_free_mb: Some(u64::MAX / 1_048_576), // mock không dùng VRAM
            loaded_models: vec![ModelId("mock-turbo".into())],
            detail: Some("mock provider — không có engine thật".into()),
        })
    }

    async fn models(&self) -> Result<Vec<ModelDescriptor>> {
        Ok(vec![
            ModelDescriptor {
                id: ModelId("mock-turbo".into()),
                tier: ModelTier::Turbo,
                checksum: "mock-checksum-turbo".into(),
                vram_estimate_mb: Some(0),
                warm: true,
            },
            ModelDescriptor {
                id: ModelId("mock-sft".into()),
                tier: ModelTier::Sft,
                checksum: "mock-checksum-sft".into(),
                vram_estimate_mb: Some(0),
                warm: true,
            },
            ModelDescriptor {
                id: ModelId("mock-base".into()),
                tier: ModelTier::Base,
                checksum: "mock-checksum-base".into(),
                vram_estimate_mb: Some(0),
                warm: true,
            },
        ])
    }

    async fn plan(&self, input: PlanInput, cx: JobCtx) -> Result<PlanOutput> {
        cx.report(10, ProgressStage::Planning).await;
        interruptible_sleep(self.plan_delay, &cx.cancel).await?;
        let ph = plan_hash(&input.recipe, &self.id(), &input.model.0)
            .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;
        cx.report(100, ProgressStage::Planning).await;
        Ok(PlanOutput {
            audio_codes: format!("MOCKCODES:{}", ph.0),
            lyrics: if input.recipe.lyrics.trim().is_empty() {
                Some("[Verse]\nMock lyric line one\n[Chorus]\nMock lyric line two".into())
            } else {
                Some(input.recipe.lyrics.clone())
            },
            metas: serde_json::json!({
                "bpm": input.recipe.bpm,
                "key_scale": input.recipe.key_scale,
                "mock": true,
            }),
        })
    }

    async fn render(&self, input: RenderInput, cx: JobCtx) -> Result<RenderOutput> {
        if cx.cancel.is_cancelled() {
            return Err(ProviderError::Cancelled);
        }
        cx.report(0, ProgressStage::Rendering).await;
        interruptible_sleep(self.render_delay, &cx.cancel).await?;

        let seed = input.recipe.sampling.seed.unwrap_or_else(|| {
            // Deterministic: suy seed từ audio_codes.
            let h = blake3::hash(input.plan.audio_codes.as_bytes());
            u64::from_le_bytes(h.as_bytes()[..8].try_into().expect("8 bytes"))
        });
        let duration_ms = u64::from(input.recipe.duration_s) * 1000;
        let bytes = synth_wav(duration_ms, seed);

        cx.report(100, ProgressStage::Rendering).await;
        Ok(RenderOutput {
            audio: AudioBlob {
                format: AudioFormat::WavF32,
                bytes,
            },
            seed_used: seed,
            duration_ms,
            // Echo codes lại — provider thật (py) trả codes trong result json.
            audio_codes: if input.plan.audio_codes.is_empty() {
                None
            } else {
                Some(input.plan.audio_codes.clone())
            },
        })
    }

    async fn understand(&self, input: UnderstandInput, cx: JobCtx) -> Result<AudioAnalysis> {
        cx.report(50, ProgressStage::Queued).await;
        if !input.audio_path.exists() {
            return Err(ProviderError::InvalidResponse(format!(
                "file không tồn tại: {}",
                input.audio_path.display()
            )));
        }
        Ok(AudioAnalysis {
            caption: "mock caption: sine sweep".into(),
            bpm: Some(120.0),
            key_scale: Some("C major".into()),
            duration_ms: None,
            extra: serde_json::json!({ "mock": true }),
        })
    }

    async fn warmup(&self, _model: &ModelId, _slot: Slot) -> Result<()> {
        Ok(())
    }

    async fn cancel(&self, _job: &JobId) -> Result<CancelOutcome> {
        // Mock không giữ trạng thái dispatch → job nào cũng huỷ được.
        Ok(CancelOutcome::Cancelled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synth_wav_deterministic() {
        let a = synth_wav(1000, 42);
        let b = synth_wav(1000, 42);
        assert_eq!(a, b, "cùng duration + seed phải ra cùng bytes");
        let c = synth_wav(1000, 43);
        assert_ne!(a, c);
        assert!(a.starts_with(b"RIFF"), "output phải là WAV hợp lệ");
    }
}
