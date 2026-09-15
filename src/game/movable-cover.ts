import type { Simulation } from "./simulation";

/** Keep AI footprints current, including tipped blocks. Navigation patches are throttled
 * to 4 Hz and only rebuilt after a half-metre change, or once a block settles. */
export function updateMovableCover(sim: Simulation): void {
  for (const cover of sim.movableCovers) {
    if (!cover.alive) {
      continue;
    }
    const m = cover.motion!;
    const p = cover.body.translation();
    const q = cover.body.rotation();
    cover.x = p.x;
    cover.z = p.z;
    // Conservative rotated-box projection of the original barrier bounds.
    cover.w =
      Math.abs(1 - 2 * (q.y * q.y + q.z * q.z)) * m.w +
      Math.abs(2 * (q.x * q.y - q.z * q.w)) * cover.h +
      Math.abs(2 * (q.x * q.z + q.y * q.w)) * m.d;
    cover.d =
      Math.abs(2 * (q.x * q.z - q.y * q.w)) * m.w +
      Math.abs(2 * (q.y * q.z + q.x * q.w)) * cover.h +
      Math.abs(1 - 2 * (q.x * q.x + q.y * q.y)) * m.d;
    if (sim.elapsed < m.checkAt) {
      continue;
    }
    m.checkAt = sim.elapsed + 0.25;
    const change = Math.max(
      Math.abs(cover.x - m.x),
      Math.abs(cover.z - m.z),
      Math.abs(cover.w - m.navW) / 2,
      Math.abs(cover.d - m.navD) / 2,
    );
    if (change < (cover.body.isSleeping() ? 0.02 : 0.5)) {
      continue;
    }
    const left = Math.min(m.x - m.navW / 2, cover.x - cover.w / 2);
    const right = Math.max(m.x + m.navW / 2, cover.x + cover.w / 2);
    const near = Math.min(m.z - m.navD / 2, cover.z - cover.d / 2);
    const far = Math.max(m.z + m.navD / 2, cover.z + cover.d / 2);
    sim.nav.rebuild(sim.covers, {
      ...cover,
      x: (left + right) / 2,
      z: (near + far) / 2,
      w: right - left,
      d: far - near,
    });
    m.x = cover.x;
    m.z = cover.z;
    m.navW = cover.w;
    m.navD = cover.d;
  }
}
