# The multiplayer server as a linux/amd64 image. Build it with
# `pnpm run server:build-docker-image`, which first cross-compiles the static musl
# binary with `scripts/build-server.mjs --vps` in the normal Cargo target directory,
# so compiled dependencies are reused as in any local or cached CI build, and passes
# the content version and server build the binary stamps (scripts/content-version.mjs)
# and the commit.

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
COPY target/x86_64-unknown-linux-musl/server/sloppy-server /sloppy-server
# Listen on every interface: the container's loopback is unreachable from outside.
ENV HOST=0.0.0.0 PORT=8787
EXPOSE 8787
USER 65532:65532
ENTRYPOINT ["/sloppy-server"]
