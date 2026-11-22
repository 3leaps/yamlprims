// SPDX-License-Identifier: MIT OR Apache-2.0
// Malformed-wire decode inventory and the engineDetail property
// (spec revision 1.7 §11.1, 1.7/E-6).
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import {
  YamlprimsError,
  createDocument,
  createTag,
  createTaggedNode,
  decodeWire,
  defaultOptions,
  encode,
  encodeError,
  loadBytes,
  validateOptions,
} from "@3leaps/yamlprims";

const casesPath = process.env.YAMLPRIMS_WIRE_DECODE_CASES_FILE;
assert.ok(casesPath, "YAMLPRIMS_WIRE_DECODE_CASES_FILE is required");
const EXPECTED_CASES = 31;
const REFUSAL = "invalid yamlprims wire payload";
const enc = new TextEncoder();

function decodeRecord(rec) {
  const wide = defaultOptions();
  let opts;
  try {
    opts = validateOptions({ limits: rec.limits ?? {} });
  } catch (err) {
    assert.ok(err instanceof YamlprimsError);
    return ["refused", encodeError(err, wide)];
  }
  let env;
  try {
    env = decodeWire(rec.input, opts);
  } catch (err) {
    assert.ok(err instanceof YamlprimsError, String(err));
    return ["refused", encodeError(err, wide)];
  }
  if (env.kind === "documents") return ["documents", encode(env.documents, wide)];
  return ["decoded_error", encodeError(env.error, wide)];
}

test("wire decode inventory meets every expectation", () => {
  const records = readFileSync(casesPath, "utf8")
    .split("\n")
    .filter((l) => l.trim())
    .map((l) => JSON.parse(l));
  assert.equal(records.length, EXPECTED_CASES);
  for (const rec of records) {
    const exp = rec.expect;
    const [outcome, wire] = decodeRecord(rec);
    assert.equal(outcome, exp.outcome, rec.id);
    if (exp.reencodes_input) assert.equal(wire, rec.input, rec.id);
    const env = JSON.parse(wire);
    if (exp.outcome === "documents") {
      assert.ok("documents" in env, rec.id);
      continue;
    }
    for (const key of ["kind", "detail", "site", "related", "path", "message"]) {
      if (key in exp) assert.deepEqual(env.error[key], exp[key], `${rec.id} ${key}`);
    }
  }
});

test("refusal keeps its reason out of the wire", () => {
  let caught;
  try {
    decodeWire("{}");
  } catch (e) {
    caught = e;
  }
  assert.ok(caught instanceof YamlprimsError);
  assert.equal(caught.kind, "internal");
  assert.equal(caught.message, REFUSAL);
  assert.equal(caught.rawMessage, REFUSAL);
  assert.ok(caught.engineDetail);
  assert.ok(!encodeError(caught, defaultOptions()).includes(caught.engineDetail));
  assert.ok(!Object.keys(caught).includes("engineDetail"));
  assert.ok(!JSON.stringify(caught).includes(caught.engineDetail));
});

test("engineDetail is absent unless set, and read-only", () => {
  let loadErr;
  try {
    loadBytes(enc.encode("a: ["), defaultOptions());
  } catch (e) {
    loadErr = e;
  }
  assert.ok(loadErr instanceof YamlprimsError);
  assert.equal(loadErr.engineDetail, undefined);
  const e = new YamlprimsError({
    kind: "internal",
    detail: {},
    site: { notSource: { lastKnown: null } },
    message: "x",
    engineDetail: "why",
  });
  // ES modules are strict: assigning a non-writable property throws.
  assert.throws(() => {
    e.engineDetail = "other";
  }, TypeError);
  assert.equal(e.engineDetail, "why");
});

test("engineDetail is bounded on a UTF-8 boundary", () => {
  const e = new YamlprimsError({
    kind: "internal",
    detail: {},
    site: { notSource: { lastKnown: null } },
    message: "x",
    engineDetail: "é".repeat(5000),
  });
  assert.ok(enc.encode(e.engineDetail).byteLength <= 4096);
  assert.ok(e.engineDetail.endsWith("…"));
  let caught;
  try {
    decodeWire(`{"format":"yamlprims-value","version":1,"${"k".repeat(20000)}":0,"documents":[]}`);
  } catch (err) {
    caught = err;
  }
  assert.equal(caught.message, REFUSAL);
  assert.ok(enc.encode(caught.engineDetail).byteLength <= 4096);
});

test("decoded error envelope keeps the caller message", () => {
  const text =
    '{"format":"yamlprims-value","version":1,"error":{"kind":"internal","detail":{},' +
    `"site":{"not_source":{"last_known":null}},"related":[],"path":[],"message":"${REFUSAL}"}}`;
  const env = decodeWire(text);
  assert.equal(env.kind, "error");
  assert.equal(env.error.message, REFUSAL);
  assert.equal(env.error.engineDetail, undefined);
  assert.equal(encodeError(env.error, defaultOptions()), text);
});

test("limits and options are not folded into the refusal", () => {
  let e1;
  try {
    decodeWire("nope", { limits: { maxInputBytes: 3 } });
  } catch (e) {
    e1 = e;
  }
  assert.equal(e1.kind, "limit_exceeded");
  assert.equal(e1.message, "wire payload exceeds max_input_bytes (3)");
  let e2;
  try {
    decodeWire("nope", { limits: { maxDepth: 0 } });
  } catch (e) {
    e2 = e;
  }
  assert.equal(e2.kind, "invalid_options");
});

test("constructor refusals match the shared table", () => {
  const tablePath = process.env.YAMLPRIMS_CONSTRUCTOR_CASES_FILE;
  assert.ok(tablePath, "YAMLPRIMS_CONSTRUCTOR_CASES_FILE is required");
  const table = JSON.parse(readFileSync(tablePath, "utf8"));
  const syn = { span: null, via: null, tagStart: null };
  const calls = {
    "tag.empty-uri": () => createTag(""),
    "tag.core-uri": () => createTag("tag:yaml.org,2002:str"),
    "tag.inconsistent-spelling": () => createTag("!x", "!!", "y"),
    "tagged.bool-inner": () => createTaggedNode("!t", { kind: "bool", value: true, ...syn }),
    "tagged.merge-sequence-inner": () =>
      createTaggedNode("tag:yaml.org,2002:merge", { kind: "sequence", items: [], ...syn }),
    "document.version-2-0": () => createDocument({ kind: "null", ...syn }, [2, 0], false),
  };
  assert.deepEqual(Object.keys(calls).sort(), table.cases.map((c) => c.id).sort());
  for (const c of table.cases) {
    let err;
    try {
      calls[c.id]();
    } catch (e) {
      err = e;
    }
    assert.ok(err instanceof YamlprimsError, `${c.id}: ${err}`);
    const want =
      `{"format":"yamlprims-value","version":1,"error":{"kind":"${c.kind}","detail":{},` +
      `"site":{"not_source":{"last_known":null}},"related":[],"path":[],"message":"${c.message}"}}`;
    assert.equal(encodeError(err, defaultOptions()), want, c.id);
  }
});

test("out-of-range metadata refusals match the shared table", () => {
  const table = JSON.parse(readFileSync(process.env.YAMLPRIMS_CONSTRUCTOR_CASES_FILE, "utf8"));
  const big = 2 ** 53;
  const ns = { notSource: { lastKnown: null } };
  const mk = (kind, extra) => new YamlprimsError({ kind, detail: {}, site: ns, message: "m", ...extra });
  const errors = {
    "path-index-2^53": mk("internal", { path: [{ index: big }] }),
    "site-offset-2^53": mk("internal", {
      site: { source: { byteOffset: big, utf16Offset: big, line: 1, column: 1 } },
    }),
    "configured-2^53": mk("limit_exceeded", { detail: { limit: "max_nodes", configured: big } }),
    "configured-zero": mk("limit_exceeded", { detail: { limit: "max_nodes", configured: 0 } }),
  };
  assert.deepEqual(Object.keys(errors).sort(), table.encode_error_cases.map((c) => c.id).sort());
  for (const c of table.encode_error_cases) {
    let refused;
    try {
      encodeError(errors[c.id], defaultOptions());
    } catch (e) {
      refused = e;
    }
    assert.ok(refused instanceof YamlprimsError, c.id);
    assert.equal(refused.kind, "internal", c.id);
    assert.equal(refused.message, c.message, c.id);
    assert.deepEqual(refused.site, ns, c.id);
  }
});
