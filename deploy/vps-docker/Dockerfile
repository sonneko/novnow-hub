# VPS + Docker の代替構成用 Dockerfile（M7 §2.1）。
#
# 多段ビルドで web（静的成果物）とサーバー（Rust バイナリ）を構築し、
# 1 つのランタイムイメージにまとめる。秘密はビルド引数・環境変数で渡し、
# リポジトリに載せない。

# ---- ステージ1: web の静的成果物を構築 ----
FROM node:22-alpine AS web-build
WORKDIR /app
COPY web/package.json web/package-lock.json* ./
RUN npm ci
COPY web/ ./
RUN npm run build

# ---- ステージ2: サーバー（Rust）を構築 ----
FROM rust:1-slim AS server-build
WORKDIR /app
COPY server/Cargo.toml server/Cargo.lock* ./
COPY server/src ./src
RUN cargo build --release --locked

# ---- ステージ3: ランタイム ----
FROM debian:bookworm-slim AS runtime

# サーバーの依存（TLS 等）を最小限インストール。
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# サーバーバイナリを配置。
COPY --from=server-build /app/target/release/novnow-hub-server /usr/local/bin/novnow-hub-server

# web の静的成果物を配置（サーバーが公開ページとして配信する想定）。
COPY --from=web-build /app/dist /app/web-dist

# 待受ポート（既定 8080）。
EXPOSE 8080

# 秘密は実行時に環境変数で渡す（リポジトリに載せない）。
# 例: HUB_GITHUB_OAUTH_CLIENT_SECRET、HUB_WEBHOOK_SECRET 等。
ENV HUB_BIND_ADDR=0.0.0.0 \
    HUB_PORT=8080

# サーバー起動時に web-dist を公開ページとして配信する。
# 公開ベース URL は HUB_PUBLIC_BASE で指定する。
CMD ["novnow-hub-server"]
