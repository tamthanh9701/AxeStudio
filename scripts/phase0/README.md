# Phase 0 — Spike scripts

Các script này chạy trên **máy thật có GPU**, không chạy trên CI. Kết quả điền
vào `docs/phase0/spike-report.md`. Không có số → không mở Sprint 1.

| Script | Spike | Việc |
| --- | --- | --- |
| `Bench-Matrix.ps1` | S-03 | Đo warm generation theo ma trận model × duration |
| `../Model-Fetch.ps1` | — | Tải model HF có resume + verify SHA256 (S7 dùng lại) |

## Chuẩn bị

```powershell
# Backend Python (S-02)
git clone https://github.com/ace-step/ACE-Step-1.5
cd ACE-Step-1.5; uv sync
$env:ACESTEP_API_KEY = "dev-local"
uv run acestep-api   # http://127.0.0.1:8001

# Backend C++ (S-01) — xem lệnh build trong build plan mục 3.2
```

## Chạy ma trận (S-03)

```powershell
foreach ($m in @("acestep-v15-turbo", "acestep-v15-sft")) {
  foreach ($d in @(30, 120, 240)) {
    ./scripts/phase0/Bench-Matrix.ps1 -Backend py -Model $m -DurationS $d
  }
}
# → in 1 dòng CSV mỗi lần: backend,model,duration,seconds — dán vào spike report.
```

## Kill criteria nhắc lại

- Warm gen 120s > 30s → bỏ UX "như nhạc cụ", chuyển render queue + notification.
- Cold start > 60s → bắt buộc warm model lúc mở project.
- vLLM không native + `pt` chậm > 2× → cpp là backend duy nhất v1.
- Vulkan build fail → "NVIDIA only" ở v1.
