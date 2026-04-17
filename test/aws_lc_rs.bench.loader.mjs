import { createRequire } from "node:module";
import { copyFile, mkdtemp, readdir } from "node:fs/promises";
import { basename, join } from "node:path";
import { tmpdir } from "node:os";

const require = createRequire(import.meta.url);
const cacheKey = Symbol.for("slacc.bench.isolatedBindings");

function getBindingCache() {
  if (!globalThis[cacheKey]) {
    globalThis[cacheKey] = new Map();
  }

  return globalThis[cacheKey];
}

async function resolveNativeBinaryPath() {
  const files = await readdir(process.cwd());
  const candidates = files.filter((file) => /^slacc\..+\.node$/i.test(file));

  if (candidates.length !== 1) {
    throw new Error(
      "slacc native binary was not found or multiple candidates were found",
    );
  }

  return join(process.cwd(), candidates[0]);
}

export async function loadIsolatedSlaccBinding(namespace) {
  const cache = getBindingCache();

  if (cache.has(namespace)) {
    return cache.get(namespace);
  }

  const sourcePath = await resolveNativeBinaryPath();
  const tempDir = await mkdtemp(join(tmpdir(), `slacc-bench-${namespace}-`));
  const destinationPath = join(
    tempDir,
    basename(sourcePath, ".node") + `.${namespace}.node`,
  );

  await copyFile(sourcePath, destinationPath);

  const binding = require(destinationPath);

  cache.set(namespace, binding);

  return binding;
}
