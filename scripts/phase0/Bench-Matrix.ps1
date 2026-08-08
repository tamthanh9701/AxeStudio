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
  - Backend cpp: TODO(S-01) sau khi build được ace-server — thêm nhánh gọi /synth.
#>
param(
  [Parameter(Mandatory)][ValidateSet("py")] [string]$Backend,
  [Parameter(Mandatory)][string]$Model,
  [Parameter(Mandatory)][ValidateSet(30, 120, 240)] [int]$DurationS,
  [string]$BaseUrl = "http://127.0.0.1:8001",
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
