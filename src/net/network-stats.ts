import { renderRows, type RenderStats, type StatsSections } from "../game/nerd-stats";

/** The part of `NetGame.stats_json()` a room's Stats for nerds panel reads. */
export interface NetworkStatsSource extends RenderStats {
  scene?: {
    tanks: number;
    alive: number;
    mines: number;
    pickups: number;
    pickupsReady: number;
    shots: number;
    fragments: number;
    elapsed: number;
  };
  network: {
    rttMs: number;
    receivedUpdates: number;
    snapshotAgeMs: number;
    bufferMs: number;
    marginMs: number;
    underrun: number;
    serverTick: number;
    inputSeq: number;
    inputAck: number;
    connected: boolean;
    lateBatches: number;
    longestBatchGapMs: number;
    predictionLeadMs: number;
    correctionMPerS: number;
    correctionP95M: number;
    inputStarts: number;
    lateInputStarts: number;
  };
}

/** A room's rows for `NerdStats` (opened with `{ network: true }`), read from `NetGame`
 * when the panel refreshes. The update rate is measured between refreshes. */
export function networkStatsSections(
  read: (now: number) => NetworkStatsSource,
  pixelRatio: () => number,
): () => StatsSections {
  let sampleMs = 0;
  let sampleUpdates = 0;
  return () => {
    const now = performance.now();
    const stats = read(now);
    const { network, scene } = stats;
    const rate = sampleMs
      ? ((network.receivedUpdates - sampleUpdates) * 1000) / (now - sampleMs)
      : 0;
    sampleMs = now;
    sampleUpdates = network.receivedUpdates;
    return {
      Network: [
        ["RTT", `${Math.round(network.rttMs)} ms`, "Measured round-trip time to the game server."],
        [
          "Updates received",
          network.receivedUpdates,
          "Full-state messages and snapshot batches received during this page session. A batch can contain several simulation snapshots.",
        ],
        [
          "Update rate",
          `${rate.toFixed(1)} /s`,
          "Full-state messages and snapshot batches received per second, not rendered FPS.",
        ],
        [
          "Snapshot age",
          `${Math.round(network.snapshotAgeMs)} ms`,
          "Time since the last full state or snapshot arrived.",
        ],
        [
          "Playout buffer",
          `${Math.round(network.bufferMs)} ms`,
          "How far other tanks are drawn behind the fastest recent snapshot arrival. It grows when snapshots arrive late and shrinks slowly afterwards.",
        ],
        [
          "Buffered ahead",
          `${Math.round(network.marginMs)} ms`,
          "Received simulation not yet displayed. Negative means snapshots are late and other tanks are briefly extrapolated.",
        ],
        [
          "Underrun",
          `${(network.underrun * 100).toFixed(1)} %`,
          "Share of recent frames drawn past the newest snapshot. Sustained values mean visible stutter.",
        ],
        [
          "Late batches",
          network.lateBatches,
          "Snapshot batches that arrived over 150 ms after the previous one (they leave every 50 ms) during this page session. The connection held them up, for example while TCP resent a lost packet, or the page itself froze.",
        ],
        [
          "Longest batch gap",
          `${Math.round(network.longestBatchGapMs)} ms`,
          "The longest wait between consecutive snapshot batches during this page session; about 50 ms while the stream flows.",
        ],
        [
          "Prediction lead",
          `${Math.round(network.predictionLeadMs)} ms`,
          "How far your own tank is predicted ahead of the newest server tick, so driving responds at once. About the round trip plus a little margin.",
        ],
        [
          "Corrections",
          `${(network.correctionMPerS * 100).toFixed(1)} cm/s`,
          "How far server snapshots moved your predicted tank, per second of driving. Each correction is glided out over about 0.1 s. Blasts, other players' steering and debris cause most of them.",
        ],
        [
          "Correction p95",
          `${(network.correctionP95M * 100).toFixed(1)} cm`,
          "The 95th percentile of your tank's recent prediction corrections.",
        ],
        [
          "Late input starts",
          `${network.lateInputStarts} / ${network.inputStarts}`,
          "Input changes the server started after the tick your prediction asked for, out of all acknowledged changes during this page session. Late ones arrived too late to start on time, so the prediction is corrected.",
        ],
        ["Server tick", network.serverTick, "Latest authoritative simulation tick received."],
        [
          "Input seq sent / ack",
          `${network.inputSeq} / ${network.inputAck}`,
          "Latest input sequence sent and acknowledged by the server. Active input sends up to 20/s; unchanged idle input refreshes once/s to retain your seat. These are not received state updates.",
        ],
        [
          "Connection",
          network.connected ? "Connected" : "Reconnecting",
          "Current game-server connection state.",
        ],
      ],
      Render: renderRows(stats),
      Battle: scene
        ? [
            ["Tanks", `${scene.alive} / ${scene.tanks}`, "Tanks alive out of total spawned."],
            ["Mines", scene.mines, "Live mines on the field."],
            [
              "Pickups ready",
              `${scene.pickupsReady} / ${scene.pickups}`,
              "Pickups available now out of total placed.",
            ],
            ["Projectiles", scene.shots, "Shots currently flying."],
            [
              "Debris bodies",
              scene.fragments,
              "Debris pieces in the received scene; server physics allocation is not measured here.",
            ],
            [
              "Sim time",
              `${scene.elapsed.toFixed(1)}s`,
              "Elapsed simulation time since the round started.",
            ],
          ]
        : [],
      Configuration: [
        [
          "Pixel ratio",
          pixelRatio(),
          "Renderer resolution multiplier, capped from the display pixel ratio.",
        ],
      ],
    };
  };
}
