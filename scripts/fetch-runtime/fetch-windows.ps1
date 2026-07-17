$ErrorActionPreference = "Stop"

$Root = Resolve-Path "$PSScriptRoot/../.."
$RuntimeDir = Join-Path $Root "src-tauri/resources/runtime"
$BinDir = Join-Path $RuntimeDir "bin"
$PyDir = Join-Path $RuntimeDir "python"

New-Item -ItemType Directory -Force -Path $BinDir | Out-Null

$UvExe = Join-Path $BinDir "uv.exe"
if (-not (Test-Path $UvExe)) {
    Write-Host "==> Downloading uv (x86_64-pc-windows-msvc)"
    Invoke-WebRequest -Uri "https://github.com/astral-sh/uv/releases/latest/download/uv-x86_64-pc-windows-msvc.zip" -OutFile "$env:TEMP\uv.zip"
    Expand-Archive -Path "$env:TEMP\uv.zip" -DestinationPath "$env:TEMP\uv-extract" -Force
    Copy-Item (Get-ChildItem -Path "$env:TEMP\uv-extract" -Filter "uv.exe" -Recurse | Select-Object -First 1).FullName -Destination $UvExe
    Remove-Item "$env:TEMP\uv.zip", "$env:TEMP\uv-extract" -Recurse -Force
} else {
    Write-Host "==> uv already present, skipping"
}

$BunExe = Join-Path $BinDir "bun.exe"
if (-not (Test-Path $BunExe)) {
    Write-Host "==> Downloading bun (windows-x64)"
    Invoke-WebRequest -Uri "https://github.com/oven-sh/bun/releases/latest/download/bun-windows-x64.zip" -OutFile "$env:TEMP\bun.zip"
    Expand-Archive -Path "$env:TEMP\bun.zip" -DestinationPath "$env:TEMP\bun-extract" -Force
    Copy-Item (Get-ChildItem -Path "$env:TEMP\bun-extract" -Filter "bun.exe" -Recurse | Select-Object -First 1).FullName -Destination $BunExe
    Remove-Item "$env:TEMP\bun.zip", "$env:TEMP\bun-extract" -Recurse -Force
} else {
    Write-Host "==> bun already present, skipping"
}

if (-not (Test-Path $PyDir)) {
    Write-Host "==> Fetching python-build-standalone release metadata"
    $PbsTag = "20260623"
    $Headers = @{}
    if ($env:GITHUB_TOKEN) {
        $Headers["Authorization"] = "Bearer $env:GITHUB_TOKEN"
    }
    $Release = Invoke-RestMethod -Uri "https://api.github.com/repos/astral-sh/python-build-standalone/releases/tags/$PbsTag" -Headers $Headers
    $Asset = $Release.assets | Where-Object {
        $_.name -like "cpython-3.12*x86_64-pc-windows-msvc*install_only.tar.gz" -and
        $_.name -notlike "*debug*" -and $_.name -notlike "*stripped*"
    } | Select-Object -First 1
    if (-not $Asset) {
        Write-Error "没找到匹配的 Windows 资产，检查 PbsTag 或过滤条件"
        exit 1
    }
    Write-Host "==> Downloading $($Asset.browser_download_url)"
    Invoke-WebRequest -Uri $Asset.browser_download_url -OutFile "$env:TEMP\python-standalone.tar.gz"
    New-Item -ItemType Directory -Force -Path $PyDir | Out-Null
    tar -xzf "$env:TEMP\python-standalone.tar.gz" -C $PyDir --strip-components=1
    Remove-Item "$env:TEMP\python-standalone.tar.gz" -Force
} else {
    Write-Host "==> python-build-standalone already present, skipping"
}

Write-Host "==> Runtime fetch complete: $RuntimeDir"
