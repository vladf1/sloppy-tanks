# The multiplayer server as a linux/amd64 image. Build it with
# `pnpm run server:build-docker-image`, which first cross-compiles the static musl
# binary with `scripts/build-server.mjs --vps` in the normal Cargo target directory,
# so compiled dependencies are reused as in any local or cached CI build, builds the
# server's copy of the game page (scripts/server-site.mjs), and passes the content
# version and server build the binary stamps (scripts/content-version.mjs), the page
# copy's site build, the commit, the build time and the release version
# (scripts/release-version.mjs).

# The binary is static and bundles the root certificates its one outbound HTTPS client
# (announcing to the server directory) trusts, so it needs no base image: no shell, libc
# or CA certificates.
FROM scratch
ARG SLOPPY_CONTENT_VERSION
ARG SLOPPY_SERVER_BUILD
ARG SLOPPY_SITE_BUILD
ARG GIT_COMMIT
ARG BUILT_AT
ARG RELEASE
# The VPS updater compares these labels with what /health reports after a restart.
# The source label links the registry package to the repository.
LABEL org.opencontainers.image.title="Sloppy Tanks multiplayer server" \
      org.opencontainers.image.source="https://github.com/vladf1/sloppy-tanks" \
      org.opencontainers.image.revision=${GIT_COMMIT} \
      me.fridman.sloppy-tanks.content-version=${SLOPPY_CONTENT_VERSION} \
      me.fridman.sloppy-tanks.server-build=${SLOPPY_SERVER_BUILD} \
      me.fridman.sloppy-tanks.site-build=${SLOPPY_SITE_BUILD} \
      me.fridman.sloppy-tanks.release=${RELEASE}
COPY target/x86_64-unknown-linux-musl/server/sloppy-server /sloppy-server
# The game page the server serves at /, which changes more often than the binary, so it
# comes last and a page change reuses the binary's layer.
COPY dist-server /site
# Listen on every interface: the container's loopback is unreachable from outside.
ENV HOST=0.0.0.0 PORT=8787 SITE_DIR=/site
# /health reports these beside the stamps; as environment rather than compiled in, they
# leave the binary unchanged from one commit to the next.
ENV SLOPPY_COMMIT=${GIT_COMMIT} SLOPPY_BUILT_AT=${BUILT_AT} SLOPPY_RELEASE=${RELEASE}
EXPOSE 8787
USER 65532:65532
ENTRYPOINT ["/sloppy-server"]
