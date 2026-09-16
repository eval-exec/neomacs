// Navigation stays on the browser main thread; the editor only sends URL data.
function webUrl(value) {
  const url = new URL(value);
  if (url.protocol !== "https:" && url.protocol !== "http:") {
    throw new Error("Only HTTP and HTTPS links can open in a browser tab");
  }
  return url.href;
}

export function createNavigationHostImports(memory, post) {
  const decoder = new TextDecoder("utf-8", { fatal: true });
  return {
    open_external_url(source, length) {
      try {
        const value = decoder.decode(new Uint8Array(memory().buffer, source, length));
        post(webUrl(value));
        return 1;
      } catch {
        return 0;
      }
    },
  };
}

export function openExternalUrl(value, open = globalThis.open.bind(globalThis)) {
  const url = webUrl(value);
  // Opening a blank tab lets us distinguish popup rejection from success.
  // `open(url, ..., "noopener")` returns null in BOTH cases. Detach the blank
  // tab synchronously before navigating so the destination has no opener.
  const tab = open("about:blank", "_blank");
  if (!tab) return false;
  tab.opener = null;
  tab.location.replace(url);
  return true;
}
