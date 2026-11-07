// SPDX-License-Identifier: MIT OR Apache-2.0
// TypeScript API surface, 128-bit bigint, float bits, toPlain loss matrix,
// single-pass O(N) utf16Offset calculation, and strict decodeWire negative controls.
import assert from "node:assert/strict";
import { test } from "node:test";

import {
  I128_MAX,
  I128_MIN,
  U128_MAX,
  YamlprimsError,
  decodeWire,
  defaultLimits,
  defaultOptions,
  encode,
  encodeError,
  encodePlain,
  keyEq,
  load,
  loadAll,
  loadAllBytes,
  loadBytes,
  loadPlain,
  mappingGet,
  toPlain,
  validateOptions,
} from "@3leaps/yamlprims";

const enc = (s) => new TextEncoder().encode(s);

test("defaults and option validation", () => {
  const lim = defaultLimits();
  assert.equal(lim.maxInputBytes, 8 * 1024 * 1024);
  assert.equal(lim.maxDepth, 128);
  assert.equal(lim.maxNodes, 250_000);
  assert.equal(lim.maxExpandedNodes, 100_000);
  assert.equal(lim.maxAliasEvents, 10_000);
  assert.equal(lim.maxOwnedBytes, 64 * 1024 * 1024);
  assert.equal(lim.maxScalarBytes, 1 * 1024 * 1024);
  assert.equal(lim.maxKeyBytes, 64 * 1024);
  assert.equal(lim.maxMappingEntries, 100_000);
  assert.equal(lim.maxDocuments, 1_024);
  assert.equal(lim.maxOutputBytes, 64 * 1024 * 1024);
  assert.equal(lim.maxWarnings, 1_000);

  const opts = defaultOptions();
  assert.equal(opts.mergeKeys, "reject");
  assert.equal(opts.unknownTags, "preserve");

  // Zero or >255 maxDepth, or maxScalarBytes > maxOwnedBytes, rejects with invalid_options.
  for (const bad of [
    { limits: { maxDepth: 0 } },
    { limits: { maxDepth: 256 } },
    { limits: { maxOwnedBytes: 100, maxScalarBytes: 200, maxKeyBytes: 50 } },
    { limits: { maxOwnedBytes: 100, maxScalarBytes: 50, maxKeyBytes: 200 } },
    { mergeKeys: "bad" },
    { unknownTags: "bad" },
  ]) {
    assert.throws(
      () => validateOptions(bad),
      (e) => e instanceof YamlprimsError && e.kind === "invalid_options",
    );
  }
});

test("128-bit bigint preservation and range boundaries", () => {
  const doc = load(
    `min: ${I128_MIN.toString()}\nmax_i: ${I128_MAX.toString()}\nmax_u: ${U128_MAX.toString()}\n`,
  );
  assert.equal(doc.root.kind, "mapping");
  assert.equal(mappingGet(doc.root, "min")?.value, I128_MIN);
  assert.equal(mappingGet(doc.root, "max_i")?.value, I128_MAX);
  assert.equal(mappingGet(doc.root, "max_u")?.value, U128_MAX);

  // Round-trip through encode -> decodeWire preserves exact 128-bit bigint values.
  const wire = encode([doc]);
  const decoded = decodeWire(wire);
  assert.equal(decoded.kind, "documents");
  const rtRoot = decoded.documents[0].root;
  assert.equal(mappingGet(rtRoot, "min")?.value, I128_MIN);
  assert.equal(mappingGet(rtRoot, "max_u")?.value, U128_MAX);

  // Out-of-range integers in YAML fail with integer_out_of_range.
  assert.throws(
    () => load(`${(U128_MAX + 1n).toString()}\n`),
    (e) => e instanceof YamlprimsError && e.kind === "integer_out_of_range",
  );
  assert.throws(
    () => load(`${(I128_MIN - 1n).toString()}\n`),
    (e) => e instanceof YamlprimsError && e.kind === "integer_out_of_range",
  );
});

test("float special values, -0.0, and canonical NaN bits", () => {
  const doc = load("neg0: -0.0\npos0: 0.0\nnan: .nan\npinf: .inf\nninf: -.inf\n");
  const neg0 = mappingGet(doc.root, "neg0");
  const pos0 = mappingGet(doc.root, "pos0");
  const nan = mappingGet(doc.root, "nan");
  const pinf = mappingGet(doc.root, "pinf");
  const ninf = mappingGet(doc.root, "ninf");

  assert.equal(neg0?.kind, "float");
  assert.ok(Object.is(neg0.value, -0));
  assert.equal(neg0.bits, "8000000000000000");

  assert.equal(pos0?.kind, "float");
  assert.ok(Object.is(pos0.value, +0));
  assert.equal(pos0.bits, "0000000000000000");

  assert.equal(nan?.kind, "float");
  assert.ok(Number.isNaN(nan.value));
  assert.equal(nan.bits, "7ff8000000000000");

  assert.equal(pinf?.value, Infinity);
  assert.equal(pinf?.bits, "7ff0000000000000");
  assert.equal(ninf?.value, -Infinity);
  assert.equal(ninf?.bits, "fff0000000000000");

  // keyEq treats -0.0 and +0.0 as equal keys, and NaN as never equal to anything.
  assert.equal(keyEq(neg0, pos0), true);
  assert.equal(keyEq(nan, nan), false);
});

test("toPlain null-prototype objects and loss matrix errors", () => {
  const doc = load("a: 1\nb: -0.0\nc: .nan\nd: .inf\ne: [true, null, 'hi']\n");
  const plain = toPlain(doc);
  assert.equal(Object.getPrototypeOf(plain), null);
  assert.equal(plain.a, 1);
  assert.ok(Object.is(plain.b, -0));
  assert.ok(Number.isNaN(plain.c));
  assert.equal(plain.d, Infinity);
  assert.deepEqual(plain.e, [true, null, "hi"]);

  // Integer outside ±(2^53-1) is rejected by toPlain with kind: "type", but encodePlain succeeds!
  const bigIntDoc = load("9007199254740992\n");
  assert.throws(
    () => toPlain(bigIntDoc),
    (e) => e instanceof YamlprimsError && e.kind === "type",
  );
  assert.equal(encodePlain(bigIntDoc), "9007199254740992");
  assert.equal(loadPlain("9007199254740992\n"), "9007199254740992");

  // Non-string mapping key is rejected by both toPlain and encodePlain.
  const intKeyDoc = load("{1: one}\n");
  for (const fn of [toPlain, encodePlain]) {
    assert.throws(
      () => fn(intKeyDoc),
      (e) => e instanceof YamlprimsError && e.kind === "type",
    );
  }

  // Tagged value is rejected by both toPlain and encodePlain.
  const tagDoc = load("!custom hello\n");
  for (const fn of [toPlain, encodePlain]) {
    assert.throws(
      () => fn(tagDoc),
      (e) => e instanceof YamlprimsError && e.kind === "type",
    );
  }

  // Non-finite float is allowed in toPlain (in-memory JS number), but rejected in encodePlain.
  const nanDoc = load(".nan\n");
  assert.ok(Number.isNaN(toPlain(nanDoc)));
  assert.throws(
    () => encodePlain(nanDoc),
    (e) => e instanceof YamlprimsError && e.kind === "type",
  );
});

test("single-pass O(N) utf16Offset across BOM, 2-byte, 3-byte, 4-byte UTF-8, and InvalidUtf8", () => {
  // Leading UTF-8 BOM (3 bytes = 1 UTF-16 code unit) + "é" (2 bytes = 1 UTF-16) + "🙂" (4 bytes = 2 UTF-16)
  const src = "\uFEFFk: \"é🙂\"\nv: 1\n";
  const doc = load(src);
  // Root mapping starts after BOM at byteOffset 3, utf16Offset 1, line 1, column 1.
  assert.deepEqual(doc.root.span?.start, {
    byteOffset: 3,
    utf16Offset: 1,
    line: 1,
    column: 1,
  });
  // Verify that utf16Offset matches JS string slicing at every node location!
  const vNode = mappingGet(doc.root, "v");
  assert.ok(vNode?.span);
  assert.equal(src.slice(vNode.span.start.utf16Offset, vNode.span.end.utf16Offset), "1");

  // InvalidUtf8 after valid multi-byte prefix ("a: é" + 0xff):
  // "a: é" is 5 UTF-8 bytes ("a"=1, ":"=1, " "=1, "é"=2) and 4 UTF-16 code units.
  const badBytes = new Uint8Array([0x61, 0x3a, 0x20, 0xc3, 0xa9, 0xff, 0x0a]);
  assert.throws(
    () => loadBytes(badBytes),
    (e) => {
      assert.ok(e instanceof YamlprimsError);
      assert.equal(e.kind, "invalid_utf8");
      const loc = e.location();
      assert.deepEqual(loc, {
        byteOffset: 5,
        utf16Offset: 4,
        line: 1,
        column: 5,
      });
      return true;
    },
  );
});

test("unpaired surrogates in string inputs, decodeWire, encode, encodeError, encodePlain, and toPlain are rejected", () => {
  for (const fn of [load, loadAll, loadPlain]) {
    assert.throws(() => fn('"\ud800"'), TypeError);
    assert.throws(() => fn('"\udc00"'), TypeError);
  }
  assert.throws(() => loadBytes("a: 1"), TypeError);
  assert.throws(() => loadAllBytes("a: 1"), TypeError);

  // Raw and escaped lone surrogates in decodeWire must be rejected with kind: "internal".
  for (const badStr of ["\ud800", "\udfff", "a\ud800b", "\\ud800", "\\udc00", "\\ud800\\u0041"]) {
    const rawWire = `{"format":"yamlprims-value","version":1,"documents":[{"yaml_version":null,"explicit_start":false,"root":{"kind":"string","value":"${badStr}","span":null,"via":null,"tag_start":null}}]}`;
    assert.throws(
      () => decodeWire(rawWire),
      (e) => e instanceof YamlprimsError && e.kind === "internal",
      `expected decodeWire to reject lone surrogate in: ${badStr}`,
    );
  }

  // Host StringNode, Tag, mapping key, and YamlprimsError fields with lone surrogates must be rejected
  // before replacement or truncation in encode, encodePlain, toPlain, and encodeError.
  for (const lone of ["\ud800", "\udc00", "prefix\ud83dsuffix"]) {
    const badStringNode = {
      kind: "string",
      value: lone,
      span: null,
      via: null,
      tagStart: null,
    };
    const badDoc = { yamlVersion: null, explicitStart: false, root: badStringNode };

    assert.throws(
      () => encode([badDoc]),
      (e) => e instanceof YamlprimsError && e.kind === "internal",
    );
    assert.throws(
      () => encodePlain(badDoc),
      (e) => e instanceof YamlprimsError && e.kind === "internal",
    );
    assert.throws(
      () => toPlain(badDoc),
      (e) => e instanceof YamlprimsError && e.kind === "internal",
    );

    const badTagNode = {
      kind: "tagged",
      tag: { uri: `!${lone}`, handle: "!", suffix: "x" },
      value: { kind: "null", span: null, via: null, tagStart: null },
      span: null,
      via: null,
      tagStart: null,
    };
    assert.throws(
      () => encode([{ yamlVersion: null, explicitStart: false, root: badTagNode }]),
      (e) => e instanceof YamlprimsError && e.kind === "internal",
    );

    // Also test bounded string fields in encodeError (before truncation at 4096 / 1024 bytes).
    const badErr = new YamlprimsError({
      kind: "custom",
      detail: {},
      site: { notSource: { lastKnown: null } },
      message: "a".repeat(5000) + lone,
    });
    assert.throws(
      () => encodeError(badErr),
      (e) => e instanceof YamlprimsError && e.kind === "internal",
    );
  }

  // Positive control: valid supplementary characters (surrogate pairs) round-trip losslessly.
  const paired = "hello 😀 \uD83D\uDE80 world";
  const goodNode = {
    kind: "string",
    value: paired,
    span: null,
    via: null,
    tagStart: null,
  };
  const goodWire = encode([{ yamlVersion: null, explicitStart: false, root: goodNode }]);
  const rtBytes = new TextEncoder().encode(goodWire);
  const rtText = new TextDecoder("utf-8", { fatal: true }).decode(rtBytes);
  const rtDecoded = decodeWire(rtText);
  assert.equal(rtDecoded.kind, "documents");
  assert.equal(rtDecoded.documents[0].root.kind, "string");
  assert.equal(rtDecoded.documents[0].root.value, paired);
  assert.equal(toPlain(rtDecoded.documents[0]), paired);
  assert.equal(JSON.parse(encodePlain(rtDecoded.documents[0])), paired);
});

test("strict decodeWire negative controls", () => {
  const validNullNode = `{"kind":"null","span":null,"via":null,"tag_start":null}`;
  const validDoc = `{"yaml_version":null,"explicit_start":false,"root":${validNullNode}}`;
  const validEnv = `{"format":"yamlprims-value","version":1,"documents":[${validDoc}]}`;
  assert.equal(decodeWire(validEnv).kind, "documents");

  const badEnvelopes = [
    // Duplicate JSON object key
    `{"format":"yamlprims-value","format":"yamlprims-value","version":1,"documents":[${validDoc}]}`,
    // Unknown top-level field
    `{"format":"yamlprims-value","version":1,"documents":[${validDoc}],"extra":1}`,
    // Both documents and error
    `{"format":"yamlprims-value","version":1,"documents":[${validDoc}],"error":null}`,
    // Wrong format or version
    `{"format":"other","version":1,"documents":[${validDoc}]}`,
    `{"format":"yamlprims-value","version":2,"documents":[${validDoc}]}`,
    // yaml_version > 0xffffffff
    `{"format":"yamlprims-value","version":1,"documents":[{"yaml_version":[4294967296,0],"explicit_start":false,"root":${validNullNode}}]}`,
    // Non-canonical integer string (leading zero, +sign, -0)
    `{"format":"yamlprims-value","version":1,"documents":[{"yaml_version":null,"explicit_start":false,"root":{"kind":"int","value":"01","span":null,"via":null,"tag_start":null}}]}`,
    `{"format":"yamlprims-value","version":1,"documents":[{"yaml_version":null,"explicit_start":false,"root":{"kind":"int","value":"-0","span":null,"via":null,"tag_start":null}}]}`,
    // Integer outside 128-bit range
    `{"format":"yamlprims-value","version":1,"documents":[{"yaml_version":null,"explicit_start":false,"root":{"kind":"int","value":"340282366920938463463374607431768211456","span":null,"via":null,"tag_start":null}}]}`,
    // Invalid float bits (uppercase hex, non-canonical NaN bits, wrong value for finite bits)
    `{"format":"yamlprims-value","version":1,"documents":[{"yaml_version":null,"explicit_start":false,"root":{"kind":"float","value":1.0,"bits":"3FF0000000000000","span":null,"via":null,"tag_start":null}}]}`,
    `{"format":"yamlprims-value","version":1,"documents":[{"yaml_version":null,"explicit_start":false,"root":{"kind":"float","value":"nan","bits":"7ff8000000000001","span":null,"via":null,"tag_start":null}}]}`,
    `{"format":"yamlprims-value","version":1,"documents":[{"yaml_version":null,"explicit_start":false,"root":{"kind":"float","value":2.0,"bits":"3ff0000000000000","span":null,"via":null,"tag_start":null}}]}`,
    // Nested Tagged-in-Tagged
    `{"format":"yamlprims-value","version":1,"documents":[{"yaml_version":null,"explicit_start":false,"root":{"kind":"tagged","tag":{"uri":"!a","handle":"!","suffix":"a"},"value":{"kind":"tagged","tag":{"uri":"!b","handle":"!","suffix":"b"},"value":${validNullNode},"span":null,"via":null,"tag_start":null},"span":null,"via":null,"tag_start":null}}]}`,
    // NaN mapping key
    `{"format":"yamlprims-value","version":1,"documents":[{"yaml_version":null,"explicit_start":false,"root":{"kind":"mapping","entries":[[{"kind":"float","value":"nan","bits":"7ff8000000000000","span":null,"via":null,"tag_start":null},${validNullNode}]],"span":null,"via":null,"tag_start":null}}]}`,
    // Duplicate mapping key (+0.0 and -0.0)
    `{"format":"yamlprims-value","version":1,"documents":[{"yaml_version":null,"explicit_start":false,"root":{"kind":"mapping","entries":[[{"kind":"float","value":0.0,"bits":"0000000000000000","span":null,"via":null,"tag_start":null},${validNullNode}],[{"kind":"float","value":-0.0,"bits":"8000000000000000","span":null,"via":null,"tag_start":null},${validNullNode}]],"span":null,"via":null,"tag_start":null}}]}`,
    // Unknown syntax_kind in error envelope
    `{"format":"yamlprims-value","version":1,"error":{"kind":"syntax","detail":{"syntax_kind":"not_a_real_syntax_kind"},"site":{"source":[0,1,1]},"related":[],"path":[],"message":"bad"}}`,
    // Non-null span inside PathKey
    `{"format":"yamlprims-value","version":1,"error":{"kind":"custom","detail":{},"site":{"not_source":{"last_known":null}},"related":[],"path":[{"key":{"kind":"string","value":"k","span":[0,1,1,1,1,2],"via":null,"tag_start":null}}],"message":"bad"}}`,
  ];

  for (const w of badEnvelopes) {
    assert.throws(
      () => decodeWire(w),
      (e) => e instanceof YamlprimsError,
      `expected decodeWire to reject: ${w}`,
    );
  }
});

test("encodeError and encode enforce caller-side bounds and reject fabricated syntaxKind", () => {
  const badSyntax = new YamlprimsError({
    kind: "syntax",
    detail: { syntaxKind: "fabricated_code" },
    site: { source: { byteOffset: 0, line: 1, column: 1 } },
    message: "bad",
  });
  assert.throws(
    () => encodeError(badSyntax),
    (e) => e instanceof YamlprimsError && e.kind === "internal",
  );

  // Truncation of long error messages to <= 4096 UTF-8 bytes ending with '…'
  const longMsg = new YamlprimsError({
    kind: "custom",
    detail: {},
    site: { notSource: { lastKnown: null } },
    message: "é".repeat(3000),
  });
  const encodedErr = encodeError(longMsg);
  const decodedErr = decodeWire(encodedErr);
  assert.equal(decodedErr.kind, "error");
  assert.ok(enc(decodedErr.error.message).byteLength <= 4096);
  assert.ok(decodedErr.error.message.endsWith("…"));
});

test("public package root surface excludes internal _wasmTestHarness hook", async () => {
  const pkg = await import("@3leaps/yamlprims");
  assert.equal("_wasmTestHarness" in pkg, false);
  assert.equal(Object.keys(pkg).includes("_wasmTestHarness"), false);
  assert.equal(Object.getOwnPropertyNames(pkg).includes("_wasmTestHarness"), false);
});
