import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { test } from "bun:test";
import { SOUNDS, soundFile } from "../src/sounds";

/**
 * SHA-256 of each sound's WAV file, as the generator wrote it when the MP3s in
 * `public/audio/` were encoded. The page plays these files and the native apps those
 * MP3s, so a change here is a change to what one of them plays: regenerate the MP3s
 * (`bun run generate:audio`) with it.
 */
const FILES: Record<string, string> = {
  shot: "218d670c3d8ff24e6ecbad75245558f9e57ba4a5e1c990805d487ad7f31ec63d",
  explosion: "cef563969e9930b6741b05c6b824a00bb6943fea45bad37b0ec85d4523fd7b9c",
  impact: "69031f01932334f8422962b6f60c5752562c622ca9fdbfabb0078b02db706253",
  "wood-break": "86e2e93a78e6d496503fb8113ab9f3b397ccdf74f435db625f7ae900593fb71f",
  "rubble-break": "5ec272d2c686b9f875d0c64fe7827b3467ddab1f505eb2e617831ca93f8a8423",
  pickup: "5197b6d9fbc47fa0dcab866d5b819215e8d8b915d678787a4f6777a2facab62d",
  "shot-spread": "2f94df69087947fa76b5d961073aaa5f3b2f10ff4c7da4509203f43ebb81c0e7",
  "shot-rocket": "c16c3c8cf161af1d488c9beb078faba227681cd7042e022f5e8c01fc7b29e4b0",
  "shot-ricochet": "b69596848b5040577c999cf45c68c8b640d791c50d5930f6bfbac2f87f148292",
  "shot-piercing": "17a7bec8de2dcd7fcd95e80a80514767bcc8c883d624e5705b07f4c4c9e3610b",
  hit: "cf0207ae112c4fc3fad901cb39960108eade321229049c287ed04b7186b9b05d",
  laser: "edc95435845918a3dd56f6a44cedb2a92e63770d418206d98badcc0c247c071c",
  promotion: "8f9cba9f650e985c888cc627e92e027e559017eace7562058604143b263bf1ba",
};

test("the page's sounds are the files the apps' MP3s were encoded from", () => {
  assert.deepEqual(
    SOUNDS.map(([name]) => name),
    Object.keys(FILES),
  );
  for (const sound of SOUNDS) {
    const digest = createHash("sha256").update(soundFile(sound)).digest("hex");
    assert.equal(digest, FILES[sound[0]], sound[0]);
  }
});

test("every sound the shell plays has a file, and the apps an MP3 of it", async () => {
  const source = await Bun.file("crates/web/src/sounds.rs").text();
  const names = [...source.matchAll(/Sound::\w+ => "([\w-]+)"/g)].map((match) => match[1]);
  assert.deepEqual(names.sort(), SOUNDS.map(([name]) => name).sort());
  for (const name of names) {
    assert.ok(await Bun.file(`public/audio/${name}.mp3`).exists(), `${name}.mp3`);
  }
});
