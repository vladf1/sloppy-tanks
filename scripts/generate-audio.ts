import { mkdir, stat } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { SOUNDS, soundFile } from "../src/sounds.ts";

// Offline only: the MP3s the native apps embed, encoded from the WAV files the page
// makes for itself when it starts (src/sounds.ts). Optional names regenerate only the
// requested assets.
const selected = new Set(process.argv.slice(2));
const output = new URL("../public/audio/", import.meta.url);
await mkdir(output, { recursive: true });
for (const sound of SOUNDS) {
  const [name] = sound;
  if (selected.size && !selected.has(name)) continue;
  const data = soundFile(sound);
  const target = new URL(`${name}.mp3`, output);
  const result = spawnSync(
    "ffmpeg",
    [
      "-hide_banner",
      "-loglevel",
      "error",
      "-y",
      "-f",
      "wav",
      "-i",
      "pipe:0",
      "-c:a",
      "libmp3lame",
      "-q:a",
      "2",
      "-map_metadata",
      "-1",
      "-id3v2_version",
      "0",
      "-write_id3v1",
      "0",
      fileURLToPath(target),
    ],
    { input: data },
  );
  if (result.error || result.status !== 0)
    throw new Error(
      `Offline audio encoding requires FFmpeg with libmp3lame: ${result.error?.message ?? result.stderr.toString()}`,
    );
  console.log(`${name}.mp3: ${(await stat(target)).size} bytes`);
}
