import assert from "node:assert/strict";
import { createServer } from "node:http";
import test from "node:test";

import { fetchHttp } from "./http.mjs";
import { createHttpHostImports } from "./host.mjs";

async function endpoint(t, handler) {
  const server = createServer(handler);
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  t.after(() => new Promise((resolve) => {
    server.closeAllConnections();
    server.close(resolve);
  }));
  return `http://127.0.0.1:${server.address().port}`;
}

test("HTTP retrieval preserves binary error-page bytes and response metadata", async (t) => {
  const url = await endpoint(t, (_request, response) => {
    response.writeHead(404, { "Content-Type": "application/octet-stream" });
    response.end(Buffer.from([0, 128, 255, 10]));
  });

  const response = await fetchHttp({ url });
  assert.equal(response.status, 404);
  assert.equal(response.url, `${url}/`);
  assert.equal(new Map(response.headers).get("content-type"), "application/octet-stream");
  assert.deepEqual(response.body, new Uint8Array([0, 128, 255, 10]));
});

test("cancelling a response interrupts body consumption", { timeout: 5000 }, async (t) => {
  // Control the external Fetch boundary, not timing on the server: the second
  // pull proves that the consumer has read the first chunk and needs more.
  let waitingForMore;
  const consumed = new Promise((resolve) => { waitingForMore = resolve; });
  t.mock.method(globalThis, "fetch", async (_url, { signal }) => {
    let sent = false;
    const stream = new ReadableStream({
      start(controller) {
        signal.addEventListener("abort", () => controller.error(signal.reason), { once: true });
      },
      pull(controller) {
        if (!sent) {
          sent = true;
          controller.enqueue(new Uint8Array([1, 2, 3]));
        } else {
          waitingForMore();
        }
      },
    }, { highWaterMark: 0 });
    return new Response(stream);
  });
  const controller = new AbortController();
  const pending = fetchHttp({ url: "https://fixture.invalid" }, { signal: controller.signal });
  const rejected = assert.rejects(pending, { name: "AbortError" });
  await consumed;
  controller.abort();
  await rejected;
});

test("POST sends the supplied method, headers, and binary body", async (t) => {
  const url = await endpoint(t, async (request, response) => {
    const chunks = [];
    for await (const chunk of request) chunks.push(chunk);
    response.writeHead(request.method === "POST" && request.headers["x-test"] === "yes" ? 201 : 400);
    response.end(Buffer.concat(chunks));
  });
  const response = await fetchHttp({
    url, method: "POST", headers: [["X-Test", "yes"]], body: new Uint8Array([0, 255]),
  });
  assert.equal(response.status, 201);
  assert.deepEqual(response.body, new Uint8Array([0, 255]));
});

test("redirects return the final document URL for resolving relative links", async (t) => {
  const url = await endpoint(t, (request, response) => {
    if (request.url === "/") {
      response.writeHead(302, { Location: "/document/" });
      response.end();
    } else {
      response.end("document");
    }
  });
  const response = await fetchHttp({ url });
  assert.equal(response.url, `${url}/document/`);
  assert.equal(new TextDecoder().decode(response.body), "document");
});

test("empty responses and bodies exactly at the limit are accepted", async (t) => {
  const url = await endpoint(t, (request, response) => {
    if (request.url === "/empty") response.writeHead(204);
    response.end(request.url === "/empty" ? undefined : "123");
  });
  assert.equal((await fetchHttp({ url: `${url}/empty` }, { maxResponseBytes: 0 })).body.length, 0);
  assert.deepEqual((await fetchHttp({ url }, { maxResponseBytes: 3 })).body, new Uint8Array([49, 50, 51]));
});

test("invalid response limits fail before issuing a request", async () => {
  for (const maxResponseBytes of [-1, NaN, Infinity, 0.5]) {
    await assert.rejects(fetchHttp({ url: "https://invalid.invalid" }, { maxResponseBytes }), {
      name: "RangeError",
    });
  }
});

test("response limit is enforced without trusting Content-Length", async (t) => {
  const url = await endpoint(t, (_request, response) => {
    response.write("123");
    response.end("456");
  });
  await assert.rejects(fetchHttp({ url }, { maxResponseBytes: 5 }), {
    name: "HttpResponseTooLargeError",
  });
});

test("HTTP transport rejects non-HTTP URLs", async () => {
  await assert.rejects(fetchHttp({ url: "data:text/plain,not-http" }), {
    name: "TypeError",
  });
});

test("worker HTTP requests complete as owned metadata and binary bytes", async (t) => {
  const url = await endpoint(t, (_request, response) => response.end(Buffer.from([0, 255])));
  const memory = new WebAssembly.Memory({ initial: 2 });
  let wake;
  const completed = new Promise((resolve) => { wake = resolve; });
  const host = createHttpHostImports(() => memory, wake);
  const request = new TextEncoder().encode(JSON.stringify({url, method: "GET", headers: [], hasBody: false}));
  new Uint8Array(memory.buffer).set(request);
  const id = host.http_start(0, request.length, 0, 0);
  assert.ok(id > 0);
  assert.equal(host.http_poll(id), 0);
  await completed;
  assert.equal(host.http_poll(id), 1);
  assert.equal(host.http_result_len(id, 1), 2);
  assert.equal(host.http_copy_result(id, 1, 1024, 2), 2);
  assert.deepEqual(new Uint8Array(memory.buffer, 1024, 2), new Uint8Array([0, 255]));
  host.http_cancel(id);
  assert.equal(host.http_poll(id), 3);
});
