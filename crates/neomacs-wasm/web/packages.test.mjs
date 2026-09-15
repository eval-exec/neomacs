import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { fetchPackageAssets } from "./packages.mjs";

test("verified package bytes are reused from cache without a second bundle download", async () => {
  const bytes = new TextEncoder().encode("package archive");
  const digest = createHash("sha256").update(bytes).digest("hex");
  const entries = new Map();
  const cache = { match: async key => entries.get(key)?.clone(),
    put: async (key, response) => entries.set(key, response.clone()) };
  let downloads = 0;
  const fetcher = async url => {
    if (String(url).endsWith("packages.json")) return Response.json({schema: 1, sha256: digest});
    downloads++;
    return new Response(bytes);
  };
  const url = "https://example.test/builds/test/packages.json";
  assert.deepEqual((await fetchPackageAssets(url, {fetcher, cache})).archive, bytes);
  assert.deepEqual((await fetchPackageAssets(url, {fetcher, cache})).archive, bytes);
  assert.equal(downloads, 1);
});

test("a corrupt download is not cached or exposed to the editor", async () => {
  let stored = false;
  const cache = {match: async () => undefined, put: async () => { stored = true; }};
  const fetcher = async url => String(url).endsWith("packages.json")
    ? Response.json({schema: 1, sha256: "0".repeat(64)}) : new Response("wrong bytes");
  await assert.rejects(fetchPackageAssets("https://example.test/packages.json", {fetcher, cache}), /digest/);
  assert.equal(stored, false);
});
