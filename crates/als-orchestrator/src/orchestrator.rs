//! Orchestrator task + handle.

use crate::commands::OrchCommand;
use crate::error::{OrchError, Result};
use crate::events::OrchEvent;
use crate::registry::Registry;
use als_assets::AssetStore;
use als_core::{
    plan_hash, render_hash, EngineStatus, GenerationRecipe, JobId, JobKind, JobState, ProviderId,
    TakeId,
};
use als_project::{AssetRow, Db, JobRow, PlanCacheRow, TakeRow};
use als_provider::{
    CancelOutcome, Capability, JobCtx, ModelDescriptor, PlanInput, PlanOutput, Progress,
    ProviderError, RenderInput, RenderOutput, RenderProvider,
};
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_util::sync::CancellationToken;

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

type PhaseFuture = Pin<
    Box<dyn std::future::Future<Output = std::result::Result<(PlanOutput, RenderOutput), ProviderError>>
            + Send>,
>;

/// Job đang chạy: metadata để finish_job còn biết đường ghi cache/take.
struct InFlight {
    job_id: JobId,
    clip_id: String,
    recipe: GenerationRecipe,
    model: ModelDescriptor,
    plan_hash: String,
    plan_cache_hit: bool,
    cancel: CancellationToken,
    fut: PhaseFuture,
}

#[derive(serde::Deserialize)]
struct SubmitPayload {
    clip_id: String,
    recipe: GenerationRecipe,
}

/// Hai pha LM+DiT như MỘT future — với provider non-split (py) plan là opaque
/// và render() tự chạy cả pipeline.
async fn run_phases(
    provider: Arc<dyn RenderProvider>,
    recipe: GenerationRecipe,
    plan: Option<PlanOutput>,
    model: ModelDescriptor,
    job_id: JobId,
    cancel: CancellationToken,
    progress: mpsc::Sender<Progress>,
) -> std::result::Result<(PlanOutput, RenderOutput), ProviderError> {
    let split = provider.capabilities().contains(&Capability::SplitPlanRender);
    let plan = if split {
        match plan {
            Some(p) => p,
            None => {
                let cx = JobCtx {
                    job_id: job_id.clone(),
                    cancel: cancel.clone(),
                    progress: progress.clone(),
                };
                provider
                    .plan(
                        PlanInput {
                            recipe: recipe.clone(),
                            model: model.id.clone(),
                        },
                        cx,
                    )
                    .await?
            }
        }
    } else {
        PlanOutput::opaque()
    };
    let cx = JobCtx {
        job_id,
        cancel,
        progress,
    };
    let out = provider
        .render(
            RenderInput {
                recipe,
                plan: plan.clone(),
                model: model.id,
            },
            cx,
        )
        .await?;
    Ok((plan, out))
}

pub struct Orchestrator {
    rx: mpsc::Receiver<OrchCommand>,
    db: Db,
    assets: AssetStore,
    registry: Registry,
    events: broadcast::Sender<OrchEvent>,
    current: Option<InFlight>,
    progress_rx: Option<mpsc::Receiver<Progress>>,
}

/// Handle phía IPC (src-tauri). Clone tự do — mọi thứ qua channel.
#[derive(Clone)]
pub struct OrchestratorHandle {
    tx: mpsc::Sender<OrchCommand>,
    events: broadcast::Sender<OrchEvent>,
}

impl OrchestratorHandle {
    pub fn subscribe(&self) -> broadcast::Receiver<OrchEvent> {
        self.events.subscribe()
    }

    pub async fn submit_generate(
        &self,
        clip_id: String,
        recipe: GenerationRecipe,
        prio: i32,
    ) -> Result<JobId> {
        let (tx, rx) = oneshot::channel();
        self.send(OrchCommand::SubmitGenerate {
            clip_id,
            recipe,
            priority: prio,
            resp: tx,
        })?;
        rx.await
            .map_err(|_| OrchError::JobNotFound("orchestrator đã dừng".into()))?
    }

    pub async fn cancel(&self, job_id: JobId) -> Result<CancelOutcome> {
        let (tx, rx) = oneshot::channel();
        self.send(OrchCommand::Cancel { job_id, resp: tx })?;
        rx.await
            .map_err(|_| OrchError::JobNotFound("orchestrator đã dừng".into()))?
    }

    pub async fn engine_status(&self) -> Result<EngineStatus> {
        let (tx, rx) = oneshot::channel();
        self.send(OrchCommand::EngineStatus { resp: tx })?;
        rx.await
            .map_err(|_| OrchError::NoProvider("orchestrator đã dừng".into()))
    }

    pub async fn switch_backend(&self, provider: ProviderId) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.send(OrchCommand::SwitchBackend { provider, resp: tx })?;
        rx.await
            .map_err(|_| OrchError::NoProvider("orchestrator đã dừng".into()))?
    }

    pub async fn shutdown(&self) {
        let _ = self.tx.send(OrchCommand::Shutdown).await;
    }

    fn send(&self, cmd: OrchCommand) -> Result<()> {
        self.tx
            .try_send(cmd)
            .map_err(|_| OrchError::NoProvider("queue lệnh orchestrator đầy".into()))
    }
}

/// Spawn orchestrator task. `db` bị MOVE vào task — nơi khác cần đọc thì mở
/// connection thứ hai (WAL cho phép 1 writer + n reader trong cùng process).
pub fn spawn(
    db: Db,
    assets: AssetStore,
    providers: Vec<Arc<dyn RenderProvider>>,
    active: ProviderId,
) -> Result<OrchestratorHandle> {
    let registry = Registry::new(providers, active)?;
    let (cmd_tx, cmd_rx) = mpsc::channel(64);
    let (ev_tx, _) = broadcast::channel(256);
    let orch = Orchestrator {
        rx: cmd_rx,
        db,
        assets,
        registry,
        events: ev_tx.clone(),
        current: None,
        progress_rx: None,
    };
    tokio::spawn(orch.run());
    Ok(OrchestratorHandle {
        tx: cmd_tx,
        events: ev_tx,
    })
}

impl Orchestrator {
    fn emit(&self, ev: OrchEvent) {
        // Không có subscriber cũng không sao — broadcast::send trả Err, bỏ qua.
        let _ = self.events.send(ev);
    }

    fn set_job_state(&self, job_id: &JobId, state: JobState, error: Option<String>) {
        let _ = self.db.job_update_state(job_id, state, error.as_deref());
        self.emit(OrchEvent::JobState {
            job_id: job_id.clone(),
            state,
            error,
        });
    }

    async fn run(mut self) {
        loop {
            tokio::select! {
                maybe_cmd = self.rx.recv() => {
                    match maybe_cmd {
                        Some(OrchCommand::Shutdown) | None => break,
                        Some(cmd) => self.handle_command(cmd).await,
                    }
                }
                Some(p) = async {
                    // Guard bảo đảm Some mới poll.
                    self.progress_rx.as_mut().expect("guarded").recv().await
                }, if self.current.is_some() => {
                    self.emit(OrchEvent::Progress(p));
                }
                outcome = async {
                    // fut.as_mut() → Pin<&mut dyn Future> impl Future — không
                    // move future ra khỏi &mut borrow (E0507).
                    self.current.as_mut().expect("guarded").fut.as_mut().await
                }, if self.current.is_some() => {
                    self.finish_job(outcome).await;
                }
            }
            if self.current.is_none() {
                self.maybe_dispatch().await;
            }
        }
        tracing::info!("orchestrator dừng");
    }

    async fn handle_command(&mut self, cmd: OrchCommand) {
        match cmd {
            OrchCommand::SubmitGenerate {
                clip_id,
                recipe,
                priority: prio,
                resp,
            } => {
                let out = self.submit(clip_id, recipe, prio);
                let _ = resp.send(out);
            }
            OrchCommand::Cancel { job_id, resp } => {
                let out = self.cancel_job(&job_id).await;
                let _ = resp.send(out);
            }
            OrchCommand::EngineStatus { resp } => {
                let provider = self.registry.active_provider();
                let health = provider.health().await.ok();
                let depth = self.db.job_queue_depth().unwrap_or(0);
                let _ = resp.send(EngineStatus {
                    backend: self.registry.active_id(),
                    ready: health.as_ref().map(|h| h.ready).unwrap_or(false),
                    warm_models: health
                        .map(|h| h.loaded_models.iter().map(|m| m.0.clone()).collect())
                        .unwrap_or_default(),
                    vram_free_mb: None,
                    queue_depth: depth,
                });
            }
            OrchCommand::SwitchBackend { provider, resp } => {
                let _ = resp.send(self.registry.set_active(provider));
            }
            OrchCommand::Shutdown => {}
        }
    }

    fn submit(
        &mut self,
        clip_id: String,
        recipe: GenerationRecipe,
        prio: i32,
    ) -> Result<JobId> {
        // Validate ở biên — UI có thể gửi bậy, job không được vào queue.
        recipe.validate()?;
        let job_id = JobId::new();
        let payload = serde_json::to_string(&serde_json::json!({
            "clip_id": clip_id,
            "recipe": recipe,
        }))?;
        let now = now_unix();
        self.db.job_insert(&JobRow {
            id: job_id.to_string(),
            kind: serde_json::to_string(&JobKind::Render)?
                .trim_matches('"')
                .to_owned(),
            state: "queued".into(),
            priority: i64::from(prio),
            payload_json: payload,
            provider_id: Some(self.registry.active_id().to_string()),
            external_id: None,
            error: None,
            created_at: now,
            updated_at: now,
        })?;
        self.emit(OrchEvent::JobState {
            job_id: job_id.clone(),
            state: JobState::Queued,
            error: None,
        });
        Ok(job_id)
    }

    async fn cancel_job(&mut self, job_id: &JobId) -> Result<CancelOutcome> {
        // Đang chạy → nhờ provider quyết (cpp/py: TooLate trung thực).
        if let Some(cur) = &self.current {
            if &cur.job_id == job_id {
                cur.cancel.cancel();
                let provider = self.registry.active_provider();
                let outcome = provider.cancel(job_id).await.unwrap_or(CancelOutcome::TooLate);
                return Ok(outcome);
            }
        }
        // Còn nằm trong queue → huỷ sạch.
        match self.db.job_by_id(job_id.as_str())? {
            Some(row) if row.state == "queued" => {
                self.set_job_state(job_id, JobState::Cancelled, None);
                Ok(CancelOutcome::Cancelled)
            }
            _ => Err(OrchError::JobNotFound(job_id.to_string())),
        }
    }

    /// Nhặt job kế tiếp khi rảnh. Cache-hit tầng 2 xử lý NGAY tại đây (không
    /// chiếm slot in-flight), rồi nhặt tiếp — một lệnh submit trúng cache
    /// không được chặn job đang chờ khác.
    async fn maybe_dispatch(&mut self) {
        while self.current.is_none() {
            let job = match self.db.job_pick_next() {
                Ok(Some(j)) => j,
                Ok(None) => return,
                Err(e) => {
                    tracing::warn!(error = %e, "job_pick_next lỗi");
                    return;
                }
            };
            let job_id = JobId::from(job.id.clone());
            match self.prepare(&job).await {
                Ok(Preparation::Started(in_flight)) => {
                    self.current = Some(in_flight);
                }
                Ok(Preparation::CacheHitDone) => continue,
                Err(e) => {
                    tracing::warn!(error = %e, job = %job_id, "prepare job thất bại");
                    self.set_job_state(&job_id, JobState::Failed, Some(e.to_string()));
                    continue;
                }
            }
        }
    }

    async fn prepare(&mut self, job: &JobRow) -> Result<Preparation> {
        let job_id = JobId::from(job.id.clone());
        let payload: SubmitPayload = serde_json::from_str(&job.payload_json)?;
        payload.recipe.validate()?;

        let provider = self.registry.active_provider();
        let cap = Capability::for_task(payload.recipe.task);
        if !provider.capabilities().contains(&cap) {
            return Err(OrchError::Provider(
                ProviderError::CapabilityNotSupported(cap),
            ));
        }
        let models = provider.models().await?;
        let model = models
            .into_iter()
            .find(|m| m.tier == payload.recipe.model_tier)
            .ok_or_else(|| {
                OrchError::Provider(ProviderError::ModelMissing(format!(
                    "không có model tier {:?}",
                    payload.recipe.model_tier
                )))
            })?;

        // Tầng 2: render_hash.
        let rh = render_hash(
            &payload.recipe,
            &self.registry.active_id(),
            &model.id.0,
            &model.checksum,
        )?;
        if let Some(take) = self.db.take_by_render_hash(&rh.0)? {
            self.set_job_state(&job_id, JobState::Done, None);
            self.emit(OrchEvent::TakeReady {
                job_id,
                clip_id: payload.clip_id,
                take_id: TakeId::from(take.id),
                cached: true,
            });
            return Ok(Preparation::CacheHitDone);
        }

        // Tầng 1: plan_hash.
        let ph = plan_hash(&payload.recipe, &self.registry.active_id(), &model.id.0)?;
        let (plan_opt, plan_hit) = match self.db.plan_get(&ph.0)? {
            Some(row) => {
                self.db.plan_hit(&ph.0)?;
                (
                    Some(PlanOutput {
                        audio_codes: row.audio_codes,
                        lyrics: row.lyrics,
                        metas: serde_json::from_str(&row.metas_json)
                            .unwrap_or(serde_json::Value::Null),
                    }),
                    true,
                )
            }
            None => (None, false),
        };

        self.set_job_state(&job_id, JobState::Running, None);
        let (prog_tx, prog_rx) = mpsc::channel(64);
        let token = CancellationToken::new();
        let fut = Box::pin(run_phases(
            provider,
            payload.recipe.clone(),
            plan_opt,
            model.clone(),
            job_id.clone(),
            token.clone(),
            prog_tx,
        ));
        self.progress_rx = Some(prog_rx);
        Ok(Preparation::Started(InFlight {
            job_id,
            clip_id: payload.clip_id,
            recipe: payload.recipe,
            model,
            plan_hash: ph.0,
            plan_cache_hit: plan_hit,
            cancel: token,
            fut,
        }))
    }

    async fn finish_job(
        &mut self,
        outcome: std::result::Result<(PlanOutput, RenderOutput), ProviderError>,
    ) {
        let Some(cur) = self.current.take() else {
            return;
        };
        self.progress_rx = None;
        match outcome {
            Ok((plan, out)) => match self.postprocess(&cur, plan, out).await {
                Ok(take_id) => {
                    self.set_job_state(&cur.job_id, JobState::Done, None);
                    self.emit(OrchEvent::TakeReady {
                        job_id: cur.job_id.clone(),
                        clip_id: cur.clip_id.clone(),
                        take_id,
                        cached: false,
                    });
                }
                Err(e) => {
                    self.set_job_state(&cur.job_id, JobState::Failed, Some(e.to_string()));
                }
            },
            Err(ProviderError::Cancelled) => {
                self.set_job_state(&cur.job_id, JobState::Cancelled, None);
            }
            Err(e) => {
                self.set_job_state(&cur.job_id, JobState::Failed, Some(e.to_string()));
            }
        }
    }

    /// Render xong: backfill plan_cache → asset store → decode → loudness →
    /// peaks → take row. Lỗi postprocess = job failed (file đã sinh không bị
    /// nuốt — nó nằm trong store và tìm lại được bằng render_hash lần sau).
    async fn postprocess(
        &self,
        cur: &InFlight,
        plan: PlanOutput,
        out: RenderOutput,
    ) -> Result<TakeId> {
        // Backfill tầng 1: split provider có codes từ plan(); non-split (py)
        // lấy codes từ response render — re-roll seed lần sau sẽ bỏ qua LM.
        if !cur.plan_cache_hit {
            let codes = if !plan.audio_codes.is_empty() {
                Some(plan.audio_codes.clone())
            } else {
                out.audio_codes.clone()
            };
            if let Some(codes) = codes {
                self.db.plan_put(&PlanCacheRow {
                    plan_hash: cur.plan_hash.clone(),
                    provider_id: self.registry.active_id().to_string(),
                    model_id: cur.model.id.0.clone(),
                    audio_codes: codes,
                    lyrics: plan.lyrics.clone(),
                    metas_json: plan.metas.to_string(),
                    hits: 0,
                    created_at: now_unix(),
                })?;
            }
        }

        // Asset (dedupe theo nội dung).
        let (asset_id, rel) = self.assets.put(&out.audio.bytes, "wav")?;
        self.db.asset_put(&AssetRow {
            id: asset_id.to_string(),
            kind: "render".into(),
            rel_path: rel.clone(),
            bytes: out.audio.bytes.len() as i64,
            sample_rate: Some(48_000),
            channels: Some(2),
            duration_ms: Some(out.duration_ms as i64),
            created_at: now_unix(),
        })?;

        // Decode một lần cho loudness + peaks. Lỗi đo KHÔNG làm job fail —
        // take thiếu lufs vẫn dùng được, chỉ thiếu thông tin hiển thị.
        let abs = self.assets.abs_path(&rel);
        let decoded = als_media::decode::decode_file(&abs).ok();
        let (lufs, true_peak_db) = decoded
            .as_ref()
            .and_then(|b| als_media::loudness::measure(b).ok())
            .map(|l| (Some(l.lufs), Some(l.true_peak_db)))
            .unwrap_or((None, None));
        if let Some(buf) = &decoded {
            if let Ok(mm) = als_assets::peaks::PeakMipmap::compute(
                &buf.samples,
                buf.channels,
                buf.sample_rate,
            ) {
                // Peaks đặt theo đường dẫn DERIVE từ audio asset id — UI chỉ
                // cần biết audio asset id là tìm được peaks, không cần bảng phụ.
                if let Ok(peak_rel) = AssetStore::rel_path(&asset_id, "alspeak") {
                    if self.assets.put_named(&peak_rel, &mm.encode()).is_ok() {
                        self.emit(OrchEvent::PeaksReady {
                            asset_id: asset_id.clone(),
                        });
                    }
                }
            }
        }

        let take_id = TakeId::new();
        self.db.take_insert(&TakeRow {
            id: take_id.to_string(),
            clip_id: cur.clip_id.clone(),
            recipe_json: serde_json::to_string(&cur.recipe)?,
            plan_hash: cur.plan_hash.clone(),
            render_hash: render_hash(
                &cur.recipe,
                &self.registry.active_id(),
                &cur.model.id.0,
                &cur.model.checksum,
            )?
            .0,
            asset_id: asset_id.to_string(),
            lufs,
            true_peak_db,
            starred: false,
            created_at: now_unix(),
        })?;
        Ok(take_id)
    }
}

enum Preparation {
    Started(InFlight),
    CacheHitDone,
}
