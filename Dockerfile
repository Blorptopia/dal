FROM rust:1.89.0-slim-bullseye AS builder
RUN apt-get update
RUN apt-get install -y libssl-dev pkg-config
WORKDIR /opt/dal
RUN mkdir /opt/dal/dist
COPY . ./
RUN \
    --mount=type=cache,target=/code/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/root/.cargo \
     cargo build --release && \
     cp target/release/dal /opt/dal/dist # This is in the same command because of the cache
FROM debian:bullseye-slim
WORKDIR /opt/dal
COPY --from=builder /opt/dal/migrations /opt/dal
COPY --from=builder /opt/dal/dist/dal /opt/dal
CMD ["/opt/dal/dal"]
