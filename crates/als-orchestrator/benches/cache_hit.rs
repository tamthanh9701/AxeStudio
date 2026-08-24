//! Bench đường cache-hit tầng 2 đầy đủ (ngân sách `docs/perf-budget.md`:
//! cache_hit_to_audible_ms ≤ 200 — "cache hit → nghe được").
//!
//! harness=false — tự đo và in dòng bencher để scripts/collect-bench.mjs
//! parse (`test bench_cache_hit_path ... bench: N ns/iter`). ns/iter = thời
//! gian từ `submit_generate` đến event `TakeReady { cached: true }` — tức
//! toàn bộ prepare → tra render_hash → emit, KHÔNG đụng provider.
//!
//! MockProvider render_delay=20ms cho lần đầu (làm nóng cache); các lượt đo
//! chỉ trúng hit nên không có decode/postprocess nào can thiệp.

#![allow(clippy::disallowed_methods)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use als_assets::AssetStore;
use als_core::{priority, GenerationRecipe, ModelTier, ProviderId, SamplingParams, TaskType};
use als_orchestrator::{spawn, OrchEvent};
use als_project::Project;
use als_provider::MockProvider;

const ROUNDS: usize = 10;

fn recipe(seed: u64) -> GenerationRecipe {
    GenerationRecipe {
        prompt: "bench tone".into(),
        lyrics: "[Verse]\nbench".into(),
        duration_s: 10,
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
            seed: Some(seed),
            ..Default::default()
        },
        provider_overrides: Default::default(),
    }
}

async fn wait_take_ready(
    rx: &mut tokio::sync::broadcast::Receiver<OrchEvent>,
    deadline: Instant,
) -> Result<(), &'static str> {
    loop {
        if Instant::now() >= deadline {
            return Err("timeout chờ TakeReady");
        }
        match tokio::time::timeout_at(deadline.into(), rx.recv()).await {
            Ok(Ok(OrchEvent::TakeReady { .. })) => return Ok(()),
            Ok(Ok(_)) => continue,
            Ok(Err(_)) => return Err("channel đóng"),
            Err(_) => return Err("timeout recv"),
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let project = Project::create(&dir.path().join("b.aiproj"), "b", "0.0.1")?;
    let assets = AssetStore::new(project.layout.assets_dir())?;

    let mut mock = MockProvider::new();
    mock.render_delay = Duration::from_millis(20);
    let handle = spawn(
        project.db,
        assets,
        vec![Arc::new(mock)],
        ProviderId(ProviderId::MOCK.to_owned()),
    )?;

    // Lượt 1: render thật để có render_hash trong db.
    let mut rx = handle.subscribe();
    handle
        .submit_generate("c1".into(), recipe(7), priority::INTERACTIVE)
        .await?;
    wait_take_ready(&mut rx, Instant::now() + Duration::from_secs(15))
        .await
        .map_err(|e| format!("lượt đầu không hoàn tất: {e}"))?;

    // Đo ROUNDS lượt cache-hit: cùng recipe + seed → trúng tầng 2.
    let mut durations_ns: Vec<u128> = Vec::with_capacity(ROUNDS);
    for round in 0..ROUNDS {
        let t0 = Instant::now();
        handle
            .submit_generate(format!("clip{round}"), recipe(7), priority::INTERACTIVE)
            .await?;
        wait_take_ready(&mut rx, t0 + Duration::from_secs(15))
            .await
            .map_err(|e| format!("cache-hit lượt {round}: {e}"))?;
        durations_ns.push(t0.elapsed().as_nanos());
    }

    handle.shutdown().await;
    durations_ns.sort_unstable();
    let mean = durations_ns.iter().sum::<u128>() / durations_ns.len() as u128;

    println!(
        "rounds={ROUNDS} mean={mean}ns min={}ns max={}ns",
        durations_ns.first().copied().unwrap_or(0),
        durations_ns.last().copied().unwrap_or(0),
    );
    println!(
        "test bench_cache_hit_path ... bench: {:>11} ns/iter (+/- 0)",
        mean
    );

    Ok(())
}
