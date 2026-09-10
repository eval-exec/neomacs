import assert from "node:assert/strict";
import test from "node:test";
import { WorkerInput } from "./worker-input.mjs";
import { WorkerWait, HostWake } from "./worker-wait.mjs";

test("rejected input is consumed once and does not keep the worker awake", async () => {
  const receipts = [];
  const input = new WorkerInput(receipt => receipts.push(receipt));
  const wait = new WorkerWait(() => input.bytes() !== null);
  input.enqueue({ sequence: "1", events: [{ type: "invalid" }] });
  assert.equal(await wait.wait(0), HostWake.Input);
  input.reject("invalid browser input batch");
  assert.equal(await wait.wait(0), HostWake.TimedOut);
  assert.deepEqual(receipts, [{ type: "input-rejected", sequence: "1", message: "invalid browser input batch" }]);
  assert.equal(input.reject("again"), false);
  assert.equal(receipts.length, 1);
  input.enqueue({ sequence: "2", events: [] });
  assert.equal(await wait.wait(0), HostWake.Input);
  assert.equal(input.accept("2"), true);
  assert.equal(input.bytes(), null);
  assert.equal(receipts[1].type, "input-accepted");
});

test("a receipt for another batch cannot drop pending input", () => {
  const input = new WorkerInput(() => {});
  input.enqueue({ sequence: "4", events: [] });
  assert.equal(input.accept("3"), false);
  assert.notEqual(input.bytes(), null);
  assert.throws(() => input.enqueue({ sequence: "5", events: [] }), /pending/);
  assert.equal(input.accept("4"), true);
});

test("malformed envelopes can be rejected even without a valid sequence", () => {
  const receipts = [];
  const input = new WorkerInput(receipt => receipts.push(receipt));
  input.enqueue({ events: [] });
  assert.equal(input.reject("missing sequence"), true);
  assert.equal(input.bytes(), null);
  assert.equal(receipts[0].sequence, null);
});
