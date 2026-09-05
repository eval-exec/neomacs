import assert from "node:assert/strict";
import test from "node:test";
import { WorkerWait, HostWake } from "./worker-wait.mjs";

test("HTTP completion wakes a suspended worker without claiming keyboard input", async () => {
  const wait = new WorkerWait(() => false);
  const resumed = wait.wait(1000);
  wait.notify();
  assert.equal(await resumed, HostWake.Ready);
});

test("queued keyboard input takes precedence over a host completion", async () => {
  let input = false;
  const wait = new WorkerWait(() => input);
  const resumed = wait.wait(1000);
  input = true;
  wait.notify();
  assert.equal(await resumed, HostWake.Input);
  assert.equal(await wait.wait(1000), HostWake.Input);
});

test("timeout and notifications outside a wait do not leak into the next wait", async () => {
  const wait = new WorkerWait(() => false);
  wait.notify();
  assert.equal(await wait.wait(0), HostWake.TimedOut);
  const resumed = wait.wait(1000);
  wait.notify();
  assert.equal(await resumed, HostWake.Ready);
});
