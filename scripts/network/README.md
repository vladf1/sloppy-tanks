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

`on` adds a `sloppy-lossy` anchor to the system's own `pf` rules and a `dnctl`
dummynet pipe with that packet loss rate. The anchor sends packets from
`sloppy-tanks-server.fridman.me` on ports 443 (production) and 8443 (the dev server)
to this Mac through the pipe. Nothing else is affected, and connections already open
start losing packets at once, so the game does not need a reload. Reloading the
rules prints macOS's standard `pfctl: Use of -f option` warning.

`off` flushes the anchor, deletes the pipe, reloads `/etc/pf.conf` and releases the
script's hold on `pf`, which stays enabled only if something else enabled it (macOS
often has). Running `off` again is harmless.

Dropping the server's packets also drops its acknowledgements of the player's input,
so uploads stall too. While it is on, expect:

- the dashboard's **Resent** for the room, and `retransmittedSegments` in `/stats`,
  to rise (diluted by the room's other players: the room figure covers everyone);
- the room's **Lapses**, as held controls run out while input is resent;
- **Late batches** and **Longest batch gap** in the room page's Stats for nerds;
- the player's own `rtt … ms, N of M segments resent` in the server journal
  (`pnpm run server:logs`) when they leave the room.
