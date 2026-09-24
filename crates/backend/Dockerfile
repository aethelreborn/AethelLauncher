# Multi-stage build for minimal image
FROM rust:bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p backend

FROM gcr.io/distroless/cc-debian12
COPY --from=builder /app/target/release/aethel-backend /aethel-backend
EXPOSE 8080
CMD ["/aethel-backend"]
