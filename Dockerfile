FROM lukemathwalker/cargo-chef:latest-rust-1.72.0 AS chef

WORKDIR /app

# hadolint ignore=DL3008,DL3027
RUN apt update && apt install lld clang -y

FROM chef as planner

COPY . .

RUN cargo chef prepare --recipe-path recipe.json

FROM chef as builder

COPY --from=planner /app/recipe.json recipe.json

RUN cargo chef cook --release --recipe-path recipe.json

COPY . .

ENV SQLX_OFFLINE=true

RUN cargo build --release --bin newsletter

FROM debian:bookworm-slim AS runtime

WORKDIR /app

# hadolint ignore=DL3008
RUN apt-get update -y \
  && apt-get install -y --no-install-recommends openssl ca-certificates \
  && apt-get autoremove -y \
  && apt-get clean -y \
  && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/newsletter newsletter
COPY --from=builder /app/templates templates

COPY configuration configuration

ENV APP_ENVIRONMENT=production

ENTRYPOINT ["./newsletter"]
