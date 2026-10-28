import { test } from "node:test";
import { readFileSync } from "node:fs";
import assert from "node:assert/strict";
import { AMMO_OPTIONS } from "../src/game/ammo-options";
import { ZOOM_STEP } from "../src/game/controls";
import { FRAME, INPUT, PHASES } from "../src/game/engine-api";
import { MAP_IDS, MAP_OPTIONS } from "../src/game/map-options";
import { DEFAULT_NAMES, preferredPlayerName, rememberPlayerName } from "../src/net/player-name";
import { MAX_LISTED_SERVERS } from "../src/net/room-list";
import {
  DIFFICULTIES as LISTED_DIFFICULTIES,
  MAX_LISTED_ROOMS,
  PHASES as ROOM_PHASES,
  readRooms,
} from "../src/net/room-listing";
import {
  MAX_ROUND_MINUTES,
  PLAYER_KINDS,
  ROOM_CODE,
  ROOM_SEATS,
  steppedMinutes,
} from "../src/net/room-protocol";
import { memoryStorage, withStorage } from "./local-storage";

// The page shows Battle Setup and the room browser before it loads the engine, so it keeps
// its own copies of these tables. `crates/web/src/page_catalog_tests.rs` writes the Rust
// values to the fixture; rerun it with SLOPPY_UPDATE_FIXTURES=1 after a deliberate change.

type Slots = Record<string, number>;
interface Card {
  name: string;
  detail: string;
}
interface PageCatalog {
  maps: unknown[];
  tanks: unknown[];
  ammo: unknown[];
  battleFormats: (Card & { mode: string })[];
  difficulties: (Card & { difficulty: string })[];
  playerNames: { defaults: string[]; maxLength: number };
  teamNames: string[];
  rooms: {
    maxListedServers: number;
    maxListedRooms: number;
    seats: number;
    maxContentVersionLength: number;
    codeLength: number;
    codeAlphabet: string;
    codeCharacters: string;
    defaultRoundMinutes: number;
    maxRoundMinutes: number;
    mapModes: string[];
    difficulties: string[];
    phases: string[];
    playerKinds: string[];
  };
  inputSlots: Slots;
  frameSlots: Slots;
  netFrameSlots: Slots;
  phases: Slots;
  zoomStep: number;
  roomConnection: { pollMs: number; maxBufferedBytes: number };
}

const read = (path: string) => readFileSync(new URL(path, import.meta.url), "utf8");
const catalog = JSON.parse(read("../crates/web/tests/fixtures/page-catalog.json")) as PageCatalog;
const battleSetup = read("../src/game/battle-setup.html");

/** `const NAME = <literal>` in a shell module Node cannot import (it loads CSS or the
 * generated engine), read as JSON. */
function moduleConstant(path: string, name: string): unknown {
  const declaration = new RegExp(`^(?:export )?const ${name} = ([^;]+?)(?: as const)?;$`, "m");
  const literal = declaration.exec(read(path))?.[1];
  assert.ok(literal, `${path} declares ${name}`);
  return JSON.parse(
    literal
      .replace(/(\w+):/g, '"$1":')
      .replace(/(\d)_(\d)/g, "$1$2")
      .replace(/,(\s*[}\]])/g, "$1"),
  );
}

/** The engine's frame result is its own array, so the page names no length. */
function withoutLength({ length: _, ...slots }: Slots): Slots {
  return slots;
}

function match(source: string, pattern: RegExp): string {
  const found = pattern.exec(source)?.[1];
  assert.ok(found !== undefined, `${pattern} in battle-setup.html`);
  return found;
}

const hex = (color: number) => "#" + color.toString(16).padStart(6, "0");

test("map cards", () => {
  assert.deepEqual(MAP_OPTIONS, catalog.maps);
});

test("tank cards in battle-setup.html", () => {
  const cards = [
    ...battleSetup.matchAll(/<button class="vehicle[^"]*" data-kind="(\w+)"[^]*?<\/button>/g),
  ];
  const shown = cards.map(([card, kind]) => {
    const [hitPointsBar, speedBar] = [...card.matchAll(/--v: ([\d.]+)/g)].map(([, v]) => Number(v));
    return {
      kind,
      name: match(card, /<strong>([^<]+)<\/strong/),
      class: match(card, /<small\s*><span class="full-label">([^<]+)</),
      shortClass: match(
        card,
        /<span class="short-label" aria-hidden="true">([^<]+)<\/span\s*><\/small/,
      ),
      hitPoints: Number(match(card, />(\d+) <span class="full-label">HIT POINTS/)),
      kmPerHour: Number(match(card, />(\d+) KM\/H</)),
      hitPointsBar,
      speedBar,
    };
  });
  assert.deepEqual(shown, catalog.tanks);
});

test("ammunition", () => {
  const options = AMMO_OPTIONS.map((option) => ({ ...option, color: hex(option.color) }));
  assert.deepEqual(options, catalog.ammo);
});

test("battle formats and difficulties in battle-setup.html", () => {
  const choices = (name: string) =>
    [
      ...battleSetup.matchAll(
        new RegExp(
          `name="${name}" value="(\\w+)"[^]*?<b>([^<]+)</b\\s*>\\s*<small>([^<]+)</small`,
          "g",
        ),
      ),
    ].map(([, value, title, detail]) => ({ value, name: title, detail }));
  assert.deepEqual(
    choices("gameMode"),
    catalog.battleFormats.map(({ mode, name, detail }) => ({ value: mode, name, detail })),
  );
  assert.deepEqual(
    choices("difficulty"),
    catalog.difficulties.map(({ difficulty, name, detail }) => ({
      value: difficulty,
      name,
      detail,
    })),
  );
});

test("default player names and the name length", () => {
  const { defaults, maxLength } = catalog.playerNames;
  assert.deepEqual(DEFAULT_NAMES, defaults);
  assert.equal(Number(match(battleSetup, /id="player-name"\s+maxlength="(\d+)"/)), maxLength);
  withStorage(memoryStorage(), () => {
    rememberPlayerName("N".repeat(maxLength + 5));
    assert.equal(preferredPlayerName(), "N".repeat(maxLength));
  });
});

test("team names", () => {
  assert.deepEqual(moduleConstant("../src/net/network-ui.ts", "TEAM_NAMES"), catalog.teamNames);
});

test("room listing limits and choices", () => {
  const rooms = catalog.rooms;
  assert.equal(MAX_LISTED_SERVERS, rooms.maxListedServers);
  assert.equal(MAX_LISTED_ROOMS, rooms.maxListedRooms);
  assert.equal(ROOM_SEATS, rooms.seats);
  assert.equal(Number(match(battleSetup, /Up to (\d+) players/)), rooms.seats);
  assert.equal(MAX_ROUND_MINUTES, rooms.maxRoundMinutes);
  assert.deepEqual(MAP_IDS, rooms.mapModes);
  assert.deepEqual(LISTED_DIFFICULTIES, rooms.difficulties);
  assert.deepEqual(ROOM_PHASES, rooms.phases);
  assert.deepEqual(PLAYER_KINDS, rooms.playerKinds);

  const accepts = (contentVersion: string) => {
    const listing = {
      room: "ABCDEFGH",
      contentVersion,
      mapMode: "village",
      difficulty: "normal",
      humansOnly: false,
      roundMinutes: rooms.defaultRoundMinutes,
      players: 1,
      reserved: 1,
      phase: "lobby",
      roundId: 1,
      time: 0,
      scores: [0, 0],
    };
    try {
      readRooms([listing]);
      return true;
    } catch {
      return false;
    }
  };
  const longest = rooms.maxContentVersionLength;
  assert.equal(accepts(""), false, "an empty content version");
  assert.equal(accepts("v".repeat(longest)), true, `a content version of ${longest}`);
  assert.equal(accepts("v".repeat(longest + 1)), false, `a content version of ${longest + 1}`);
});

test("new room length on Battle Setup", () => {
  const { defaultRoundMinutes, maxRoundMinutes } = catalog.rooms;
  const field = match(battleSetup, /(<input\s+id="create-round-minutes"[^>]*>)/);
  assert.equal(Number(match(field, /\smin="(\d+)"/)), 1);
  assert.equal(Number(match(field, /\smax="(\d+)"/)), maxRoundMinutes);
  assert.equal(Number(match(field, /\svalue="(\d+)"/)), defaultRoundMinutes);
  // A blank length steps from the default.
  assert.equal(steppedMinutes(Number.NaN, 1), defaultRoundMinutes + 1);
});

test("room codes", () => {
  const { codeLength, codeAlphabet, codeCharacters } = catalog.rooms;
  let accepted = "";
  for (let code = 0x20; code <= 0x7e; code++) {
    const character = String.fromCharCode(code);
    if (ROOM_CODE.test(character.repeat(codeLength))) {
      accepted += character;
    }
  }
  assert.equal(accepted, codeCharacters);
  assert.equal(ROOM_CODE.test("A".repeat(codeLength - 1)), false);
  assert.equal(ROOM_CODE.test("A".repeat(codeLength + 1)), false);
  assert.equal(moduleConstant("../src/net/room-browser.ts", "ROOM_CODE_ALPHABET"), codeAlphabet);
});

test("input and frame slots and match phases", () => {
  assert.deepEqual(INPUT, catalog.inputSlots);
  assert.deepEqual(FRAME, withoutLength(catalog.frameSlots));
  assert.deepEqual(
    moduleConstant("../src/net/client.ts", "FRAME"),
    withoutLength(catalog.netFrameSlots),
  );
  assert.deepEqual(Object.fromEntries(PHASES.map((phase, code) => [phase, code])), catalog.phases);
});

test("zoom step and the room connection's timers", () => {
  assert.equal(ZOOM_STEP, catalog.zoomStep);
  const { pollMs, maxBufferedBytes } = catalog.roomConnection;
  assert.equal(moduleConstant("../src/net/client.ts", "POLL_MS"), pollMs);
  assert.equal(moduleConstant("../src/net/client.ts", "MAX_BUFFERED_BYTES"), maxBufferedBytes);
});

test("every section of the fixture is checked here", () => {
  assert.deepEqual(Object.keys(catalog).sort(), [
    "ammo",
    "battleFormats",
    "difficulties",
    "frameSlots",
    "inputSlots",
    "maps",
    "netFrameSlots",
    "phases",
    "playerNames",
    "roomConnection",
    "rooms",
    "tanks",
    "teamNames",
    "zoomStep",
  ]);
});
