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
  # acestep-api yêu cầu auth: body `ai_token` hoặc Authorization header (auth.py).
  [string]$ApiKey = "dev-local",
  # "" = để server dùng mặc định của nó; "pt"/"vllm" ép backend pha LM (S-02).
  [string]$LMBackend = "",
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
  batch_size      = 1
  ai_token        = $ApiKey
} | ConvertTo-Json
if ($LMBackend) {
  $bodyObj = $body | ConvertFrom-Json
  $bodyObj | Add-Member -NotePropertyName lm_backend -NotePropertyValue $LMBackend
  $body = $bodyObj | ConvertTo-Json
}

if ($Backend -eq "cpp") {
  # S-01 xác nhận contract thật của ace-server (src/request.cpp):
  #   caption (không phải prompt), duration (giây, không phải audio_duration),
  #   seed (số, không phải use_random_seed); KHÔNG có task_type/model/audio_format
  #   (mỗi process 1 model). Response /lm trả `audio_codes` — khớp client.rs.
  $sw = [System.Diagnostics.Stopwatch]::StartNew()
  $cppBody = @{
    caption         = "benchmark: cinematic orchestral, strings, taiko"
    lyrics          = "[Instrumental]"
    duration        = $DurationS
    inference_steps = $steps
    batch_size      = 1
    seed            = Get-Random -Maximum 2147483647
  } | ConvertTo-Json
  $plan = Invoke-RestMethod -Method Post -Uri "$BaseUrl/lm" -Body $cppBody -ContentType "application/json"
  if (-not $plan.audio_codes) { Write-Error "/lm không trả audio_codes: $($plan | ConvertTo-Json -Depth 4)" }
  # /synth nhận lại cùng body + audio_codes từ pha LM.
  $synthObj = $cppBody | ConvertFrom-Json
  $synthObj | Add-Member -NotePropertyName audio_codes -NotePropertyValue $plan.audio_codes
  $resp = Invoke-WebRequest -Method Post -Uri "$BaseUrl/synth?wav=1" `
    -Body ($synthObj | ConvertTo-Json) -ContentType "application/json"
  if ($resp.StatusCode -ge 400) { Write-Error "/synth → HTTP $($resp.StatusCode)" }
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
      -Body (@{ task_id_list = @($taskId); ai_token = $ApiKey } | ConvertTo-Json) -ContentType "application/json").data[0]
} while ($q.status -eq 0)
$sw.Stop()

if ($q.status -ne 1) { Write-Error "task failed: $($q.error)" }
"{0},{1},{2},{3:N1}" -f $Backend, $Model, $DurationS, $sw.Elapsed.TotalSeconds
