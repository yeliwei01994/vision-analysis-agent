FROM rust:1.88-bookworm AS build
WORKDIR /src/backend
COPY backend .
COPY db/migrations /src/db/migrations
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates ffmpeg && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/backend/target/release/vision-event-api /app/vision-event-api
EXPOSE 8080
ENTRYPOINT ["/app/vision-event-api"]
