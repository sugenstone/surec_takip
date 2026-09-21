FROM rust:1.92.0-bookworm AS build
WORKDIR /workspace
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY apps/server apps/server
COPY migrations migrations
RUN cargo build --locked --bins

FROM debian:bookworm-slim AS runtime
WORKDIR /app
COPY --from=build /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=build /workspace/target/debug/platform-server /app/platform-server
COPY --from=build /workspace/target/debug/migrate /app/migrate
COPY --from=build /workspace/target/debug/probe /app/probe
USER 10001:10001
EXPOSE 8080
CMD ["/app/platform-server"]
