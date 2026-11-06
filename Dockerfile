# syntax=docker/dockerfile:1.7
# Контекст сборки — корень репозитория (там workspace Cargo.toml): docker build .

FROM lukemathwalker/cargo-chef:latest-rust-1-trixie AS chef
WORKDIR /src

# Layer 1: рецепт зависимостей (кэш переживает изменения исходников)
FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY crates crates
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS build
COPY --from=planner /src/recipe.json recipe.json
RUN --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry \
    cargo chef cook --release --package template-web --recipe-path recipe.json

# Layer 2: исходники
COPY Cargo.toml Cargo.lock ./
COPY crates crates
RUN --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry \
    cargo build --release --locked --package template-web \
    && cp target/release/template-web /app

FROM gcr.io/distroless/cc-debian13:nonroot AS runtime
WORKDIR /app

ENV LISTEN_ADDR=0.0.0.0:8080
ENV RUST_LOG=info,sqlx=warn,sea_orm=warn

COPY --from=build /app /app/template-web

EXPOSE 8080

# в distroless нет wget/curl — бинарь умеет проверять себя сам
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD ["/app/template-web", "healthcheck"]

ENTRYPOINT ["/app/template-web"]
