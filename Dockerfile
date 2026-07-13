# Brainiac core — one image, three roles (serve | worker | mcp | eval).
#
# rustls everywhere (no OpenSSL), so the runtime layer needs nothing but CA
# certs. Build deps are cached by copying manifests first.

FROM rust:1-bookworm AS build
WORKDIR /src

# Dependency cache layer: manifests + empty crate roots.
COPY Cargo.toml Cargo.lock ./
COPY crates/brainiac-core/Cargo.toml crates/brainiac-core/
COPY crates/brainiac-fixtures/Cargo.toml crates/brainiac-fixtures/
COPY crates/brainiac-store/Cargo.toml crates/brainiac-store/
COPY crates/brainiac-gateway/Cargo.toml crates/brainiac-gateway/
COPY crates/brainiac-pipeline/Cargo.toml crates/brainiac-pipeline/
COPY crates/brainiac-eval/Cargo.toml crates/brainiac-eval/
COPY crates/brainiac-server/Cargo.toml crates/brainiac-server/
RUN for c in core fixtures store gateway pipeline eval; do \
        mkdir -p crates/brainiac-$c/src && echo "" > crates/brainiac-$c/src/lib.rs; \
    done && \
    mkdir -p crates/brainiac-server/src && \
    echo "fn main() {}" > crates/brainiac-server/src/main.rs && \
    echo "" > crates/brainiac-server/src/lib.rs && \
    cargo build --release -p brainiac-server && \
    rm -rf crates

COPY crates ./crates
COPY migrations ./migrations
COPY fixtures ./fixtures
# Touch so cargo rebuilds the real sources over the cached deps.
RUN find crates -name "*.rs" -exec touch {} + && \
    cargo build --release -p brainiac-server && \
    strip target/release/brainiac

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=build /src/target/release/brainiac /usr/local/bin/brainiac
# Migrations run at boot from the embedded sqlx migrator; fixtures are only
# needed for `eval`, but they're ~100KB and make the image self-demoing.
COPY --from=build /src/fixtures ./fixtures
ENV RUST_LOG=info
EXPOSE 8600
# Override with `worker`, `mcp`, or `eval` in the deployment.
CMD ["brainiac", "serve", "--bind", "0.0.0.0:8600"]
