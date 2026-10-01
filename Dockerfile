# syntax=docker/dockerfile:1
# check=skip=InvalidDefaultArgInFrom
# (RUST_VERSION has no default so the version lives only in rust-toolchain.toml.)
# The multiplayer server as a linux/amd64 image. Build it with
# `pnpm run server:build-docker-image`, which passes the content version and server
# build the binary stamps (scripts/content-version.mjs), the pinned Rust version and
# the commit.

# rust-toolchain.toml's channel; scripts/build-server-image.mjs passes it.
ARG RUST_VERSION

# The build stage runs natively on the build machine and cross-compiles the same
# static musl binary as `scripts/build-server.mjs --vps`, so an Apple silicon Mac
# never emulates x86 to compile.
FROM --platform=$BUILDPLATFORM rust:${RUST_VERSION}-slim-trixie AS build
ARG RUST_VERSION
RUN rustup target add x86_64-unknown-linux-musl
# Use the image's toolchain as is. Honoring rust-toolchain.toml would install its
# Wasm target, rustfmt and clippy on every build, since the build step's rustup
# changes are not kept.
ENV RUSTUP_TOOLCHAIN=${RUST_VERSION}
WORKDIR /src
COPY . .
ARG SLOPPY_CONTENT_VERSION
ARG SLOPPY_SERVER_BUILD
# Cache mounts keep downloaded crates and compiled dependencies between builds, so
# a source change recompiles only what it touches, as a local `cargo build` does.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target,sharing=locked \
    SLOPPY_CONTENT_VERSION=${SLOPPY_CONTENT_VERSION} \
    SLOPPY_SERVER_BUILD=${SLOPPY_SERVER_BUILD} \
    cargo build --locked --profile server -p sloppy-server --target x86_64-unknown-linux-musl \
    && cp target/x86_64-unknown-linux-musl/server/sloppy-server /sloppy-server

# The binary is static and makes no outbound TLS connections, so it needs no base
# image: no shell, libc or CA certificates.
FROM scratch
ARG SLOPPY_CONTENT_VERSION
ARG SLOPPY_SERVER_BUILD
ARG GIT_COMMIT
# The VPS updater compares these labels with what /health reports after a restart.
# The source label links the registry package to the repository.
LABEL org.opencontainers.image.title="Sloppy Tanks multiplayer server" \
      org.opencontainers.image.source="https://github.com/vladf1/sloppy-tanks" \
      org.opencontainers.image.revision=${GIT_COMMIT} \
      me.fridman.sloppy-tanks.content-version=${SLOPPY_CONTENT_VERSION} \
      me.fridman.sloppy-tanks.server-build=${SLOPPY_SERVER_BUILD}
COPY --from=build /sloppy-server /sloppy-server
# Listen on every interface: the container's loopback is unreachable from outside.
ENV HOST=0.0.0.0 PORT=8787
EXPOSE 8787
USER 65532:65532
ENTRYPOINT ["/sloppy-server"]
