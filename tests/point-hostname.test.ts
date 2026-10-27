import { test } from "node:test";
import assert from "node:assert/strict";
import { aRecord, cloudflareErrors, zoneFor } from "../scripts/point-hostname.mjs";

test("a hostname belongs to the longest zone it ends with", () => {
  assert.equal(zoneFor("play.example.com", ["example.com", "fridman.me"]), "example.com");
  assert.equal(
    zoneFor("a.tesla.fridman.me", ["fridman.me", "tesla.fridman.me"]),
    "tesla.fridman.me",
  );
  assert.throws(() => zoneFor("play.example.com", ["dman.me"]), /no zone/);
  assert.equal(zoneFor("fridman.me", ["fridman.me"]), "fridman.me");
});

test("only a single A record of that name is updated", () => {
  const hostname = "play.example.com";
  const record = { id: "1", type: "A", name: hostname, content: "108.61.218.248" };
  const others = [
    { id: "2", type: "AAAA", name: hostname, content: "::1" },
    { id: "3", type: "A", name: "fridman.me", content: "185.199.108.153" },
  ];
  assert.equal(aRecord([...others, record], hostname), record);
  assert.throws(() => aRecord(others, hostname), /no A record/);
  assert.throws(() => aRecord([record, { ...record, id: "4" }], hostname), /2 A records/);
});

test("only a successful reply counts", () => {
  assert.deepEqual(cloudflareErrors({ success: true, errors: [], result: {} }), []);
  assert.deepEqual(
    cloudflareErrors({
      success: false,
      errors: [{ code: 10000, message: "Authentication error" }],
    }),
    ["Authentication error (10000)"],
  );
  assert.deepEqual(cloudflareErrors({ success: false, errors: [] }), ["failed without a message"]);
  assert.match(cloudflareErrors(null)[0], /unexpected reply/);
});
