// Origin-private persistent storage and its narrow synchronous-Wasm host ABI.

export const FILE_KIND = Object.freeze({
  FILE: 1,
  DIRECTORY: 2,
  SYMBOLIC_LINK: 3,
  OTHER: 4,
});

export const WRITE_MODE = Object.freeze({
  TRUNCATE: 1,
  APPEND: 2,
  AT: 3,
  CREATE_NEW: 4,
});

export const HOST_STATUS = Object.freeze({
  OK: 0,
  NOT_FOUND: 1,
  ALREADY_EXISTS: 2,
  PERMISSION_DENIED: 3,
  QUOTA_EXCEEDED: 4,
  INVALID_INPUT: 5,
  IS_DIRECTORY: 6,
  NOT_A_DIRECTORY: 7,
  DIRECTORY_NOT_EMPTY: 8,
  UNSUPPORTED: 9,
  OTHER: 10,
});

class HostFileSystemError extends Error {
  constructor(status, message, options = {}) {
    super(message, options);
    this.name = "HostFileSystemError";
    this.status = status;
  }
}

function invalidInput(message) {
  return new HostFileSystemError(HOST_STATUS.INVALID_INPUT, message);
}

function normalizePath(path) {
  if (typeof path !== "string" || !path.startsWith("/")) {
    throw invalidInput("OPFS paths must be absolute");
  }
  if (path.includes("\0")) throw invalidInput("OPFS paths cannot contain NUL");
  const components = [];
  for (const component of path.split("/")) {
    if (!component || component === ".") continue;
    if (component === "..") {
      if (!components.length) throw invalidInput("OPFS path escapes its root");
      components.pop();
    } else {
      components.push(component);
    }
  }
  return components;
}

function isLookupMiss(error) {
  return error?.name === "NotFoundError" || error?.name === "TypeMismatchError";
}

function statusForError(error) {
  if (error instanceof HostFileSystemError) return error.status;
  switch (error?.name) {
    case "NotFoundError":
      return HOST_STATUS.NOT_FOUND;
    case "InvalidModificationError":
      return HOST_STATUS.DIRECTORY_NOT_EMPTY;
    case "NoModificationAllowedError":
    case "NotAllowedError":
    case "SecurityError":
      return HOST_STATUS.PERMISSION_DENIED;
    case "QuotaExceededError":
      return HOST_STATUS.QUOTA_EXCEEDED;
    case "TypeMismatchError":
      return HOST_STATUS.OTHER;
    case "NotSupportedError":
      return HOST_STATUS.UNSUPPORTED;
    default:
      return HOST_STATUS.OTHER;
  }
}

async function childHandle(directory, name) {
  try {
    return await directory.getFileHandle(name);
  } catch (error) {
    if (!isLookupMiss(error)) throw error;
  }
  try {
    return await directory.getDirectoryHandle(name);
  } catch (error) {
    if (isLookupMiss(error)) {
      throw new HostFileSystemError(HOST_STATUS.NOT_FOUND, `OPFS entry does not exist: ${name}`, {
        cause: error,
      });
    }
    throw error;
  }
}

async function directoryFor(root, components, create) {
  let directory = root;
  for (const name of components) {
    try {
      directory = await directory.getDirectoryHandle(name, { create });
    } catch (error) {
      if (error?.name === "TypeMismatchError") {
        throw new HostFileSystemError(
          HOST_STATUS.NOT_A_DIRECTORY,
          `OPFS path component is not a directory: ${name}`,
          { cause: error },
        );
      }
      throw error;
    }
  }
  return directory;
}

async function parentAndName(root, path) {
  const components = normalizePath(path);
  const name = components.pop();
  if (name === undefined) throw invalidInput("operation is not valid for the OPFS root");
  return {
    parent: await directoryFor(root, components, false),
    name,
  };
}

/** A complete-file adapter over the browser's origin-private filesystem. */
export class OriginPrivateFileSystem {
  constructor(root) {
    this.root = root;
  }

  static async open(storageManager = globalThis.navigator?.storage) {
    if (!storageManager?.getDirectory) {
      throw new HostFileSystemError(
        HOST_STATUS.UNSUPPORTED,
        "this browser does not provide origin-private filesystem storage",
      );
    }
    return new OriginPrivateFileSystem(await storageManager.getDirectory());
  }

  async lookup(path) {
    const components = normalizePath(path);
    if (!components.length) return this.root;
    const name = components.pop();
    const parent = await directoryFor(this.root, components, false);
    return childHandle(parent, name);
  }

  async stat(path) {
    const handle = await this.lookup(path);
    if (handle.kind === "directory") {
      return { kind: FILE_KIND.DIRECTORY, len: 0, modifiedMilliseconds: null };
    }
    if (handle.kind !== "file") {
      return { kind: FILE_KIND.OTHER, len: 0, modifiedMilliseconds: null };
    }
    const file = await handle.getFile();
    return {
      kind: FILE_KIND.FILE,
      len: file.size,
      modifiedMilliseconds: file.lastModified,
    };
  }

  async read(path) {
    const handle = await this.lookup(path);
    if (handle.kind !== "file") {
      throw new HostFileSystemError(HOST_STATUS.IS_DIRECTORY, `OPFS path is not a file: ${path}`);
    }
    return new Uint8Array(await (await handle.getFile()).arrayBuffer());
  }

  async readDirectory(path) {
    const handle = await this.lookup(path);
    if (handle.kind !== "directory") {
      throw new HostFileSystemError(
        HOST_STATUS.NOT_A_DIRECTORY,
        `OPFS path is not a directory: ${path}`,
      );
    }
    const names = [];
    for await (const [name] of handle.entries()) names.push(name);
    return names;
  }

  async write(path, contents, request) {
    const { parent, name } = await parentAndName(this.root, path);
    if (!Object.values(WRITE_MODE).includes(request.mode)) {
      throw invalidInput(`unknown OPFS write mode ${request.mode}`);
    }
    if (request.mode === WRITE_MODE.CREATE_NEW) {
      try {
        await childHandle(parent, name);
        throw new HostFileSystemError(
          HOST_STATUS.ALREADY_EXISTS,
          `OPFS entry already exists: ${path}`,
        );
      } catch (error) {
        if (!(error instanceof HostFileSystemError) || error.status !== HOST_STATUS.NOT_FOUND) {
          throw error;
        }
      }
    }
    let fileHandle;
    try {
      fileHandle = await parent.getFileHandle(name, { create: true });
    } catch (error) {
      if (error?.name === "TypeMismatchError") {
        throw new HostFileSystemError(HOST_STATUS.IS_DIRECTORY, `OPFS path is a directory: ${path}`, {
          cause: error,
        });
      }
      throw error;
    }
    if (typeof fileHandle.createSyncAccessHandle !== "function") {
      throw new HostFileSystemError(
        HOST_STATUS.UNSUPPORTED,
        "this browser does not provide Worker OPFS sync access handles",
      );
    }
    const access = await fileHandle.createSyncAccessHandle();
    try {
      let offset = 0;
      if (request.mode === WRITE_MODE.TRUNCATE || request.mode === WRITE_MODE.CREATE_NEW) {
        access.truncate(0);
      } else if (request.mode === WRITE_MODE.APPEND) {
        offset = access.getSize();
      } else {
        offset = request.offset;
      }
      const written = access.write(contents, { at: offset });
      if (written !== contents.byteLength) {
        throw new HostFileSystemError(
          HOST_STATUS.OTHER,
          `short OPFS write: wrote ${written} of ${contents.byteLength} bytes`,
        );
      }
      if (request.sync) access.flush();
    } finally {
      access.close();
    }
    return this.stat(path);
  }

  async createDirectory(path, parents) {
    const components = normalizePath(path);
    if (!components.length) {
      if (parents) return;
      throw new HostFileSystemError(HOST_STATUS.ALREADY_EXISTS, "OPFS root already exists");
    }
    if (parents) {
      await directoryFor(this.root, components, true);
      return;
    }
    const name = components.pop();
    const parent = await directoryFor(this.root, components, false);
    try {
      await childHandle(parent, name);
      throw new HostFileSystemError(
        HOST_STATUS.ALREADY_EXISTS,
        `OPFS entry already exists: ${path}`,
      );
    } catch (error) {
      if (!(error instanceof HostFileSystemError) || error.status !== HOST_STATUS.NOT_FOUND) {
        throw error;
      }
    }
    await parent.getDirectoryHandle(name, { create: true });
  }

  async removeFile(path) {
    const { parent, name } = await parentAndName(this.root, path);
    const handle = await childHandle(parent, name);
    if (handle.kind !== "file") {
      throw new HostFileSystemError(HOST_STATUS.IS_DIRECTORY, `OPFS path is a directory: ${path}`);
    }
    await parent.removeEntry(name);
  }

  async removeDirectory(path, recursive) {
    const { parent, name } = await parentAndName(this.root, path);
    const handle = await childHandle(parent, name);
    if (handle.kind !== "directory") {
      throw new HostFileSystemError(
        HOST_STATUS.NOT_A_DIRECTORY,
        `OPFS path is not a directory: ${path}`,
      );
    }
    await parent.removeEntry(name, { recursive });
  }

  async rename(from, to, replace) {
    const source = await this.lookup(from);
    if (typeof source.move !== "function") {
      throw new HostFileSystemError(
        HOST_STATUS.UNSUPPORTED,
        "this browser does not provide atomic OPFS moves",
      );
    }
    const destination = await parentAndName(this.root, to);
    try {
      await childHandle(destination.parent, destination.name);
      if (!replace) {
        throw new HostFileSystemError(
          HOST_STATUS.ALREADY_EXISTS,
          `OPFS destination already exists: ${to}`,
        );
      }
      await destination.parent.removeEntry(destination.name, { recursive: true });
    } catch (error) {
      if (!(error instanceof HostFileSystemError) || error.status !== HOST_STATUS.NOT_FOUND) {
        throw error;
      }
    }
    await source.move(destination.parent, destination.name);
  }

  async canonicalize(path) {
    await this.lookup(path);
    return `/${normalizePath(path).join("/")}`;
  }
}

/**
 * Build the raw imports consumed by `BrowserOpfsFileSystem` in the editor
 * Worker. Operation calls are asynchronous; the caller wraps them in JSPI's
 * `WebAssembly.Suspending` before instantiation.
 */
export function createOpfsHostImports(filesystem, getMemory) {
  const decoder = new TextDecoder("utf-8", { fatal: true });
  const encoder = new TextEncoder();
  let resultBytes = new Uint8Array();
  let resultMetadata = null;
  let resultError = "";

  function memoryBytes(source, length) {
    return new Uint8Array(getMemory().buffer, source, length);
  }

  function pathFromMemory(source, length) {
    try {
      return decoder.decode(memoryBytes(source, length));
    } catch (error) {
      throw new HostFileSystemError(HOST_STATUS.INVALID_INPUT, "path is not valid UTF-8", {
        cause: error,
      });
    }
  }

  async function perform(operation) {
    resultBytes = new Uint8Array();
    resultMetadata = null;
    resultError = "";
    try {
      await operation();
      return HOST_STATUS.OK;
    } catch (error) {
      resultError = error instanceof Error ? error.message : String(error);
      return statusForError(error);
    }
  }

  function copyResult(destination, capacity) {
    if (resultBytes.byteLength > capacity) return 0;
    memoryBytes(destination, resultBytes.byteLength).set(resultBytes);
    return resultBytes.byteLength;
  }

  return {
    fs_stat: (path, length) => perform(async () => {
      resultMetadata = await filesystem.stat(pathFromMemory(path, length));
    }),
    fs_read: (path, length) => perform(async () => {
      resultBytes = await filesystem.read(pathFromMemory(path, length));
    }),
    fs_read_directory: (path, length) => perform(async () => {
      resultBytes = encoder.encode(JSON.stringify(
        await filesystem.readDirectory(pathFromMemory(path, length)),
      ));
    }),
    fs_write: (path, pathLength, source, sourceLength, mode, offset, sync) =>
      perform(async () => {
        resultMetadata = await filesystem.write(
          pathFromMemory(path, pathLength),
          memoryBytes(source, sourceLength).slice(),
          { mode, offset, sync: sync !== 0 },
        );
      }),
    fs_create_directory: (path, length, parents) => perform(() =>
      filesystem.createDirectory(pathFromMemory(path, length), parents !== 0)),
    fs_remove_file: (path, length) => perform(() =>
      filesystem.removeFile(pathFromMemory(path, length))),
    fs_remove_directory: (path, length, recursive) => perform(() =>
      filesystem.removeDirectory(pathFromMemory(path, length), recursive !== 0)),
    fs_rename: (from, fromLength, to, toLength, replace) => perform(() =>
      filesystem.rename(
        pathFromMemory(from, fromLength),
        pathFromMemory(to, toLength),
        replace !== 0,
      )),
    fs_canonicalize: (path, length) => perform(async () => {
      resultBytes = encoder.encode(await filesystem.canonicalize(pathFromMemory(path, length)));
    }),
    fs_result_kind: () => resultMetadata?.kind ?? 0,
    fs_result_len: () => resultMetadata?.len ?? resultBytes.byteLength,
    fs_result_modified_milliseconds: () => resultMetadata?.modifiedMilliseconds ?? Number.NaN,
    fs_result_error_len: () => encoder.encode(resultError).byteLength,
    fs_copy_result: copyResult,
    fs_copy_result_error: (destination, capacity) => {
      const previous = resultBytes;
      resultBytes = encoder.encode(resultError);
      const copied = copyResult(destination, capacity);
      resultBytes = previous;
      return copied;
    },
  };
}
