#requires -Version 7.0
<#
.SYNOPSIS
  S-04 — đo cold start, peak VRAM và peak RAM của một backend trong lúc render.

.EXAMPLE
  # Python backend — spawn qua uv trong thư mục ACE-Step-1.5 đã clone NGOÀI repo:
  ./Measure-ColdVram.ps1 -Backend py -Model acestep-v15-turbo `
    -ExePath uv -ArgList run,acestep-api -WorkDir D:\ACE-Step-1.5

  # C++ backend:
  ./Measure-ColdVram.ps1 -Backend cpp -Model acestep-v15-turbo `
    -ExePath .\vendor\acestep-cpp\build\ace-server.exe

.NOTES
  - Chạy trên MÁY ĐÃ GHI cấu hình ở docs/phase0/spike-report.md (issue #2 cấm
    đổi máy giữa chừng).
  - Peak VRAM = MAX mẫu `nvidia-smi --query-gpu=memory.used` mỗi giây TRONG lúc
    render — KHÔNG phải Task Manager snapshot (issue #1 cấm). Số là tổng cả GPU
    (gồm process khác) — chấp nhận được vì máy đo chuyên dụng.
  - Peak RAM = MAX working set của process backend, cùng chu kỳ mẫu.
  - Đầu ra: backend,model,cold_s,peak_vram_mb,peak_ram_mb
#>
param(
  [Parameter(Mandatory)][ValidateSet("py", "cpp")] [string]$Backend,
  [Parameter(Mandatory)][string]$Model,
  [string]$ExePath,
  [string[]]$ArgList = @(),
  [string]$WorkDir,
  [string]$BaseUrl = $(if ($Backend -eq "cpp") { "http://127.0.0.1:8080" } else { "http://127.0.0.1:8001" }),
  [int]$DurationS = 120,
  [int]$TimeoutMin = 20
)
$ErrorActionPreference = "Stop"

if (-not $ExePath) {
  # Cold start đo từ lúc SPAWN process (issue #1) — không có ExePath thì không đo được.
  Write-Error "S-04 bắt buộc -ExePath để spawn process backend"
}

$healthUrl = if ($Backend -eq "cpp") { "$BaseUrl/props" } else { "$BaseUrl/v1/models" }

# ---------- 1. Spawn + poll health → cold start ----------
$tmpOut = [IO.Path]::GetTempFileName()
$tmpErr = [IO.Path]::GetTempFileName()
try {
  $proc = Start-Process -FilePath $ExePath -ArgumentList $ArgList `
    -WorkingDirectory ($(if ($WorkDir) { $WorkDir } else { (Get-Location).Path })) `
    -PassThru -RedirectStandardOutput $tmpOut -RedirectStandardError $tmpErr

  $sw = [System.Diagnostics.Stopwatch]::StartNew()
  $healthy = $false
  while ($sw.Elapsed.TotalMinutes -lt $TimeoutMin) {
    try {
      $r = Invoke-WebRequest -Uri $healthUrl -TimeoutSec 2
      if ($r.StatusCode -eq 200) { $healthy = $true; break }
    } catch {
      if ($proc.HasExited) {
        Write-Error "backend thoát sớm khi khởi động (exit $($proc.ExitCode)) — stderr: $(Get-Content $tmpErr -Raw)"
      }
    }
    Start-Sleep -Milliseconds 500
  }
  if (-not $healthy) { Write-Error "timeout ${TimeoutMin}min chờ $healthUrl trả 200" }
  $coldS = [math]::Round($sw.Elapsed.TotalSeconds, 1)
  Write-Host "cold start (${Backend}/${Model}): ${coldS}s"

  # ---------- 2. Generate $DurationS s ở background; foreground sample VRAM/RAM ----------
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

  $genJob = Start-ThreadJob -ArgumentList $Backend, $BaseUrl, $body {
    param($b, $url, $bodyJson)
    if ($b -eq "py") {
      # Giống nhánh py của Bench-Matrix.ps1: release_task → poll query_result.
      $release = Invoke-RestMethod -Method Post -Uri "$url/release_task" -Body $bodyJson -ContentType "application/json"
      $taskId = $release.data.task_id
      if (-not $taskId) { throw "release_task không trả task_id: $($release | ConvertTo-Json -Depth 4)" }
      do {
        Start-Sleep -Seconds 2
        $q = (Invoke-RestMethod -Method Post -Uri "$url/query_result" `
            -Body (@{ task_id_list = @($taskId) } | ConvertTo-Json) -ContentType "application/json").data[0]
      } while ($q.status -eq 0)
      if ($q.status -ne 1) { throw "task failed: $($q.error)" }
    } else {
      # Hai pha cpp: /lm → /synth?wav=1 (giống nhánh cpp của Bench-Matrix.ps1).
      $plan = Invoke-RestMethod -Method Post -Uri "$url/lm" -Body $bodyJson -ContentType "application/json"
      $synthBody = $bodyJson | ConvertFrom-Json
      $synthBody | Add-Member -NotePropertyName audio_codes -NotePropertyValue $plan.audio_codes
      $resp = Invoke-WebRequest -Method Post -Uri "$url/synth?wav=1" `
        -Body ($synthBody | ConvertTo-Json -Depth 5) -ContentType "application/json"
      if (-not $resp.IsSuccessStatusCode) { throw "/synth → HTTP $($resp.StatusCode)" }
    }
  }

  $peakVramMb = [long]0
  $peakRamMb = [long]0
  while ($genJob.State -notin @("Completed", "Failed", "Stopped")) {
    try {
      $vram = [long]((nvidia-smi --query-gpu=memory.used --format=csv,noheader,nounits | Select-Object -First 1))
      if ($vram -gt $peakVramMb) { $peakVramMb = $vram }
    } catch { Write-Warning "nvidia-smi lỗi, lần lấy mẫu này bỏ qua: $_" }
    try {
      $p = Get-Process -Id $proc.Id -ErrorAction SilentlyContinue
      if ($p) {
        $ram = [long]($p.WorkingSet64 / 1MB)
        if ($ram -gt $peakRamMb) { $peakRamMb = $ram }
      }
    } catch {}
    Start-Sleep -Seconds 1
  }
  # Receive-Job ném lại lỗi nếu job Failed — dừng với thông báo rõ ràng.
  Receive-Job $genJob -Wait -ErrorAction Stop | Out-Null
  Remove-Job $genJob -Force

  # ---------- 3. Kết quả ----------
  "{0},{1},{2},{3},{4}" -f $Backend, $Model, $coldS, $peakVramMb, $peakRamMb
}
finally {
  if (Get-Variable proc -ErrorAction SilentlyContinue -ValueOnly) {
    Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
  }
  Remove-Item $tmpOut, $tmpErr -ErrorAction SilentlyContinue
}
