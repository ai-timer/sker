FROM rust:bookworm AS build
WORKDIR /workspace
COPY . .
RUN cargo build --release -p sker-ledger

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /workspace/target/release/sker-ledger /usr/local/bin/sker-ledger
ENV LISTEN_ADDR=0.0.0.0:8080
EXPOSE 8080
USER 65532:65532
ENTRYPOINT ["/usr/local/bin/sker-ledger"]
