#!/usr/bin/env bash
# 一条命令在本地启动「展会展品入馆封箱与撤展核对系统」。
# 用法：./run.sh          （生产模式：Axum 在 8080 同时提供 API 与 Svelte 页面）
#       ./run.sh dev      （开发模式：Vite 5173 热更新 + 后端 8080）
set -euo pipefail
cd "$(dirname "$0")"

# 1) Rust 工具链（如缺失，提示；CI/干净环境可自动装）
if ! command -v cargo >/dev/null 2>&1; then
  echo ">> 未检测到 cargo，尝试用 rustup 安装 Rust 稳定版…"
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
  source "$HOME/.cargo/env"
fi

# 2) 前端依赖与构建（Svelte -> static/，由 Axum 直接托管）
if [ ! -d frontend/node_modules ]; then
  echo ">> 安装前端依赖…"
  (cd frontend && npm install)
fi
echo ">> 构建 Svelte 前端…"
(cd frontend && npm run build)

# 3) 数据与上传目录
mkdir -p data/uploads

if [ "${1:-}" = "dev" ]; then
  echo ">> 开发模式：后端 http://localhost:8080，前端 http://localhost:5173"
  cargo run &
  (cd frontend && npm run dev)
  wait
else
  echo ">> 启动服务：http://localhost:8080"
  exec cargo run
fi
