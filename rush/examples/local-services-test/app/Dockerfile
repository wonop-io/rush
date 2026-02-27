FROM rust:1.75 as builder

WORKDIR /app
COPY Cargo.toml .
COPY src src

RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/local-services-test-app /usr/local/bin/app
CMD ["app"]