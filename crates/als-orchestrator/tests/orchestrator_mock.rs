//! Test orchestrator end-to-end với MockProvider + project tạm.
//! Db mở connection THỨ HAI để assert (WAL: 1 writer + n reader).

use als_assets::AssetStore;
use als_core::{
    priority, GenerationRecipe, JobState, ModelTier, ProviderId, SamplingParams, TaskType,
};
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
    let peak_rel = AssetStore::rel_path(
        &als_core::AssetId::from(takes[0].asset_id.clone()),
        "alspeak",
    )
    .unwrap();
    assert!(
        fx.assets_root.join(peak_rel).exists(),
        "peaks phải được sinh"
    );
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
    wait_event(
        &mut rx,
        |e| matches!(e, OrchEvent::JobState { job_id, state: JobState::Done, .. } if *job_id == j1),
    )
    .await;

    // Cùng recipe + cùng seed → cache tầng 2 trúng, không chạy worker.
    let j2 = fx
        .handle
        .submit_generate("clip-2".into(), recipe(7), priority::INTERACTIVE)
        .await
        .unwrap();
    let ev = wait_event(
        &mut rx,
        |e| matches!(e, OrchEvent::TakeReady { job_id, cached: true, .. } if *job_id == j2),
    )
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
    wait_event(
        &mut rx,
        |e| matches!(e, OrchEvent::JobState { job_id, state: JobState::Done, .. } if *job_id == j1),
    )
    .await;

    // Đổi seed → plan_hash giữ nguyên → plan_cache.hits tăng đúng 1.
    let j2 = fx
        .handle
        .submit_generate("c1".into(), recipe(200), priority::INTERACTIVE)
        .await
        .unwrap();
    wait_event(
        &mut rx,
        |e| matches!(e, OrchEvent::JobState { job_id, state: JobState::Done, .. } if *job_id == j2),
    )
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
    wait_event(
        &mut rx,
        |e| matches!(e, OrchEvent::JobState { job_id, state: JobState::Done, .. } if *job_id == j1),
    )
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

#[tokio::test]
async fn warm_completes_with_prefixed_id_and_done() {
    let fx = fixture(0);
    let mut rx = fx.handle.subscribe();
    let job_id = fx.handle.warm(ModelTier::Turbo).await.unwrap();
    assert!(
        job_id.as_str().starts_with("warm:"),
        "job_id warm phải có tiền tố `warm:`, thấy {job_id}"
    );
    // Ít nhất một Progress (progress bar câm = lỗi UX, issue #14)...
    // LƯU Ý: KHÔNG assert Progress ở tầng orchestrator — warm instant có thể
    // hoàn thành trước khi vòng lặp kịp drain channel progress (buffer bị
    // bỏ sau khi job kết thúc). Tiến độ warm được bảo đảm ở TẦNG PROVIDER
    // bởi contract test `check_warmup_progress`.
    let ev = wait_event(&mut rx, |ev| {
        matches!(
            ev,
            OrchEvent::JobState {
                state: JobState::Done,
                ..
            }
        )
    })
    .await;
    match ev {
        OrchEvent::JobState { job_id: done, .. } => {
            assert_eq!(done, job_id, "Done phải thuộc về warm job")
        }
        other => panic!("event sai loại: {other:?}"),
    }
    fx.handle.shutdown().await;
}

#[tokio::test]
async fn warm_defers_until_render_slot_frees() {
    let fx = fixture(1500); // render giữ slot đủ lâu để quan sát thứ tự
    let mut rx = fx.handle.subscribe();
    let render_job = fx
        .handle
        .submit_generate("c1".into(), recipe(7), priority::INTERACTIVE)
        .await
        .unwrap();
    // Slot chắc chắn bận khi render đã Running.
    wait_event(&mut rx, |ev| {
        matches!(
            ev,
            OrchEvent::JobState {
                state: JobState::Running,
                ..
            }
        )
    })
    .await;
    let warm_id = fx.handle.warm(ModelTier::Turbo).await.unwrap();
    // Warm KHÔNG được chiếm slot: trạng thái đầu tiên khác Queued của nó
    // chỉ được xuất hiện SAU khi render Done.
    let mut render_done = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        assert!(!left.is_zero(), "timeout chờ warm/render");
        let ev = tokio::time::timeout(left, rx.recv())
            .await
            .expect("timeout recv")
            .expect("channel đóng");
        if let OrchEvent::JobState { job_id, state, .. } = ev {
            if job_id == render_job && state == JobState::Done {
                render_done = true;
            }
            if job_id == warm_id && state != JobState::Queued {
                assert!(
                    render_done,
                    "warm chạy trước khi render xong — phá nguyên tắc single-slot"
                );
                break;
            }
        }
    }
    fx.handle.shutdown().await;
}

#[tokio::test]
async fn engine_status_carries_capabilities_and_models() {
    let fx = fixture(0);
    let st = fx.handle.engine_status().await.unwrap();
    // ALS-F05 (#10): panel Generate đọc hai danh sách này thay vì hardcode.
    assert!(
        st.capabilities
            .contains(&als_provider::Capability::Text2Music),
        "mock phải tuyên bố Text2Music"
    );
    assert_eq!(st.models.len(), 3, "mock có đủ 3 tier");
    assert!(st.models.iter().any(|m| m.tier == ModelTier::Turbo));
    assert!(st.models.iter().all(|m| !m.checksum.is_empty()));
    fx.handle.shutdown().await;
}

#[tokio::test]
async fn cache_hit_across_reopen_creates_take_for_new_clip() {
    // BUG #3 (issue #14): tái hiện đúng chuỗi app — render thật ở phiên 1,
    // ĐÓNG project, mở lại ở phiên 2, submit clip MỚI cùng recipe → phải có
    // take row cho clip mới (cùng asset) + event TakeReady cached=true.
    let dir = tempfile::tempdir().unwrap();
    let proj_path = dir.path().join("p.aiproj");

    // --- Phiên 1: render thật ---
    let project = Project::create(&proj_path, "p", "0.0.1").unwrap();
    let assets_root = project.layout.assets_dir();
    let db_path = project.layout.db_path();
    {
        let mut mock = MockProvider::new();
        mock.render_delay = Duration::from_millis(20);
        let handle = spawn(
            project.db,
            AssetStore::new(&assets_root).unwrap(),
            vec![Arc::new(mock)],
            ProviderId(ProviderId::MOCK.to_owned()),
        )
        .unwrap();
        let mut rx = handle.subscribe();
        handle
            .submit_generate("clip-A".into(), recipe(7), priority::INTERACTIVE)
            .await
            .unwrap();
        wait_event(&mut rx, |ev| matches!(ev, OrchEvent::TakeReady { .. })).await;
        handle.shutdown().await;
    }

    // --- Phiên 2: mở lại (orchestrator + AssetStore + db mới, cùng file) ---
    let project = Project::open(&proj_path).unwrap();
    assert_eq!(project.layout.db_path(), db_path);
    {
        let mock = MockProvider::new(); // delay 0 — nếu MISS sẽ thấy qua timing? không cần: assert bên dưới quyết định
        let handle = spawn(
            project.db,
            AssetStore::new(&assets_root).unwrap(),
            vec![Arc::new(mock)],
            ProviderId(ProviderId::MOCK.to_owned()),
        )
        .unwrap();
        let mut rx = handle.subscribe();
        handle
            .submit_generate("clip-B".into(), recipe(7), priority::INTERACTIVE)
            .await
            .unwrap();
        let ev = wait_event(&mut rx, |ev| {
            matches!(ev, OrchEvent::TakeReady { cached: true, .. })
        })
        .await;
        match ev {
            OrchEvent::TakeReady {
                clip_id, take_id, ..
            } => {
                assert_eq!(clip_id, "clip-B");
                handle.shutdown().await;
                // Mở db riêng sau khi orchestrator đã nhả — project.db đã
                // move vào spawn ở trên.
                let db = Db::open(&db_path).unwrap();
                let takes = db.takes_for_clip("clip-B").unwrap();
                assert_eq!(takes.len(), 1, "clip B phải có ĐÚNG 1 take từ cache");
                assert_eq!(takes[0].id, take_id.to_string());
                let takes_a = db.takes_for_clip("clip-A").unwrap();
                assert_eq!(
                    takes[0].asset_id, takes_a[0].asset_id,
                    "hai take phải chia sẻ CÙNG asset — không copy audio"
                );
            }
            other => panic!("event sai loại: {other:?}"),
        }
    }
}
