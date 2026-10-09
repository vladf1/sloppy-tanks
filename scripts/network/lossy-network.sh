#!/usr/bin/env bash
# Drops a share of the packets the multiplayer server sends to this Mac, below TCP, so
# its kernel has to resend them (Chrome's DevTools packet loss only affects WebRTC).
#
#   sudo scripts/network/lossy-network.sh on [percent]   # default 2
#   sudo scripts/network/lossy-network.sh status
#   sudo scripts/network/lossy-network.sh off
#
# The rule goes into a child of the "com.apple/*" dummynet anchor that macOS's own
# /etc/pf.conf declares, so the active ruleset (a VPN's or Internet Sharing's rules
# included) is never reloaded. The script only touches the anchor and the dummynet pipe
# it created, and releases its hold on pf, which stays on if something else enabled it.
set -euo pipefail

# The production and dev game server machines, whose wss traffic comes from port 443.
SERVERS_FILE=$(dirname "$0")/../../deploy/servers.json
ANCHOR=com.apple/sloppy-lossy
PIPE=4242
# Exists while the script owns the pipe and anchor; holds its pf enable token.
STATE_FILE=/var/run/sloppy-lossy.state

if [[ $EUID -ne 0 ]]; then
  echo "Run with sudo." >&2
  exit 1
fi

pipe_exists() {
  [[ -n $(dnctl pipe show "$PIPE" 2>/dev/null) ]]
}

case "${1:-}" in
  on)
    percent=${2:-2}
    if [[ ! $percent =~ ^[0-9]+(\.[0-9]+)?$ ]] ||
      ! awk -v p="$percent" 'BEGIN { exit !(p > 0 && p <= 100) }'; then
      echo "The loss must be a percentage above 0 and at most 100, not '$percent'." >&2
      exit 1
    fi
    if ! pfctl -s dummynet 2>/dev/null | grep -qF 'dummynet-anchor "com.apple/*"'; then
      echo "The active pf rules have no dummynet-anchor \"com.apple/*\" to load into." >&2
      exit 1
    fi
    if [[ ! -f $STATE_FILE ]] && pipe_exists; then
      echo "dummynet pipe $PIPE belongs to something else; change PIPE in $0." >&2
      exit 1
    fi
    if [[ ! -f $STATE_FILE ]]; then
      pfctl -E 2>&1 | awk '/Token/ { print $3 }' >"$STATE_FILE"
    fi
    # A machine still to be created has no ip (plutil cannot extract null).
    servers=()
    for role in production dev; do
      if ip=$(plutil -extract "$role.ip" raw -o - "$SERVERS_FILE" 2>/dev/null); then servers+=("$ip"); fi
    done
    if [[ ${#servers[@]} -eq 0 ]]; then
      echo "deploy/servers.json lists no server ip." >&2
      exit 1
    fi
    plr=$(awk -v p="$percent" 'BEGIN { printf "%.4f", p / 100 }')
    dnctl pipe "$PIPE" config plr "$plr"
    # Server to this Mac only.
    echo "dummynet in quick proto tcp from { ${servers[*]} } port 443 to any pipe $PIPE" |
      pfctl -q -a "$ANCHOR" -f -
    echo "Dropping ${percent}% of packets from ${servers[*]}, open connections included."
    echo "Check with: sudo $0 status   Undo with: sudo $0 off"
    ;;
  off)
    if [[ ! -f $STATE_FILE ]]; then
      echo "Packet loss is not on."
      exit 0
    fi
    pfctl -q -a "$ANCHOR" -F all 2>/dev/null || true
    dnctl -q pipe delete "$PIPE" 2>/dev/null || true
    token=$(cat "$STATE_FILE")
    if [[ -n $token ]]; then
      pfctl -X "$token" >/dev/null 2>&1 || true
    fi
    rm -f "$STATE_FILE"
    echo "Packet loss off."
    ;;
  status)
    if [[ -f $STATE_FILE ]]; then
      pfctl -a "$ANCHOR" -s dummynet 2>/dev/null
      dnctl pipe show "$PIPE"
    else
      echo "Packet loss is off."
    fi
    pfctl -s info 2>/dev/null | head -1
    ;;
  *)
    echo "Usage: sudo $0 on [percent] | off | status" >&2
    exit 1
    ;;
esac
