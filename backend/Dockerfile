FROM rust:1-bookworm AS builder
WORKDIR /app

# libdrm-dev is needed at link time by gfxinfo's AMD-GPU-detection dependency
# (libdrm_amdgpu_sys) — not installed in the plain rust image by default.
RUN apt-get update \
    && apt-get install -y --no-install-recommends libdrm-dev \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY tool_derive ./tool_derive
COPY src ./src

RUN cargo build --release --locked

FROM debian:bookworm-slim
# libdrm2/libdrm-amdgpu1 are the runtime counterparts of the builder stage's
# libdrm-dev — linking needs the dev package, running needs the actual .so.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 libdrm2 libdrm-amdgpu1 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/backend /usr/local/bin/backend

ENTRYPOINT ["/usr/local/bin/backend"]
