//! Test orchestrator end-to-end với MockProvider + project tạm.
//! Db mở connection THỨ HAI để assert (WAL: 1 writer + n reader).

use als_assets::AssetStore;
use als_core::{priority, GenerationRecipe, JobState, ModelTier, ProviderId, SamplingParams, TaskType};
use als_orchestrator::{spawn, OrchEvent, OrchestratorHandle};
use als_project::{Db, Project};
use als_provider::MockProvider;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

fn recipe(seed: u64) -> GenerationRecipe {
    GenerationRecipe {
        prompt: "test tone".into(),
        lyrics: "[Verse]\nxin chào".into(),
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

struct Fixture {
    _dir: tempfile::TempDir,
    handle: OrchestratorHandle,
    db_path: std::path::PathBuf,
    assets_root: std::path::PathBuf,
}

fn fixture(render_delay_ms: u64) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("t.aiproj");
    let project = Project::create(&root, "t", "0.0.1").unwrap();
    let assets_root = project.layout.assets_dir();
    let assets = AssetStore::new(&assets_root).unwrap();
    let db_path = project.layout.db_path();

    let mut mock = MockProvider::new();
    mock.render_delay = Duration::from_millis(render_delay_ms);
    let handle = spawn(
        project.db,
        assets,
        vec![Arc::new(mock)],
        ProviderId(ProviderId::MOCK.to_owned()),
    )
    .unwrap();
    Fixture {
        _dir: dir,
        handle,
        db_path,
        assets_root,
    }
}

async fn wait_event(
    rx: &mut broadcast::Receiver<OrchEvent>,
    mut pred: impl FnMut(&OrchEvent) -> bool,
) -> OrchEvent {
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        assert!(!left.is_zero(), "timeout chờ event");
        let ev = tokio::time::timeout(left, rx.recv())
            .await
            .expect("timeout recv")
            .expect("channel đóng");
        if pred(&ev) {
            return ev;
        }
    }
}

#[tokio::test]
async fn generate_fresh_produces_take_asset_peaks() {
    let fx = fixture(50);
    let mut rx = fx.handle.subscribe();
    let job = fx
        .handle
        .submit_generate("clip-1".into(), recipe(1), priority::INTERACTIVE)
        .await
        .unwrap();

    wait_event(&mut rx, |e| {
        matches!(e, OrchEvent::JobState { job_id, state: JobState::Done, .. } if *job_id == job)
    })
    .await;

    // Take tồn tại + asset trên đĩa + peaks derive cùng id.
    let db = Db::open(&fx.db_path).unwrap();
    let takes = db.takes_for_clip("clip-1").unwrap();
    assert_eq!(takes.len(), 1);
    assert!(takes[0].lufs.is_some(), "postprocess phải đo LUFS");
    let asset = db
        .asset_get(&als_core::AssetId::from(takes[0].asset_id.clone()))
        .unwrap()
        .unwrap();
    assert!(fx.assets_root.join(&asset.rel_path).exists());
    let peak_rel =
        AssetStore::rel_path(&als_core::AssetId::from(takes[0].asset_id.clone()), "alspeak")
            .unwrap();
    assert!(fx.assets_root.join(peak_rel).exists(), "peaks phải được sinh");
    fx.handle.shutdown().await;
}

#[tokio::test]
async fn same_recipe_same_seed_hits_render_cache() {
    let fx = fixture(50);
    let mut rx = fx.handle.subscribe();
    let j1 = fx
        .handle
        .submit_generate("clip-1".into(), recipe(7), priority::INTERACTIVE)
        .await
        .unwrap();
    wait_event(&mut rx, |e| {
        matches!(e, OrchEvent::JobState { job_id, state: JobState::Done, .. } if *job_id == j1)
    })
    .await;

    // Cùng recipe + cùng seed → cache tầng 2 trúng, không chạy worker.
    let j2 = fx
        .handle
        .submit_generate("clip-2".into(), recipe(7), priority::INTERACTIVE)
        .await
        .unwrap();
    let ev = wait_event(&mut rx, |e| {
        matches!(e, OrchEvent::TakeReady { job_id, cached: true, .. } if *job_id == j2)
    })
    .await;
    match ev {
        OrchEvent::TakeReady { cached, .. } => assert!(cached),
        _ => unreachable!(),
    }
    fx.handle.shutdown().await;
}

#[tokio::test]
async fn reroll_seed_hits_plan_cache() {
    let fx = fixture(50);
    let mut rx = fx.handle.subscribe();
    let j1 = fx
        .handle
        .submit_generate("c1".into(), recipe(100), priority::INTERACTIVE)
        .await
        .unwrap();
    wait_event(&mut rx, |e| {
        matches!(e, OrchEvent::JobState { job_id, state: JobState::Done, .. } if *job_id == j1)
    })
    .await;

    // Đổi seed → plan_hash giữ nguyên → plan_cache.hits tăng đúng 1.
    let j2 = fx
        .handle
        .submit_generate("c1".into(), recipe(200), priority::INTERACTIVE)
        .await
        .unwrap();
    wait_event(&mut rx, |e| {
        matches!(e, OrchEvent::JobState { job_id, state: JobState::Done, .. } if *job_id == j2)
    })
    .await;

    let db = Db::open(&fx.db_path).unwrap();
    let mut found = false;
    // Duyệt plan_cache qua hits: phải có đúng một row hits == 1.
    let mut stmt = db
        .conn_for_test()
        .prepare("SELECT hits FROM plan_cache")
        .unwrap();
    let hits: Vec<i64> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    for h in hits {
        if h == 1 {
            found = true;
        }
    }
    assert!(found, "re-roll seed phải hit plan cache đúng một lần");
    fx.handle.shutdown().await;
}

#[tokio::test]
async fn cancel_queued_job_works() {
    let fx = fixture(400); // render chậm để job 1 chiếm slot
    let mut rx = fx.handle.subscribe();
    let j1 = fx
        .handle
        .submit_generate("c1".into(), recipe(1), priority::INTERACTIVE)
        .await
        .unwrap();
    let j2 = fx
        .handle
        .submit_generate("c2".into(), recipe(2), priority::INTERACTIVE)
        .await
        .unwrap();

    let outcome = fx.handle.cancel(j2.clone()).await.unwrap();
    assert!(matches!(outcome, als_provider::CancelOutcome::Cancelled));

    // j1 vẫn hoàn thành bình thường.
    wait_event(&mut rx, |e| {
        matches!(e, OrchEvent::JobState { job_id, state: JobState::Done, .. } if *job_id == j1)
    })
    .await;
    let db = Db::open(&fx.db_path).unwrap();
    let row = db.job_by_id(j2.as_str()).unwrap().unwrap();
    assert_eq!(row.state, "cancelled");
    fx.handle.shutdown().await;
}

#[tokio::test]
async fn invalid_recipe_rejected_at_boundary() {
    let fx = fixture(10);
    let mut bad = recipe(1);
    bad.duration_s = 5; // < 10 — validate phải chặn
    let err = fx
        .handle
        .submit_generate("c1".into(), bad, priority::INTERACTIVE)
        .await;
    assert!(err.is_err());
    fx.handle.shutdown().await;
}
