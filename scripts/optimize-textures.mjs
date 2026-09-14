import { execFileSync } from "node:child_process";

// Offline asset step: original artwork stays outside the deployed public directory.
const textures = [
  ["dry-grass.webp", "ground/dry-grass.webp", 75, 768],
  ["packed-dirt.webp", "ground/packed-dirt.webp", 80, 512],
  ["weathered-concrete.webp", "walls/weathered-concrete.webp", 80, 512],
  ["quarry/sandstone.webp", "quarry/sandstone.webp", 75, 768],
  ["tanks/armor-wear.webp", "tanks/armor-wear.webp", 85],
  ["trees/conifer-spray.webp", "trees/conifer-spray.webp", 90],
];
for (const [source, target, quality, size] of textures) {
  execFileSync(
    "cwebp",
    [
      "-quiet",
      "-q",
      String(quality),
      "-alpha_q",
      "100",
      "-m",
      "6",
      "-metadata",
      "none",
      ...(size ? ["-resize", String(size), String(size)] : []),
      `assets/texture-sources/${source}`,
      "-o",
      `public/textures/${target}`,
    ],
    { stdio: "inherit" },
  );
  console.log(`public/textures/${target}`);
}
