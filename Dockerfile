FROM rust:1.89-bookworm AS builder

WORKDIR /app

COPY Cargo.toml ./
COPY src ./src

RUN cargo build --release --bin api --bin build-index --bin build-ivf --bin bench-algorithms --bin bench-http-path

FROM debian:bookworm-slim

WORKDIR /app

COPY --from=builder /app/target/release/api /app/api
COPY --from=builder /app/target/release/build-index /app/build-index
COPY --from=builder /app/target/release/build-ivf /app/build-ivf
COPY --from=builder /app/target/release/bench-algorithms /app/bench-algorithms
COPY --from=builder /app/target/release/bench-http-path /app/bench-http-path

ENV BIND_ADDR=0.0.0.0:9999

EXPOSE 9999

ENTRYPOINT ["/app/api"]
