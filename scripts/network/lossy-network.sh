#!/usr/bin/env bash
# Drops a share of the packets the multiplayer server sends to this Mac, below TCP, so
# its kernel has to resend them (Chrome's DevTools packet loss only affects WebRTC).
#
#   sudo scripts/network/lossy-network.sh on [percent]   # default 2
#   sudo scripts/network/lossy-network.sh status
#   sudo scripts/network/lossy-network.sh off
#
# It adds a "sloppy-lossy" anchor to the system's own pf rules and a dummynet pipe;
# `off` removes both, reloads /etc/pf.conf and releases its hold on pf, which stays on
# only if something else enabled it.
set -euo pipefail

SERVER=sloppy-tanks-server.fridman.me
ANCHOR=sloppy-lossy
PIPE=4242
TOKEN_FILE=/var/run/sloppy-lossy.token

if [[ $EUID -ne 0 ]]; then
  echo "Run with sudo." >&2
  exit 1
fi

case "${1:-}" in
  on)
    percent=${2:-2}
    plr=$(awk -v p="$percent" 'BEGIN { printf "%.4f", p / 100 }')
    dnctl pipe "$PIPE" config plr "$plr"
    (cat /etc/pf.conf; echo "dummynet-anchor \"$ANCHOR\""; echo "anchor \"$ANCHOR\"") |
      pfctl -q -f - 2>/dev/null
    # Production (443) and the dev server (8443), server to this Mac only.
    echo "dummynet in quick proto tcp from $SERVER port { 443, 8443 } to any pipe $PIPE" |
      pfctl -q -a "$ANCHOR" -f -
    if [[ ! -f $TOKEN_FILE ]]; then
      pfctl -E 2>&1 | awk '/Token/ { print $3 }' >"$TOKEN_FILE"
    fi
    echo "Dropping ${percent}% of packets from $SERVER, open connections included."
    echo "Check with: sudo $0 status   Undo with: sudo $0 off"
    ;;
  off)
    pfctl -q -a "$ANCHOR" -F all 2>/dev/null || true
    dnctl -q pipe delete "$PIPE" 2>/dev/null || true
    pfctl -q -f /etc/pf.conf 2>/dev/null || true
    if [[ -f $TOKEN_FILE ]]; then
      pfctl -X "$(cat "$TOKEN_FILE")" >/dev/null 2>&1 || true
      rm -f "$TOKEN_FILE"
    fi
    echo "Packet loss off."
    ;;
  status)
    pfctl -a "$ANCHOR" -s dummynet 2>/dev/null || true
    dnctl pipe show "$PIPE" 2>/dev/null || echo "No pipe $PIPE."
    pfctl -s info 2>/dev/null | head -1
    ;;
  *)
    echo "Usage: sudo $0 on [percent] | off | status" >&2
    exit 1
    ;;
esac
