FROM rust:1.87-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p objective

FROM debian:bookworm-slim
RUN useradd --create-home --shell /bin/bash objective
USER objective
WORKDIR /home/objective
COPY --from=builder /app/target/release/objective /usr/local/bin/objective
EXPOSE 8080 8081
CMD ["objective", "serve"]
