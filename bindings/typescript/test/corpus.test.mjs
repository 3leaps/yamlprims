// SPDX-License-Identifier: MIT OR Apache-2.0
// Conformance vector parity harness for @3leaps/yamlprims against
// conformance/vectors/cases.jsonl (wire output, decodeWire round-trip, and plain JSON).
import assert from "node:assert/strict";
import { readFileSync, writeFileSync } from "node:fs";
import { test } from "node:test";

import {
  YamlprimsError,
  decodeWire,
  defaultEmitOptions,
  defaultOptions,
  dumpAll,
  encode,
  encodeError,
  encodePlain,
  loadAllBytes,
  loadBytes,
  loadIjson,
  loadPlain,
  validateOptions,
} from "@3leaps/yamlprims";

const vectorsPath = process.env.YAMLPRIMS_VECTORS_FILE;
assert.ok(vectorsPath, "YAMLPRIMS_VECTORS_FILE is required");

function hexToBytes(hex) {
  assert.equal(hex.length % 2, 0, "odd hex length");
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < hex.length; i += 2) {
    out[i / 2] = Number.parseInt(hex.slice(i, i + 2), 16);
  }
  return out;
}

function buildOptions(rec) {
  const rawMerge = rec.merge ?? "reject";
  const mergeKeys = rawMerge === "as-key" || rawMerge === "plain-key" ? "as_key" : rawMerge;
  const unknownTags = rec.unknown_tags === "reject" ? "reject" : "preserve";
  return validateOptions({
    mergeKeys,
    unknownTags,
    limits: rec.limits ?? {},
  });
}

function widenLimitsForRoundTrip(opts) {
  const def = defaultOptions();
  for (const k of Object.keys(def.limits)) {
    if (opts.limits[k] > def.limits[k]) {
      def.limits[k] = opts.limits[k];
    }
  }
  return def;
}

test("conformance/vectors/cases.jsonl wire, round-trip, and plain parity", () => {
  const rawLines = readFileSync(vectorsPath, "utf8")
    .split("\n")
    .filter((l) => l.trim().length > 0);
  assert.ok(rawLines.length >= 150, `expected >= 150 vectors, got ${rawLines.length}`);

  const defOpts = defaultOptions();
  const outLines = [];

  for (const line of rawLines) {
    const rec = JSON.parse(line);
    const id = rec.id;

    if (typeof rec.syntax_code === "string") {
      const err = new YamlprimsError({
        kind: "syntax",
        detail: { syntaxKind: rec.syntax_code },
        site: { source: { byteOffset: 0, line: 1, column: 1 } },
        message: `syntax error: ${rec.syntax_code}`,
      });
      const wire = encodeError(err, defOpts);
      const decoded = decodeWire(wire, defOpts);
      assert.equal(decoded.kind, "error", `${id}: expected error envelope`);
      const rt = encodeError(decoded.error, defOpts);
      assert.equal(rt, wire, `${id}: syntax_code round-trip mismatch`);
      outLines.push(JSON.stringify({ id, emit: "", ijson: "", plain: "", wire }));
      continue;
    }

    const inputBytes =
      typeof rec.input_hex === "string"
        ? hexToBytes(rec.input_hex)
        : new TextEncoder().encode(rec.input);
    const opts = buildOptions(rec);
    const rtOpts = widenLimitsForRoundTrip(opts);
    const single = Boolean(rec.single);
    const checkPlain = Boolean(rec.check_plain);

    let wire;
    try {
      const docs = single ? [loadBytes(inputBytes, opts)] : loadAllBytes(inputBytes, opts);
      try {
        wire = encode(docs, opts);
      } catch (encErr) {
        if (!(encErr instanceof YamlprimsError)) throw encErr;
        wire = encodeError(encErr, rtOpts);
      }
    } catch (loadErr) {
      if (!(loadErr instanceof YamlprimsError)) throw loadErr;
      wire = encodeError(loadErr, rtOpts);
    }

    const decoded = decodeWire(wire, rtOpts, inputBytes);
    const rt =
      decoded.kind === "documents"
        ? encode(decoded.documents, rtOpts)
        : encodeError(decoded.error, rtOpts);
    assert.equal(rt, wire, `${id}: decodeWire -> encode round-trip mismatch`);

    let plain = "";
    if (checkPlain) {
      let plainViaHost;
      try {
        const loadOptsForPlain = {
          ...opts,
          limits: {
            ...opts.limits,
            maxOutputBytes: Math.max(opts.limits.maxOutputBytes, defOpts.limits.maxOutputBytes),
          },
        };
        const doc = loadBytes(inputBytes, loadOptsForPlain);
        plainViaHost = encodePlain(doc, opts);
      } catch (err) {
        if (!(err instanceof YamlprimsError)) throw err;
        plainViaHost = encodeError(err, rtOpts);
      }

      let plainViaWasm;
      try {
        plainViaWasm = loadPlain(inputBytes, opts);
      } catch (err) {
        if (!(err instanceof YamlprimsError)) throw err;
        plainViaWasm = encodeError(err, rtOpts);
      }

      assert.equal(
        plainViaWasm,
        plainViaHost,
        `${id}: loadPlain vs loadBytes+encodePlain mismatch`,
      );
      plain = plainViaHost;
    }

    const loadOptsWideOut = {
      ...opts,
      limits: {
        ...opts.limits,
        maxOutputBytes: Math.max(opts.limits.maxOutputBytes, defOpts.limits.maxOutputBytes),
      },
    };
    let docsForEmit = null;
    try {
      docsForEmit = single
        ? [loadBytes(inputBytes, loadOptsWideOut)]
        : loadAllBytes(inputBytes, loadOptsWideOut);
    } catch {
      docsForEmit = null;
    }

    let emit = "";
    if (docsForEmit !== null) {
      try {
        emit = dumpAll(docsForEmit, defaultEmitOptions());
      } catch (err) {
        if (!(err instanceof YamlprimsError)) throw err;
        emit = encodeError(err, rtOpts);
      }
    }

    let ijson = "";
    try {
      ijson = loadIjson(inputBytes, {
        mergeKeys: opts.mergeKeys,
        unknownTags: "reject",
        limits: opts.limits,
      });
    } catch (err) {
      if (!(err instanceof YamlprimsError)) throw err;
      ijson = encodeError(err, rtOpts);
    }

    outLines.push(JSON.stringify({ id, emit, ijson, plain, wire }));
  }

  if (process.env.YAMLPRIMS_RESULTS_OUT) {
    writeFileSync(process.env.YAMLPRIMS_RESULTS_OUT, outLines.join("\n") + "\n", "utf8");
  }
});
