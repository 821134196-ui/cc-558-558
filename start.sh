#!/usr/bin/env bash
# 展会展品入馆封箱与撤展核对系统 — 一键本地启动
# 用法: ./start.sh
set -euo pipefail
cd "$(dirname "$0")"

export PATH="$HOME/.cargo/bin:$PATH"

# 1. 构建前端（首次或加 --rebuild 时重新构建）
if [ ! -d web/node_modules ]; then
  echo "==> 安装前端依赖"
  (cd web && npm install)
fi
if [ ! -d web/dist ] || [ "${1:-}" = "--rebuild" ]; then
  echo "==> 构建前端"
  (cd web && npm run build)
fi

# 2. 准备数据目录（SQLite 数据库与现场照片）
mkdir -p data uploads
export DATABASE_URL="${DATABASE_URL:-sqlite://data/app.db?mode=rwc}"
export UPLOAD_DIR="${UPLOAD_DIR:-uploads}"
export RUST_LOG="${RUST_LOG:-info}"

# 3. 启动后端（同时托管前端页面）
echo "==> 启动服务: http://localhost:3000"
cargo run
