# Network conditions

Tools that put a real, lossy network between this machine and the multiplayer
server, to check what the server's and the client's network figures report. They
change the operating system's packet filter, so they run with `sudo` and are manual,
outside CI.

## Why not DevTools or the client's parameters

TCP only resends a segment that was actually lost on the way. Chrome DevTools'
throttling profiles emulate download, upload and latency inside Chrome, and their
**Packet Loss** setting applies only to WebRTC (the DevTools Protocol's `packetLoss`
is "WebRTC packet loss"). The dev client's `?latency`, `?jitter` and `?stall`
parameters delay messages inside the page. None of them loses a TCP segment, so the
server's resend count stays at zero.

## `lossy-network.sh` (macOS)

From the repository root:

```sh
sudo scripts/network/lossy-network.sh on      # drop 2% of packets from the server
sudo scripts/network/lossy-network.sh on 5    # or another percentage
sudo scripts/network/lossy-network.sh status  # the rule and the pipe, if any
sudo scripts/network/lossy-network.sh off     # remove both
```

`on` loads one rule into `com.apple/sloppy-lossy`, a child of the
`dummynet-anchor "com.apple/*"` that macOS's own `/etc/pf.conf` declares, and
creates `dnctl` dummynet pipe 4242 with that packet loss rate. The rule sends
packets from port 443 of the production and dev server machines (their `ip` in
`deploy/servers.json`) to this Mac through the pipe. Nothing else is affected: the active
ruleset, including a VPN's or Internet Sharing's rules, is never reloaded, and
connections already open start losing packets at once, so the game does not need a
reload. The percentage must be above 0 and at most 100. `on` refuses to run when
that anchor is missing or another tool already uses pipe 4242; running it again
changes the rate.

`off` flushes the anchor, deletes the pipe and releases the script's hold on `pf`,
which stays enabled only if something else enabled it (macOS often has). It touches
nothing when packet loss is not on.

Dropping the server's packets also drops its acknowledgements of the player's input,
so uploads stall too. While it is on, expect:

- the dashboard's **Resent** for the room, and `retransmittedSegments` in `/stats`,
  to rise (diluted by the room's other players: the room figure covers everyone);
- the room's **Lapses**, as held controls run out while input is resent;
- **Late batches** and **Longest batch gap** in the room page's Stats for nerds;
- the player's own `rtt … ms, N of M segments resent` in the server journal
  (`pnpm run server:logs`) when they leave the room.
