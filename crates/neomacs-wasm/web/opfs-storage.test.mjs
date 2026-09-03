import assert from "node:assert/strict";
import test from "node:test";

import {
  FILE_KIND,
  OriginPrivateFileSystem,
  WRITE_MODE,
} from "./opfs-storage.mjs";

class FakeFileHandle {
  constructor(name) {
    this.kind = "file";
    this.name = name;
    this.bytes = new Uint8Array();
    this.lastModified = 1;
  }

  async getFile() {
    const bytes = this.bytes.slice();
    return {
      size: bytes.byteLength,
      lastModified: this.lastModified,
      arrayBuffer: async () => bytes.buffer,
    };
  }

  async createSyncAccessHandle() {
    return {
      getSize: () => this.bytes.byteLength,
      truncate: (length) => {
        const next = new Uint8Array(length);
        next.set(this.bytes.subarray(0, length));
        this.bytes = next;
      },
      write: (source, options = {}) => {
        const at = options.at ?? 0;
        const end = at + source.byteLength;
        if (this.bytes.byteLength < end) {
          const next = new Uint8Array(end);
          next.set(this.bytes);
          this.bytes = next;
        }
        this.bytes.set(source, at);
        this.lastModified += 1;
        return source.byteLength;
      },
      flush: () => {},
      close: () => {},
    };
  }
}

class FakeDirectoryHandle {
  constructor(name = "") {
    this.kind = "directory";
    this.name = name;
    this.children = new Map();
  }

  async getDirectoryHandle(name, options = {}) {
    const existing = this.children.get(name);
    if (existing?.kind === "directory") return existing;
    if (existing || !options.create) throw domError("NotFoundError");
    const directory = new FakeDirectoryHandle(name);
    this.children.set(name, directory);
    return directory;
  }

  async getFileHandle(name, options = {}) {
    const existing = this.children.get(name);
    if (existing?.kind === "file") return existing;
    if (existing || !options.create) throw domError("NotFoundError");
    const file = new FakeFileHandle(name);
    this.children.set(name, file);
    return file;
  }

  async removeEntry(name, options = {}) {
    const existing = this.children.get(name);
    if (!existing) throw domError("NotFoundError");
    if (existing.kind === "directory" && existing.children.size && !options.recursive) {
      throw domError("InvalidModificationError");
    }
    this.children.delete(name);
  }

  async *entries() {
    yield* this.children.entries();
  }
}

function domError(name) {
  return Object.assign(new Error(name), { name });
}

test("OPFS adapter persists complete writes across editor sessions", async () => {
  const root = new FakeDirectoryHandle();
  const storage = { getDirectory: async () => root };
  const first = await OriginPrivateFileSystem.open(storage);

  await first.createDirectory("/.emacs.d", false);
  await first.write("/.emacs.d/init.el", new TextEncoder().encode("alpha"), {
    mode: WRITE_MODE.TRUNCATE,
    offset: 0,
    sync: true,
  });

  const second = await OriginPrivateFileSystem.open(storage);
  assert.equal((await second.stat("/.emacs.d")).kind, FILE_KIND.DIRECTORY);
  assert.equal(
    new TextDecoder().decode(await second.read("/.emacs.d/init.el")),
    "alpha",
  );
});

test("OPFS adapter implements append and positioned writes without truncation", async () => {
  const filesystem = await OriginPrivateFileSystem.open({
    getDirectory: async () => new FakeDirectoryHandle(),
  });
  await filesystem.write("/note", new TextEncoder().encode("alpha"), {
    mode: WRITE_MODE.CREATE_NEW,
    offset: 0,
    sync: false,
  });
  await filesystem.write("/note", new TextEncoder().encode("-beta"), {
    mode: WRITE_MODE.APPEND,
    offset: 0,
    sync: false,
  });
  await filesystem.write("/note", new TextEncoder().encode("ALPHA"), {
    mode: WRITE_MODE.AT,
    offset: 0,
    sync: true,
  });
  assert.equal(new TextDecoder().decode(await filesystem.read("/note")), "ALPHA-beta");
});

test("OPFS adapter rejects paths outside its virtual root", async () => {
  const filesystem = await OriginPrivateFileSystem.open({
    getDirectory: async () => new FakeDirectoryHandle(),
  });
  await assert.rejects(filesystem.stat("/safe/../../outside"), /escapes its root/);
});

test("unsupported rename preserves an existing destination", async () => {
  const filesystem = await OriginPrivateFileSystem.open({
    getDirectory: async () => new FakeDirectoryHandle(),
  });
  await filesystem.write("/source", new TextEncoder().encode("source"), {
    mode: WRITE_MODE.TRUNCATE,
    offset: 0,
    sync: false,
  });
  await filesystem.write("/destination", new TextEncoder().encode("destination"), {
    mode: WRITE_MODE.TRUNCATE,
    offset: 0,
    sync: false,
  });

  await assert.rejects(
    filesystem.rename("/source", "/destination", true),
    /does not provide atomic OPFS moves/,
  );
  assert.equal(
    new TextDecoder().decode(await filesystem.read("/destination")),
    "destination",
  );
});
