# Roadmap

Bản đầy đủ nằm ở build plan (Notion). File này là bản tóm tắt công khai để repo
tự giải thích được mình đang ở đâu.

- **Phase 0 (W1–W2) — Spike:** 8 spike S-01…S-08, đo số thật, điền
  `docs/phase0/spike-report.md`. Kill criteria trong đó. → xem Issues.
- **Phase 1 (W3–W16) — MVP:** 7 sprint × 2 tuần, bàn giao `.msi` ký số:
  generate → export, không chạm terminal.
- **Phase 2 (W17–W26):** controlled editing — repaint UI, stem separation, understand.
- **Phase 3 (W27–W34):** LoRA library + training.
- **Phase 4 (TBD):** vocal synth provider (phụ thuộc engine bên thứ ba).

## Nguyên tắc không đổi

1. Không bắt đầu Phase 1 khi Phase 0 chưa có số.
2. Ngân sách hiệu năng (docs/perf-budget.md) là điều kiện merge.
3. Contract đổi → PR riêng chỉ chứa contract + ADR.
