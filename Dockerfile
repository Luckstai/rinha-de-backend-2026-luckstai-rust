FROM debian:bookworm-slim

WORKDIR /app

COPY bin/api /app/api
COPY resources /app/resources

ENV BIND_ADDR=0.0.0.0:9999

EXPOSE 9999

ENTRYPOINT ["/app/api"]
