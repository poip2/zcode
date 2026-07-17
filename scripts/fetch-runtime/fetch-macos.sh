#!/usr/bin/env bash
# 只在 CI（macos-latest = Apple Silicon）或本地手动执行；幂等。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RUNTIME_DIR="$ROOT/src-tauri/resources/runtime"
BIN_DIR="$RUNTIME_DIR/bin"
PY_DIR="$RUNTIME_DIR/python"

mkdir -p "$BIN_DIR"

if [ ! -f "$BIN_DIR/uv" ]; then
  echo "==> Downloading uv (aarch64-apple-darwin)"
  TMP_UV_DIR="$(mktemp -d)"
  curl -fL "https://github.com/astral-sh/uv/releases/latest/download/uv-aarch64-apple-darwin.tar.gz" -o "$TMP_UV_DIR/uv.tar.gz"
  tar -xzf "$TMP_UV_DIR/uv.tar.gz" -C "$TMP_UV_DIR"
  find "$TMP_UV_DIR" -maxdepth 2 -name uv -type f -exec cp {} "$BIN_DIR/uv" \;
  chmod +x "$BIN_DIR/uv"
  rm -rf "$TMP_UV_DIR"
else
  echo "==> uv already present, skipping"
fi

if [ ! -f "$BIN_DIR/bun" ]; then
  echo "==> Downloading bun (darwin-aarch64)"
  TMP_BUN_DIR="$(mktemp -d)"
  curl -fL "https://github.com/oven-sh/bun/releases/latest/download/bun-darwin-aarch64.zip" -o "$TMP_BUN_DIR/bun.zip"
  unzip -q "$TMP_BUN_DIR/bun.zip" -d "$TMP_BUN_DIR/extract"
  cp "$TMP_BUN_DIR/extract/bun-darwin-aarch64/bun" "$BIN_DIR/bun"
  chmod +x "$BIN_DIR/bun"
  rm -rf "$TMP_BUN_DIR"
else
  echo "==> bun already present, skipping"
fi

if [ ! -d "$PY_DIR" ]; then
  echo "==> Fetching python-build-standalone release metadata"
  PBS_TAG="20260623"
  AUTH_HEADER=()
  if [ -n "${GITHUB_TOKEN:-}" ]; then
    AUTH_HEADER=(-H "Authorization: Bearer ${GITHUB_TOKEN}")
  fi
  ASSET_URL=$(curl -fsSL "${AUTH_HEADER[@]}" "https://api.github.com/repos/astral-sh/python-build-standalone/releases/tags/${PBS_TAG}" \
    | grep browser_download_url \
    | grep "cpython-3.12" \
    | grep "aarch64-apple-darwin-install_only.tar.gz" \
    | grep -v debug | grep -v stripped \
    | cut -d '"' -f 4 | head -n1)
  if [ -z "$ASSET_URL" ]; then
    echo "!! 没找到匹配的资产，检查 PBS_TAG 或过滤条件" >&2
    exit 1
  fi
  echo "==> Downloading $ASSET_URL"
  curl -fL "$ASSET_URL" -o /tmp/python-standalone.tar.gz
  mkdir -p "$PY_DIR"
  tar -xzf /tmp/python-standalone.tar.gz -C "$PY_DIR" --strip-components=1
  rm -f /tmp/python-standalone.tar.gz
else
  echo "==> python-build-standalone already present, skipping"
fi

echo "==> Runtime fetch complete: $RUNTIME_DIR"
