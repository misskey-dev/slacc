import * as crypto from "node:crypto";
import { promisify } from "node:util";
import { beforeAll, describe, expect, it } from "vitest";
import { loadIsolatedSlaccBinding } from "./aws_lc_rs.loader.mjs";

describe("aws_lc_rs", () => {
  /** @type {import("../index.js")} */
  let slacc;

  beforeAll(async () => {
    slacc = await loadIsolatedSlaccBinding(`aws_lc_rs.test`);
    slacc.init(1);
  });

  describe("Signer", () => {
    describe.for(["Mldsa44", "Eddsa", "Rsa2048_8192"])(
      "%s",
      (suite) => {
        it("should sign correctly", async () => {
          const { publicKey, privateKey } = await promisify(
            crypto.generateKeyPair,
          )(
            ...{
              Mldsa44: [
                "ml-dsa-44",
                {
                  params: {
                    seed: Buffer.alloc(32, 0x42),
                  },
                },
              ],
              Eddsa: [
                "ed25519",
                {
                  params: {
                    seed: Buffer.alloc(32, 0x42),
                  },
                },
              ],
              Rsa2048_8192: [
                "rsa",
                {
                  modulusLength: 2048,
                  publicExponent: 0x10001,
                },
              ],
            }[suite],
          );
          const signer = slacc.Signer.fromPkcs8Der(
            suite,
            privateKey.export({ type: "pkcs8", format: "der" }),
          );
          expect(signer.publicKey).toBeInstanceOf(Buffer);
          expect(signer.publicKey.length).toBe(
            {
              Mldsa44: 1312,
              Eddsa: 32,
              Rsa2048_8192: 270,
            }[suite],
          );
          expect(signer.publicKey).toEqual(
            publicKey.export({ type: "spki", format: "der" }).subarray(
              {
                Mldsa44: -1312,
                Eddsa: -32,
                Rsa2048_8192: -270,
              }[suite],
            ),
          );
          const message = [
            Buffer.from('{"type":"DataIntegrityProof"}'),
            Buffer.from('{"body":"Hello, world!"}'),
          ];
          const sign = promisify(signer.signParts.bind(signer));
          const signature = await sign(message);
          expect(signature).toBeInstanceOf(Buffer);
          expect(signature.length).toBe(
            {
              Mldsa44: 2420,
              Eddsa: 64,
              Rsa2048_8192: 256,
            }[suite],
          );
          expect(
            await promisify(crypto.verify)(
              null,
              Buffer.concat(
                message.map((value) =>
                  crypto.createHash("sha256").update(value).digest(),
                ),
              ),
              publicKey,
              signature,
            ),
          ).toBe(true);
        });
      },
    );
  });

  describe("Verifier", () => {
    describe.for(["Mldsa44", "Eddsa", "Rsa2048_8192"])(
      "%s",
      (suite) => {
        it("should verify correctly", async () => {
          const { publicKey, privateKey } = await promisify(
            crypto.generateKeyPair,
          )(
            ...{
              Mldsa44: [
                "ml-dsa-44",
                {
                  params: {
                    seed: Buffer.alloc(32, 0x42),
                  },
                },
              ],
              Eddsa: [
                "ed25519",
                {
                  params: {
                    seed: Buffer.alloc(32, 0x42),
                  },
                },
              ],
              Rsa2048_8192: [
                "rsa",
                {
                  modulusLength: 2048,
                  publicExponent: 0x10001,
                },
              ],
            }[suite],
          );
          const message = [
            Buffer.from('{"type":"DataIntegrityProof"}'),
            Buffer.from('{"body":"Hello, world!"}'),
          ];
          const signature = await promisify(crypto.sign)(
            null,
            Buffer.concat(
              message.map((value) =>
                crypto.createHash("sha256").update(value).digest(),
              ),
            ),
            privateKey,
          );
          const verifier = slacc.Verifier.fromSpkiDer(
            suite,
            publicKey.export({ type: "spki", format: "der" }),
          );
          const verify = promisify(verifier.verifyParts.bind(verifier));
          expect(await verify(signature, message)).toBe(true);
        });
      },
    );
  });
});
