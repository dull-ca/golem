# golemd container — multi-stage: compile with the Rust toolchain, then run on a
# slim Debian base. Produces an image whose default command is `golemd`.
#
#   docker build -t golemd:latest .
#   docker run -p 7474:7474 golemd:latest --host <NODE>
#
# golemd needs a writable --state-dir for its SQLite plan room; the image
# defaults it to /var/lib/golem (declared a VOLUME) and binds 0.0.0.0:7474 so
# the API is reachable from outside the container.

# ---- build stage -----------------------------------------------------------
FROM rust:1.95-slim-bookworm AS build
WORKDIR /src

# Build dependencies first (better layer caching), then the real sources.
# rusqlite uses the `bundled` feature, so we just need a C toolchain.
RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config build-essential \
    && rm -rf /var/lib/apt/lists/*

COPY . .
RUN cargo build --release -p golemd -p golemctl \
    && cp target/release/golemd target/release/golemctl /usr/local/bin/

# ---- runtime stage ---------------------------------------------------------
FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=build /usr/local/bin/golemd /usr/local/bin/golemd
COPY --from=build /usr/local/bin/golemctl /usr/local/bin/golemctl

# golemd reads --host from the GOLEM_HOST env var (clap `env`) when the flag is
# omitted; it has no default, so the caller must supply one or the other.
# --state-dir and --listen are fixed in the ENTRYPOINT below.
ENV GOLEM_HOST=""

RUN mkdir -p /var/lib/golem
VOLUME ["/var/lib/golem"]
EXPOSE 7474

ENTRYPOINT ["golemd", "--state-dir", "/var/lib/golem", "--listen", "0.0.0.0:7474"]
