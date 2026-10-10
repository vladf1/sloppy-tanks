import { test } from "node:test";
import assert from "node:assert/strict";
import { namecheapErrors, namecheapRecord } from "../scripts/point-hostname.mjs";

test("Namecheap names the record by its host within the domain", () => {
  assert.deepEqual(namecheapRecord("sloppy-tanks-server.fridman.me"), {
    host: "sloppy-tanks-server",
    domain: "fridman.me",
  });
  assert.deepEqual(namecheapRecord("a.b.fridman.me"), { host: "a.b", domain: "fridman.me" });
  assert.throws(() => namecheapRecord("fridman.me"), /Not a subdomain/);
});

test("only a reply with no errors counts as an update", () => {
  assert.deepEqual(
    namecheapErrors(
      '<?xml version="1.0"?><interface-response><Command>SETDNSHOST</Command>' +
        "<IP>108.61.218.248</IP><ErrCount>0</ErrCount><errors /><Done>true</Done></interface-response>",
    ),
    [],
  );
  assert.deepEqual(
    namecheapErrors(
      "<interface-response><ErrCount>1</ErrCount><errors><Err1>Passwords do not match</Err1>" +
        "</errors><Done>true</Done></interface-response>",
    ),
    ["Passwords do not match"],
  );
  assert.match(namecheapErrors("<html>Bad gateway</html>")[0], /unexpected reply/);
});
