# Changelog

Mọi thay đổi đáng kể của AxeStudio được ghi tại đây.
Format: [Keep a Changelog](https://keepachangelog.com/vi/1.1.0/), version theo SemVer —
**tăng MAJOR khi đổi project schema** (ADR-004).

## [Unreleased]

### Added

- Bộ khung monorepo: 9 crate Rust + app Tauri 2 + packages TS (Sprint 1).
- Contract-first: schema SQLite 001, trait RenderProvider, bề mặt IPC (docs/contracts/ipc.md).
- Cache 2 tầng plan_hash/render_hash (ADR-003) + property tests.
- Audio engine skeleton: mixer/transport/lock-free commands, golden-buffer + no-alloc tests.
- Orchestrator: queue bền, 1 job in-flight, postprocess (LUFS + peaks), MockProvider.
- Phase 0 spike kit: docs/phase0/spike-report.md + scripts/phase0.

[Unreleased]: https://github.com/tamthanh9701/AxeStudio/compare/v0.0.0...HEAD
