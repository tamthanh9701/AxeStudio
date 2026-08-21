//! Property tests cho canonicalize + hash 2 tầng.
//! Đây là test hợp đồng của ADR-003 — fail ở đây nghĩa là cache đã bị phá.

use als_core::{
    canonicalize, normalize_line, plan_hash, render_hash, render_hash_with_version,
    GenerationRecipe, ModelTier, ProviderId, SamplingParams, TaskType,
};
use proptest::prelude::*;
use serde_json::Value;

fn provider() -> ProviderId {
    ProviderId(ProviderId::CPP.to_owned())
}

fn recipe_turbo() -> GenerationRecipe {
    GenerationRecipe {
        prompt: "epic cinematic orchestral, strings, taiko".into(),
        lyrics: "[Verse]\nTiếng trống vang đêm".into(),
        duration_s: 120,
        bpm: Some(90),
        key_scale: Some("D minor".into()),
        time_signature: Some(4),
        vocal_language: Some("vi".into()),
        task: TaskType::Text2Music,
        model_tier: ModelTier::Turbo,
        reference_audio: None,
        source_audio: None,
        repaint_range_ms: None,
        sampling: SamplingParams::default(),
        provider_overrides: Default::default(),
    }
}

#[test]
fn lyrics_nfc_nfd_same_plan_hash() {
    let mut a = recipe_turbo();
    a.lyrics = "Ti\u{1EBF}ng tr\u{1ED1}ng".into(); // NFC
    let mut b = recipe_turbo();
    b.lyrics = "Tie\u{0302}\u{0301}ng tro\u{0302}\u{0301}ng".into(); // NFD
    assert_eq!(
        plan_hash(&a, &provider(), "acestep-v15-turbo").unwrap(),
        plan_hash(&b, &provider(), "acestep-v15-turbo").unwrap()
    );
}

#[test]
fn whitespace_noise_same_plan_hash() {
    let mut a = recipe_turbo();
    a.prompt = "epic cinematic orchestral, strings, taiko".into();
    let mut b = recipe_turbo();
    b.prompt = "  epic   cinematic \n\torchestral,   strings, taiko  ".into();
    assert_eq!(
        plan_hash(&a, &provider(), "m").unwrap(),
        plan_hash(&b, &provider(), "m").unwrap()
    );
}

#[test]
fn change_seed_keeps_plan_changes_render() {
    let mut a = recipe_turbo();
    a.sampling.seed = Some(42);
    let mut b = recipe_turbo();
    b.sampling.seed = Some(1337);

    let p = provider();
    assert_eq!(
        plan_hash(&a, &p, "m").unwrap(),
        plan_hash(&b, &p, "m").unwrap(),
        "đổi seed không được làm mất hiệu lực plan cache"
    );
    assert_ne!(
        render_hash(&a, &p, "m", "ck").unwrap(),
        render_hash(&b, &p, "m", "ck").unwrap()
    );
}

#[test]
fn change_prompt_changes_both() {
    let a = recipe_turbo();
    let mut b = recipe_turbo();
    b.prompt = "lofi chill, rain, vinyl crackle".into();

    let p = provider();
    assert_ne!(
        plan_hash(&a, &p, "m").unwrap(),
        plan_hash(&b, &p, "m").unwrap()
    );
    assert_ne!(
        render_hash(&a, &p, "m", "ck").unwrap(),
        render_hash(&b, &p, "m", "ck").unwrap()
    );
}

#[test]
fn change_pipeline_version_changes_render_not_plan() {
    let a = recipe_turbo();
    let p = provider();
    let v1 = render_hash_with_version(&a, &p, "m", "ck", 1).unwrap();
    let v2 = render_hash_with_version(&a, &p, "m", "ck", 2).unwrap();
    assert_ne!(v1, v2);
}

#[test]
fn change_model_checksum_changes_render() {
    // Đổi quant Q8_0 → Q4_K_M đổi checksum → không được trả nhầm cache cũ.
    let a = recipe_turbo();
    let p = provider();
    assert_ne!(
        render_hash(&a, &p, "m", "q8_checksum").unwrap(),
        render_hash(&a, &p, "m", "q4_checksum").unwrap()
    );
}

#[test]
fn cross_provider_no_cache_collision() {
    let a = recipe_turbo();
    let cpp = ProviderId(ProviderId::CPP.to_owned());
    let py = ProviderId(ProviderId::PY.to_owned());
    assert_ne!(
        plan_hash(&a, &cpp, "m").unwrap(),
        plan_hash(&a, &py, "m").unwrap(),
        "hai provider không được đọc nhầm plan cache của nhau"
    );
}

#[test]
fn float_rounding_unifies_close_values() {
    let mut a = recipe_turbo();
    a.sampling.guidance_scale = Some(0.85);
    let mut b = recipe_turbo();
    b.sampling.guidance_scale = Some(0.8500001);
    let p = provider();
    assert_eq!(
        render_hash(&a, &p, "m", "ck").unwrap(),
        render_hash(&b, &p, "m", "ck").unwrap()
    );
}

fn arb_json() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        any::<bool>().prop_map(Value::from),
        any::<i64>().prop_map(Value::from),
        // Giới hạn biên độ để test nhanh và tránh in số khổng lồ.
        (-1e9f64..1e9f64).prop_map(|f| Value::from(serde_json::Number::from_f64(f).unwrap())),
        "[a-zA-Z0-9 À-ỹ]{0,32}".prop_map(Value::from),
    ];
    leaf.prop_recursive(4, 64, 8, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..8).prop_map(Value::from),
            prop::collection::btree_map("[a-z_]{1,8}", inner, 0..8)
                .prop_map(|m| Value::Object(m.into_iter().collect())),
        ]
    })
}

proptest! {
    #[test]
    fn canonicalize_idempotent(v in arb_json()) {
        let once = canonicalize(&v).unwrap();
        let parsed: Value = serde_json::from_str(&once).unwrap();
        prop_assert_eq!(once, canonicalize(&parsed).unwrap());
    }

    #[test]
    fn prompt_whitespace_variants_hash_equal(
        words in prop::collection::vec("[a-z]{1,12}", 1..8),
        seps in prop::collection::vec("[ \\t\\n]{1,4}", 1..8),
    ) {
        let mut messy = String::new();
        for (i, w) in words.iter().enumerate() {
            if i > 0 {
                messy.push_str(&seps[i % seps.len()]);
            }
            messy.push_str(w);
        }
        let clean = normalize_line(&messy);

        let mut a = recipe_turbo();
        a.prompt = messy;
        let mut b = recipe_turbo();
        b.prompt = clean;

        let p = provider();
        prop_assert_eq!(
            plan_hash(&a, &p, "m").unwrap(),
            plan_hash(&b, &p, "m").unwrap()
        );
    }

    #[test]
    fn any_seed_change_preserves_plan(seed_a in any::<u64>(), seed_b in any::<u64>()) {
        let mut a = recipe_turbo();
        a.sampling.seed = Some(seed_a);
        let mut b = recipe_turbo();
        b.sampling.seed = Some(seed_b);
        let p = provider();
        prop_assert_eq!(
            plan_hash(&a, &p, "m").unwrap(),
            plan_hash(&b, &p, "m").unwrap()
        );
        if seed_a != seed_b {
            prop_assert_ne!(
                render_hash(&a, &p, "m", "ck").unwrap(),
                render_hash(&b, &p, "m", "ck").unwrap()
            );
        }
    }
}
