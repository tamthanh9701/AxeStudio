#requires -Version 7.0
<#
.SYNOPSIS
  Tải model từ HuggingFace có RESUME + verify SHA256. Sprint 7 dùng lại script
  này trong first-run downloader (logic tương đương sẽ được port sang Rust).

.EXAMPLE
  ./Model-Fetch.ps1 -Url "https://huggingface.co/Serveurperso/ACE-Step-1.5-GGUF/resolve/main/acestep-v15-turbo-Q8_0.gguf" `
                    -OutFile "D:\models\acestep-v15-turbo-Q8_0.gguf" -Sha256 "<hex>"

.NOTES
  - curl.exe -C - : resume từ byte đã tải (file .part).
  - Thiếu -Sha256 → cảnh báo, không chặn (chỉ chặn khi verify fail).
  - Lý do .part + rename: kill giữa chừng không để lại file trông-như-xong.
#>
param(
  [Parameter(Mandatory)][string]$Url,
  [Parameter(Mandatory)][string]$OutFile,
  [string]$Sha256
)
$ErrorActionPreference = "Stop"

$part = "$OutFile.part"
New-Item -ItemType Directory -Force -Path (Split-Path $OutFile) | Out-Null

Write-Host "Tải $Url"
& curl.exe -L -C - --retry 5 --retry-delay 3 -o $part $Url
if ($LASTEXITCODE -ne 0) { Write-Error "curl exit $LASTEXITCODE — giữ nguyên file .part để resume lần sau" }

if ($Sha256) {
  $actual = (Get-FileHash $part -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($actual -ne $Sha256.ToLowerInvariant()) {
    Remove-Item $part -Force
    Write-Error "SHA256 KHÔNG khớp (got $actual) — đã xoá file lỗi"
  }
  Write-Host "✓ SHA256 khớp"
} else {
  Write-Warning "Không có SHA256 để verify — chỉ chấp nhận cho spike nội bộ"
}

Move-Item -Force $part $OutFile
Write-Host "✓ $OutFile"
