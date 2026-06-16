# Dockerfile — Geniuz MCP server (stdio) for container / sandbox use.
#
# Purpose: lets Glama (glama.ai) build and launch the server to run its quality
# introspection check, and lets anyone run `geniuz mcp serve` in a container.
# This is a BUILD RECIPE ONLY — it does not change the Geniuz product or any
# existing code path. The server speaks JSON-RPC over stdio (initialize /
# tools/list / tools/call); introspection needs no embedding model, and the
# server soft-fails to keyword-only mode if the model is absent, so it always
# starts and responds.

# ---- builder ----
# Trixie (Debian 13: glibc 2.41 / gcc-14), NOT bookworm — the prebuilt ONNX
# Runtime that `ort` downloads is linked against glibc >= 2.38 (it references
# __isoc23_strtol etc.) and a newer libstdc++; bookworm (glibc 2.36) can't link it.
FROM rust:slim-trixie AS builder
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential pkg-config libssl-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
# ORT (ONNX Runtime) ships compiled C++ objects; the final link must pull in the
# C++ standard library or it fails with `undefined reference to __cxa_call_terminate`.
ENV RUSTFLAGS="-Clink-arg=-lstdc++"
# Only the core `geniuz` binary — not the tray feature or desktop/Tauri app.
# Stage the binary plus any shared lib `ort` copied beside it (copy-dylibs);
# the `|| true` keeps the build green if ORT linked statically (no .so).
RUN cargo build --release --bin geniuz \
 && mkdir -p /artifacts \
 && cp target/release/geniuz /artifacts/ \
 && (cp target/release/*.so* /artifacts/ 2>/dev/null || true)

# ---- runtime ----
# Must match the builder's glibc — trixie, so the binary's newer glibc/libstdc++
# symbols resolve at runtime.
FROM debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /artifacts/ /app/
ENV LD_LIBRARY_PATH=/app
# Model auto-downloads here on first remember/recall when network is available;
# introspection does not require it.
ENV GENIUZ_MODELS_PATH=/app/models
# stdio MCP server — Glama pipes JSON-RPC over stdin/stdout.
ENTRYPOINT ["/app/geniuz", "mcp", "serve"]
