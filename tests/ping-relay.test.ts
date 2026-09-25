import { test } from "node:test";
import assert from "node:assert/strict";
import { relayRoomSocket } from "../server/ping-relay";

class Socket extends EventTarget {
  sent: (string | ArrayBuffer)[] = [];
  closes: { code: number; reason: string }[] = [];
  failSend = false;
  send(data: string | ArrayBuffer): void {
    if (this.failSend) throw new Error("closed");
    this.sent.push(data);
  }
  close(code: number, reason: string): void {
    this.closes.push({ code, reason });
  }
  message(data: string | ArrayBuffer): void {
    this.dispatchEvent(new MessageEvent("message", { data }));
  }
}
function setup() {
  const client = new Socket(),
    room = new Socket();
  let clock = 100;
  relayRoomSocket(client, room, () => clock);
  return { client, room, setTime: (value: number) => (clock = value) };
}

test("relay times pongs on the Worker clock and preserves unrelated traffic", () => {
  const h = setup();
  const ping = JSON.stringify({ type: "ping", t: 987654, observedTick: 12, roundId: 1 });
  h.client.message(ping);
  assert.equal(h.room.sent[0], ping);
  h.setTime(107.5);
  h.room.message('{"type":"pong","t":987654,"tick":13}');
  assert.deepEqual(JSON.parse(String(h.client.sent[0])), {
    type: "pong",
    t: 987654,
    tick: 13,
    workerToRoomMs: 7.5,
  });
  for (const data of [
    '{"type":"snapshot","padding":"' + "x".repeat(2048) + '"}',
    "null",
    "invalid",
    new ArrayBuffer(4),
  ]) {
    h.room.message(data);
    assert.equal(h.client.sent.at(-1), data);
  }
  for (const data of ['{"type":"input","seq":2}', "invalid", "null"]) {
    h.client.message(data);
    assert.equal(h.room.sent.at(-1), data);
  }
  h.room.message('{"type":"pong","t":987654,"tick":14}');
  assert.equal(h.client.sent.at(-1), '{"type":"pong","t":987654,"tick":14}');
});

test("relay matches outstanding timestamps independently and bounds pending records", () => {
  const h = setup();
  for (let i = 0; i < 20; i++) {
    h.setTime(100 + i);
    h.client.message(JSON.stringify({ type: "ping", t: i }));
  }
  h.setTime(130);
  h.room.message('{"type":"pong","t":0}');
  assert.equal(h.client.sent.at(-1), '{"type":"pong","t":0}');
  for (const [t, expected] of [
    [19, 11],
    [4, 26],
  ]) {
    h.room.message(JSON.stringify({ type: "pong", t }));
    assert.equal(JSON.parse(String(h.client.sent.at(-1))).workerToRoomMs, expected);
  }
});

test("relay propagates closure codes, normalizes abnormal closure and stops forwarding", () => {
  for (const [code, expected] of [
    [4001, 4001],
    [1006, 1011],
    [1012, 1012],
  ]) {
    const h = setup();
    const event = new Event("close");
    Object.assign(event, { code, reason: "test close" });
    h.room.dispatchEvent(event);
    h.client.dispatchEvent(event);
    h.client.message('{"type":"ping","t":1}');
    h.room.message('{"type":"pong","t":1}');
    for (const socket of [h.client, h.room]) {
      assert.deepEqual(socket.closes, [{ code: expected, reason: "test close" }]);
      assert.equal(socket.sent.length, 0);
    }
  }
});

test("relay closes both peers on errors and send failures", () => {
  for (const fail of [
    (h: ReturnType<typeof setup>) => h.client.dispatchEvent(new Event("error")),
    (h: ReturnType<typeof setup>) => {
      h.room.failSend = true;
      h.client.message("{}");
    },
    (h: ReturnType<typeof setup>) => {
      h.client.failSend = true;
      h.room.message("{}");
    },
  ]) {
    const h = setup();
    fail(h);
    assert.equal(h.client.closes.length, 1);
    assert.deepEqual(h.client.closes, h.room.closes);
  }
});
