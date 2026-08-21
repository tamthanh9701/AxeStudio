#requires -Version 7.0
<#
.SYNOPSIS
  S-03 — đo warm generation time cho một ô trong ma trận backend × model × duration.

.EXAMPLE
  ./Bench-Matrix.ps1 -Backend py -Model acestep-v15-turbo -DurationS 120
  # → py,acestep-v15-turbo,120,14.3

.NOTES
  - Máy đo phải GHI LẠI cấu hình (GPU/VRAM/RAM/driver) vào spike-report trước khi đo.
  - Chạy ít nhất 1 lần warm-up trước khi đo (lần đầu nạp model = cold start, đo riêng ở S-04).
  - Backend cpp: gọi POST /lm rồi POST /synth?wav=1 (đúng 2 pha mà
    AceServerClient dùng — crates/als-provider-cpp/src/client.rs). Tên field
    payload chờ spike S-01 xác nhận; lệch thì sửa đúng khối `if ($Backend -eq "cpp")`.
#>
param(
  [Parameter(Mandatory)][ValidateSet("py", "cpp")] [string]$Backend,
  [Parameter(Mandatory)][string]$Model,
  [Parameter(Mandatory)][ValidateSet(30, 120, 240)] [int]$DurationS,
  [string]$BaseUrl = $(if ($Backend -eq "cpp") { "http://127.0.0.1:8080" } else { "http://127.0.0.1:8001" }),
  [int]$TimeoutMin = 20
)
$ErrorActionPreference = "Stop"

$steps = if ($Model -like "*turbo*") { 8 } else { 50 }
$body = @{
  task_type       = "text2music"
  model           = $Model
  prompt          = "benchmark: cinematic orchestral, strings, taiko"
  lyrics          = "[Instrumental]"
  audio_duration  = $DurationS
  audio_format    = "wav"
  inference_steps = $steps
  use_random_seed = $true
  batch_size      = 1
} | ConvertTo-Json

if ($Backend -eq "cpp") {
  # Stopwatch bấm từ TRƯỚC /lm đến khi nhận xong WAV — tổng wall của cả 2 pha,
  # tương đương cách nhánh py tính từ release_task đến kết quả.
  $sw = [System.Diagnostics.Stopwatch]::StartNew()
  $plan = Invoke-RestMethod -Method Post -Uri "$BaseUrl/lm" -Body $body -ContentType "application/json"
  if (-not $plan.audio_codes) { Write-Error "/lm không trả audio_codes: $($plan | ConvertTo-Json -Depth 4)" }
  $synthBody = $body | ConvertFrom-Json
  # Tên field `audio_codes` theo contract client.rs; ace-server đặt tên khác → sửa ĐÚNG CHỖ NÀY.
  $synthBody | Add-Member -NotePropertyName audio_codes -NotePropertyValue $plan.audio_codes
  $resp = Invoke-WebRequest -Method Post -Uri "$BaseUrl/synth?wav=1" `
    -Body ($synthBody | ConvertTo-Json -Depth 5) -ContentType "application/json"
  if (-not $resp.IsSuccessStatusCode) { Write-Error "/synth → HTTP $($resp.StatusCode)" }
  $riff = [System.Text.Encoding]::ASCII.GetString($resp.Content[0..3])
  if ($riff -ne "RIFF") { Write-Error "/synth trả về không phải WAV RIFF: '$riff'" }
  $sw.Stop()
  "{0},{1},{2},{3:N1}" -f $Backend, $Model, $DurationS, $sw.Elapsed.TotalSeconds
  return
}

$sw = [System.Diagnostics.Stopwatch]::StartNew()
$release = Invoke-RestMethod -Method Post -Uri "$BaseUrl/release_task" -Body $body -ContentType "application/json"
$taskId = $release.data.task_id
if (-not $taskId) { Write-Error "release_task không trả task_id: $($release | ConvertTo-Json -Depth 4)" }

do {
  Start-Sleep -Seconds 2
  if ($sw.Elapsed.TotalMinutes -gt $TimeoutMin) { Write-Error "timeout ${TimeoutMin}min chờ $taskId" }
  $q = (Invoke-RestMethod -Method Post -Uri "$BaseUrl/query_result" `
      -Body (@{ task_id_list = @($taskId) } | ConvertTo-Json) -ContentType "application/json").data[0]
} while ($q.status -eq 0)
$sw.Stop()

if ($q.status -ne 1) { Write-Error "task failed: $($q.error)" }
"{0},{1},{2},{3:N1}" -f $Backend, $Model, $DurationS, $sw.Elapsed.TotalSeconds
