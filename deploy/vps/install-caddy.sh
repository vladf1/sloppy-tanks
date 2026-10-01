# Sourced by provision.sh: Caddy from its GitHub release, pinned and checked against the
# release's checksums. Caddy's Cloudsmith apt repository still signs with a subkey that
# expired in 2024, which apt rejects (caddyserver/caddy#8095), and Ubuntu's own package
# trails far behind. To upgrade, change both values from the release's checksums.txt.
CADDY_VERSION=2.11.4
CADDY_DEB_SHA512=1c6f5404f3622e46d401d81f4af59677d46b886229c6694d60fd936b87c72d3bb5d1fcf42b55c8d555769fa75acf434ab618fc7e0df2c79cf8512ee580d38d06

if [ "$(dpkg-query -W -f '${Version}' caddy 2>/dev/null || true)" != "$CADDY_VERSION" ]; then
  caddy_deb=/tmp/caddy_${CADDY_VERSION}_linux_amd64.deb
  curl -fsSL -o "$caddy_deb" \
    "https://github.com/caddyserver/caddy/releases/download/v$CADDY_VERSION/caddy_${CADDY_VERSION}_linux_amd64.deb"
  echo "$CADDY_DEB_SHA512  $caddy_deb" | sha512sum --check --quiet
  apt-get install -y "$caddy_deb"
  rm -f "$caddy_deb"
fi
