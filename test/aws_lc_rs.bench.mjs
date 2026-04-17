import crypto from "node:crypto";
import { execFile } from "node:child_process";
import { watch } from "node:fs";
import { open, readFile, stat, unlink } from "node:fs/promises";
import os from "node:os";
import { basename, dirname, join } from "node:path";
import { promisify } from "node:util";
import { afterEach, beforeEach, bench, describe, expect } from "vitest";
import { loadIsolatedSlaccBinding } from "./aws_lc_rs.bench.loader.mjs";

const { privateKey, publicKey } = crypto.generateKeyPairSync("rsa", {
  modulusLength: 2048,
  publicExponent: 0x10001,
});

const privateKeyPem = privateKey
  .export({ type: "pkcs8", format: "pem" })
  .toString();
const privateKeyPkcs8Der = privateKey.export({ type: "pkcs8", format: "der" });
const execFileAsync = promisify(execFile);
const BENCH_CONFIG = {
  matrix: {
    payloadSizes: [256, 4096, 65536],
    keyPoolSizes: [1],
    payloadPoolSizes: [1],
  },
  pattern: {
    burstLevels: [1, 16, 256, 4096],
    targetTotalOps: 4096,
  },
  mutex: {
    fileName: ".vitest-bench-mutex.lock",
    timeoutMs: 1e4,
    staleMs: 1e4,
  },
  benchOptions: {},
};

async function readDarwinSysctlInt(key) {
  try {
    const { stdout } = await execFileAsync("sysctl", ["-n", key], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    });
    const parsed = Number.parseInt(stdout.trim(), 10);
    return Number.isFinite(parsed) && parsed > 0 ? parsed : null;
  } catch {
    return null;
  }
}

async function detectLinuxPhysicalCoreCount() {
  try {
    const { stdout } = await execFileAsync("lscpu", ["-p=CORE,SOCKET"], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    });

    const physicalCoreSet = new Set(
      stdout
        .split("\n")
        .map((line) => line.trim())
        .filter((line) => line && !line.startsWith("#"))
        .map((line) => line.split(","))
        .filter((parts) => parts.length >= 2)
        .map(([core, socket]) => `${socket}:${core}`),
    );

    return physicalCoreSet.size > 0 ? physicalCoreSet.size : null;
  } catch {
    return null;
  }
}

async function detectWindowsPhysicalCoreCount() {
  try {
    const { stdout } = await execFileAsync(
      "powershell",
      [
        "-NoProfile",
        "-Command",
        "(Get-CimInstance Win32_Processor | Measure-Object -Property NumberOfCores -Sum).Sum",
      ],
      {
        encoding: "utf8",
        stdio: ["ignore", "pipe", "ignore"],
      },
    );

    const parsed = Number.parseInt(stdout.trim(), 10);
    return Number.isFinite(parsed) && parsed > 0 ? parsed : null;
  } catch {
    return null;
  }
}

async function detectPCoreCount() {
  if (process.platform === "darwin") {
    return (
      (await readDarwinSysctlInt("hw.perflevel0.physicalcpu")) ??
      (await readDarwinSysctlInt("hw.physicalcpu")) ??
      Math.max(1, os.availableParallelism())
    );
  }

  if (process.platform === "linux") {
    return (
      (await detectLinuxPhysicalCoreCount()) ??
      Math.max(1, os.availableParallelism())
    );
  }

  if (process.platform === "win32") {
    return (
      (await detectWindowsPhysicalCoreCount()) ??
      Math.max(1, os.availableParallelism())
    );
  }

  return Math.max(1, os.availableParallelism());
}

const maxCoreCount = await detectPCoreCount();
const slaccThreadVariants = Array.from({ length: 31 }, (_, n) => 2 ** n).filter(
  (threads) => threads <= maxCoreCount,
);
const lockFile = join(process.cwd(), BENCH_CONFIG.mutex.fileName);
const lockDir = dirname(lockFile);
const lockName = basename(lockFile);
const mutexTimeoutMs = BENCH_CONFIG.mutex.timeoutMs;
const mutexStaleMs = BENCH_CONFIG.mutex.staleMs;
const benchOptions = BENCH_CONFIG.benchOptions;
const slaccVariantCacheKey = Symbol.for("slacc.bench.slaccVariantCache");

const patterns = BENCH_CONFIG.pattern.burstLevels.map((burst) => {
  const waves = Math.max(
    1,
    Math.floor(BENCH_CONFIG.pattern.targetTotalOps / burst),
  );
  return {
    name: `${waves}x${burst}`,
    burst,
    waves,
  };
});

async function waitForLockChange(timeoutMs) {
  await new Promise((resolve, reject) => {
    let finished = false;

    const finish = (callback) => {
      if (finished) {
        return;
      }
      finished = true;
      clearTimeout(timer);
      watcher.close();
      callback();
    };

    const timer = setTimeout(() => {
      finish(() =>
        reject(new Error("Timed out while waiting benchmark mutex")),
      );
    }, timeoutMs);

    const watcher = watch(lockDir, (_eventType, filename) => {
      if (!filename || filename === lockName) {
        finish(resolve);
      }
    });

    stat(lockFile).catch((error) => {
      if (error?.code === "ENOENT") {
        finish(resolve);
      }
    });

    watcher.on("error", (error) => {
      finish(() => reject(error));
    });
  });
}

async function tryAcquireLock(ownerToken) {
  const handle = await open(lockFile, "wx");
  await handle.writeFile(
    JSON.stringify({
      ownerToken,
      pid: process.pid,
      acquiredAt: new Date().toISOString(),
    }),
  );
  await handle.close();
}

async function acquireBenchMutex(timeoutMs) {
  const ownerToken = `${process.pid}:${Date.now()}:${Math.random().toString(16).slice(2)}`;
  const deadline = Date.now() + timeoutMs;

  const removeStaleLockIfNeeded = async () => {
    try {
      const lockStat = await stat(lockFile);
      if (Date.now() - lockStat.mtimeMs < mutexStaleMs) {
        return;
      }
      await unlink(lockFile);
    } catch (error) {
      if (error?.code !== "ENOENT") {
        throw error;
      }
    }
  };

  while (Date.now() < deadline) {
    try {
      await tryAcquireLock(ownerToken);

      return async () => {
        try {
          const content = await readFile(lockFile, "utf8");
          const lockInfo = JSON.parse(content);
          if (lockInfo.ownerToken !== ownerToken) {
            return;
          }
        } catch (error) {
          if (error?.code === "ENOENT") {
            return;
          }
          throw error;
        }

        try {
          await unlink(lockFile);
        } catch (error) {
          if (error?.code !== "ENOENT") {
            throw error;
          }
        }
      };
    } catch (error) {
      if (error?.code !== "EEXIST") {
        throw error;
      }

      await removeStaleLockIfNeeded();

      const remainingMs = deadline - Date.now();
      if (remainingMs <= 0) {
        break;
      }

      await waitForLockChange(remainingMs);
    }
  }

  throw new Error("Timed out while waiting benchmark mutex");
}

function createPayloadPool(payloadSize, payloadPoolSize) {
  const pool = [];

  for (let i = 0; i < payloadPoolSize; i += 1) {
    const payload = Buffer.alloc(payloadSize, i % 256);
    pool.push(payload);
  }

  return pool;
}

function createNodePrivateKeyPool(keyPoolSize) {
  return Array.from({ length: keyPoolSize }, () =>
    crypto.createPrivateKey({
      key: privateKeyPem,
      format: "pem",
      type: "pkcs8",
    }),
  );
}

function createSlaccKeyPool(keyPoolSize, SlaccRsaKeyPair) {
  return Array.from({ length: keyPoolSize }, () =>
    SlaccRsaKeyPair.fromPem(privateKeyPem),
  );
}

function createSlaccSignPool(slaccKeyPool) {
  return slaccKeyPool.map((keyPair) => promisify(keyPair.sign.bind(keyPair)));
}

async function loadSlaccVariants() {
  if (globalThis[slaccVariantCacheKey]) {
    return globalThis[slaccVariantCacheKey];
  }

  const variants = new Map();

  for (const numThreads of slaccThreadVariants) {
    const namespace = `threads-${numThreads}`;
    const slaccBinding = await loadIsolatedSlaccBinding(namespace);
    const { init: slaccInit, RsaKeyPair: SlaccRsaKeyPair } = slaccBinding;
    slaccInit(numThreads);
    variants.set(numThreads, SlaccRsaKeyPair);
  }

  globalThis[slaccVariantCacheKey] = variants;
  return variants;
}

async function createWebCryptoKeyPool(keyPoolSize) {
  return Promise.all(
    Array.from({ length: keyPoolSize }, async () =>
      crypto.webcrypto.subtle.importKey(
        "pkcs8",
        privateKeyPkcs8Der,
        { name: "RSASSA-PKCS1-v1_5", hash: "SHA-256" },
        false,
        ["sign"],
      ),
    ),
  );
}

async function runPattern(pattern, signer) {
  let results = [];
  let operationIndex = 0;

  const startInNextMicrotask = (index) =>
    new Promise((resolve, reject) => {
      queueMicrotask(() => {
        Promise.resolve()
          .then(() => signer(index))
          .then(resolve, reject);
      });
    });

  for (let i = 0; i < pattern.waves; i += 1) {
    const base = operationIndex;
    results = await Promise.all(
      Array.from({ length: pattern.burst }, (_, j) =>
        startInNextMicrotask(base + j),
      ),
    );
    operationIndex += pattern.burst;
  }

  return results;
}

function assertSignature(results) {
  expect(results.length).toBeGreaterThan(0);
  expect(
    crypto.verify(
      "sha256",
      results[0].payload,
      publicKey,
      results[0].signature,
    ),
  ).toBe(true);
  if (results.length > 1) {
    expect(
      crypto.verify(
        "sha256",
        results[results.length - 1].payload,
        publicKey,
        results[results.length - 1].signature,
      ),
    ).toBe(true);
  }
}

const slaccVariants = await loadSlaccVariants();

describe("aws_lc_rs", () => {
  for (const payloadSize of BENCH_CONFIG.matrix.payloadSizes) {
    describe(`payloadSize=${payloadSize}`, () => {
      for (const payloadPoolSize of BENCH_CONFIG.matrix.payloadPoolSizes) {
        const payloadPool = createPayloadPool(payloadSize, payloadPoolSize);

        describe(`payloadPool=${payloadPoolSize}`, () => {
          for (const keyPoolSize of BENCH_CONFIG.matrix.keyPoolSizes) {
            describe(`keyPool=${keyPoolSize}`, () => {
              for (const pattern of patterns) {
                describe(pattern.name, () => {
                  let releaseMutex = null;

                  beforeEach(async () => {
                    releaseMutex = await acquireBenchMutex(mutexTimeoutMs);
                  });

                  afterEach(async () => {
                    if (releaseMutex) {
                      await releaseMutex();
                      releaseMutex = null;
                    }
                  });

                  for (const slaccThreads of slaccThreadVariants) {
                    bench(
                      `aws_lc_rs ${slaccThreads} threads`,
                      async () => {
                        const SlaccRsaKeyPair = slaccVariants.get(slaccThreads);
                        const slaccKeyPool = createSlaccKeyPool(
                          keyPoolSize,
                          SlaccRsaKeyPair,
                        );
                        const slaccSignPool = createSlaccSignPool(slaccKeyPool);
                        const results = await runPattern(
                          pattern,
                          async (operationIndex) => {
                            const payload =
                              payloadPool[operationIndex % payloadPool.length];
                            const sign =
                              slaccSignPool[
                                operationIndex % slaccSignPool.length
                              ];
                            const signature = await sign(payload);
                            return { payload, signature };
                          },
                        );

                        assertSignature(results);
                      },
                      benchOptions,
                    );
                  }

                  bench(
                    "Node Crypto API",
                    async () => {
                      const nodeKeyPool = createNodePrivateKeyPool(keyPoolSize);
                      const results = await runPattern(
                        pattern,
                        async (operationIndex) => {
                          const payload =
                            payloadPool[operationIndex % payloadPool.length];
                          const key =
                            nodeKeyPool[operationIndex % nodeKeyPool.length];
                          const signature = crypto.sign("sha256", payload, key);
                          return { payload, signature };
                        },
                      );

                      assertSignature(results);
                    },
                    benchOptions,
                  );

                  bench(
                    "Web Crypto API",
                    async () => {
                      const webCryptoKeyPool =
                        await createWebCryptoKeyPool(keyPoolSize);
                      const results = await runPattern(
                        pattern,
                        async (operationIndex) => {
                          const payload =
                            payloadPool[operationIndex % payloadPool.length];
                          const key =
                            webCryptoKeyPool[
                              operationIndex % webCryptoKeyPool.length
                            ];
                          const signature = Buffer.from(
                            await crypto.webcrypto.subtle.sign(
                              { name: "RSASSA-PKCS1-v1_5" },
                              key,
                              payload,
                            ),
                          );
                          return { payload, signature };
                        },
                      );

                      assertSignature(results);
                    },
                    benchOptions,
                  );
                });
              }
            });
          }
        });
      }
    });
  }
});
