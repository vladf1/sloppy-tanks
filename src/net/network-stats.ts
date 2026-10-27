import { renderRows, type RenderStats, type StatsSections } from "../game/nerd-stats";

/** One feel reading: sample count, mean, median, 95th percentile and largest. */
interface Reading {
  count: number;
  mean: number;
  p50: number;
  p95: number;
  max: number;
}

/** Correction causes in the engine's order (`CorrectionCause::ALL`): row label and what
 * the server or the prediction saw. */
const CORRECTION_CAUSES = [
  ["blast", "Blast corrections", "a blast near your tank or a hit on it"],
  ["tank", "Tank corrections", "contact with another tank"],
  ["crate", "Crate corrections", "contact with a crate or drum"],
  ["debris", "Debris corrections", "solid debris near your tank (wrecks, timber, broken scenery)"],
  ["cover", "Cover corrections", "contact with standing cover"],
  ["lateInput", "Input corrections", "an input change that reached the server after its tick"],
  ["other", "Other corrections", "none of the other causes"],
] as const;

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
    correctionCauses: Record<
      (typeof CORRECTION_CAUSES)[number][0],
      { count: number; mPerS: number }
    >;
    receiveMs: number;
    phantomStrikes: number;
  };
  feel: {
    viewDelayMs: Reading;
    fireToFlashMs: Reading;
    fireToReportMs: Reading;
    unansweredPresses: number;
    hitOffsetM: Reading;
    incomingOffsetM: Reading;
    incomingBehindM: Reading;
    unseenIncoming: number;
    ownShots: {
      predicted: number;
      confirmed: number;
      unconfirmedShells: number;
      unpredicted: number;
      errorM: Reading;
      phantomStrikes: number;
    };
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
  let sampleReceiveMs = 0;
  return () => {
    const now = performance.now();
    const stats = read(now);
    const { network, scene, feel } = stats;
    const rate = sampleMs
      ? ((network.receivedUpdates - sampleUpdates) * 1000) / (now - sampleMs)
      : 0;
    const receiveShare = sampleMs ? (network.receiveMs - sampleReceiveMs) / (now - sampleMs) : 0;
    sampleMs = now;
    sampleUpdates = network.receivedUpdates;
    sampleReceiveMs = network.receiveMs;
    const fire = feel.fireToFlashMs;
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
          "Snapshot batches that arrived over 150 ms after the previous one (they leave 60 times a second) during this page session. The connection held them up, for example while TCP resent a lost packet, or the page itself froze.",
        ],
        [
          "Snapshot CPU",
          `${(receiveShare * 1000).toFixed(1)} ms/s`,
          "Main-thread time spent applying received snapshots per second, own-hull prediction replays included. A snapshot that confirms your tank's prediction needs no replay.",
        ],
        [
          "Longest batch gap",
          `${Math.round(network.longestBatchGapMs)} ms`,
          "The longest wait between consecutive snapshot batches during this page session; about 17 ms while the stream flows.",
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
        ...CORRECTION_CAUSES.map(([key, label, cause]): [string, string, string] => [
          label,
          `${(network.correctionCauses[key].mPerS * 100).toFixed(1)} cm/s`,
          `Corrections of 2 cm or more (${network.correctionCauses[key].count} so far), per second of driving, put down to ${cause}: the first cause, in the order of these rows, that the server or the prediction saw since the previous snapshot.`,
        ]),
        [
          "View delay",
          `${Math.round(feel.viewDelayMs.mean)} ms`,
          "How far behind your own tank's present other tanks, shells and effects are drawn: the round trip, the input margin and the playout buffer.",
        ],
        [
          "Fire to flash",
          fire.count ? `${Math.round(fire.mean)} ms` : "–",
          `From pressing fire with a loaded gun to your muzzle flash on screen, the mean of ${fire.count} presses. ${Math.round(feel.fireToReportMs.mean)} ms of it passes before the server's report of the shot arrives; the rest is the playout buffer.`,
        ],
        [
          "Fire to flash p95",
          fire.count ? `${Math.round(fire.p95)} ms` : "–",
          `The 95th percentile of recent presses; the slowest took ${Math.round(fire.max)} ms.`,
        ],
        [
          "Own shots on time",
          `${feel.ownShots.confirmed} / ${feel.ownShots.predicted}`,
          `Your shots drawn the moment you fired them that the server fired too, out of all such shots. ${feel.ownShots.unpredicted} more were fired by the server without being drawn early, and ${feel.ownShots.unconfirmedShells} early shells were never fired by the server and were dropped.`,
        ],
        [
          "Own shell error",
          feel.ownShots.errorM.count ? `${(feel.ownShots.errorM.mean * 100).toFixed(1)} cm` : "–",
          `How far a shell drawn the moment you fired was from the server's shell, a tenth of a second into its flight: the mean of ${feel.ownShots.errorM.count}, 95th percentile ${(feel.ownShots.errorM.p95 * 100).toFixed(1)} cm. The shell then follows the server's path.`,
        ],
        [
          "Hit offset",
          feel.hitOffsetM.count ? `${feel.hitOffsetM.mean.toFixed(2)} m` : "–",
          `For each shell hit you scored on a tank (${feel.hitOffsetM.count} so far): how far the server's impact was from that tank as your screen drew it when your own tank reached the moment of the hit. Near zero, you hit tanks where you see them; otherwise a crossing tank needs that much extra lead.`,
        ],
        [
          "Hit offset p95",
          feel.hitOffsetM.count ? `${feel.hitOffsetM.p95.toFixed(2)} m` : "–",
          `The 95th percentile of recent hits; the largest was ${feel.hitOffsetM.max.toFixed(2)} m.`,
        ],
        [
          "Incoming offset",
          feel.incomingOffsetM.count ? `${feel.incomingOffsetM.mean.toFixed(2)} m` : "–",
          `For each enemy shell that hit you (${feel.incomingOffsetM.count} so far): how close it was drawn to your tank, where the server judged it, as your own tank reached the moment of the hit; 95th percentile ${feel.incomingOffsetM.p95.toFixed(2)} m. Near zero, a shell you see coming hits where you see it. ${feel.unseenIncoming} more were fired so close that they hit before they could be drawn.`,
        ],
        [
          "Hit where you were",
          feel.incomingBehindM.count ? `${feel.incomingBehindM.mean.toFixed(2)} m` : "–",
          `For each other player's shell that hit you (${feel.incomingBehindM.count} so far): the server judged it where that player's screen showed your tank, a moment earlier, so it strikes there, this far from your tank as drawn now; 95th percentile ${feel.incomingBehindM.p95.toFixed(2)} m. It is how far you moved in that moment.`,
        ],
        [
          "Phantom strikes",
          `${network.phantomStrikes} / ${feel.ownShots.phantomStrikes}`,
          "Shells drawn striking a tank ahead of the server that the server flew past instead, and flew on: enemy shells at your tank, then your own shells at enemy tanks.",
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
