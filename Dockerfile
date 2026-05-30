# syntax=docker/dockerfile:1.7

ARG RUST_VERSION=1.82
ARG NODE_VERSION=20-bookworm

FROM node:${NODE_VERSION} AS node
FROM rust:${RUST_VERSION}-bookworm AS base

ENV CI=true \
    NPM_CONFIG_AUDIT=false \
    NPM_CONFIG_FUND=false

COPY --from=node /usr/local /usr/local

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        ca-certificates \
        dpkg-dev \
        fakeroot \
        file \
        libayatana-appindicator3-dev \
        libcairo2-dev \
        libgdk-pixbuf-2.0-dev \
        libglib2.0-dev \
        libgtk-3-dev \
        libpango1.0-dev \
        libssl-dev \
        libsoup-3.0-dev \
        libwebkit2gtk-4.1-dev \
        libxdo-dev \
        librsvg2-dev \
        patchelf \
        pkg-config \
        rpm \
        xdg-utils \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

FROM base AS dependencies

COPY package.json package-lock.json ./
RUN npm ci

COPY src-tauri/Cargo.toml src-tauri/Cargo.lock ./src-tauri/
RUN cd src-tauri && cargo fetch --locked

COPY bootstrapper/Cargo.toml bootstrapper/Cargo.lock ./bootstrapper/
RUN cd bootstrapper && cargo fetch --locked

FROM dependencies AS builder

COPY . .
RUN cd bootstrapper && cargo build --release --locked
RUN npm run tauri build

FROM scratch AS artifacts

COPY --from=builder /app/src-tauri/target/release/bundle /bundle
COPY --from=builder /app/src-tauri/target/release/orangedl /orangedl-linux
COPY --from=builder /app/bootstrapper/target/release/orangedl-bootstrap /bootstrapper/orangedl-bootstrap-linux
