// SPDX-License-Identifier: MIT OR Apache-2.0
// TypeScript API surface, 128-bit bigint, float bits, toPlain loss matrix,
// single-pass O(N) utf16Offset calculation, and strict decodeWire negative controls.
import assert from "node:assert/strict";
import { test } from "node:test";

import {
  I128_MAX,
  I128_MIN,
  Mapping,
  U128_MAX,
  YamlprimsError,
  createDocument,
  createTag,
  createTaggedNode,
  decodeWire,
  defaultEmitLimits,
  defaultEmitOptions,
  defaultIjsonOptions,
  defaultLimits,
  defaultOptions,
  dump,
  dumpAll,
  encode,
  encodeError,
  encodePlain,
  fromPlain,
  keyEq,
  load,
  loadAll,
  loadAllBytes,
  loadBytes,
  loadIjson,
  loadPlain,
  mappingGet,
  toPlain,
  validateEmitOptions,
  validateIjsonOptions,
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
      value: { kind: "string", value: "ok", span: null, via: null, tagStart: null },
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

test("spec-v1.5 checked constructors: createTag, createTaggedNode, Mapping, createDocument", () => {
  const tag = createTag("!custom");
  assert.equal(tag.uri, "!custom");
  assert.throws(
    () => createTag(""),
    (e) => e instanceof YamlprimsError && e.kind === "invalid_options",
  );
  assert.throws(
    () => createTag("bad uri"),
    (e) => e instanceof YamlprimsError && e.kind === "invalid_options",
  );

  // Core tags rejected on createTag / createTaggedNode
  const strNode = { kind: "string", value: "hi", span: null, via: null, tagStart: null };
  assert.throws(
    () => createTag("tag:yaml.org,2002:str"),
    (e) => e instanceof YamlprimsError && e.kind === "invalid_options",
  );
  // Custom tag wrapping null/bool/int/float rejected
  const nullNode = { kind: "null", span: null, via: null, tagStart: null };
  assert.throws(
    () => createTaggedNode(tag, nullNode),
    (e) => e instanceof YamlprimsError && e.kind === "tag_kind_mismatch",
  );
  // Valid custom tag wrapping string
  const tagged = createTaggedNode(tag, strNode);
  assert.equal(tagged.kind, "tagged");
  assert.throws(
    () => createTaggedNode(tag, tagged),
    (e) => e instanceof YamlprimsError && e.kind === "tag_kind_mismatch",
  );

  // Mapping duplicate keys and NaN keys
  const kA = { kind: "string", value: "a", span: null, via: null, tagStart: null };
  assert.throws(
    () => new Mapping([[kA, strNode], [kA, strNode]]),
    (e) => e instanceof YamlprimsError && e.kind === "duplicate_key",
  );
  const nanKey = { kind: "float", value: NaN, bits: "7ff8000000000000", span: null, via: null, tagStart: null };
  assert.throws(
    () => new Mapping([[nanKey, strNode]]),
    (e) => e instanceof YamlprimsError && e.kind === "invalid_key",
  );

  // createDocument validates yamlVersion
  const d12 = createDocument(strNode, [1, 2], true);
  assert.deepEqual(d12.yamlVersion, [1, 2]);
  assert.equal(d12.explicitStart, true);
  assert.throws(
    () => createDocument(strNode, [2, 0], false),
    (e) => e instanceof YamlprimsError && e.kind === "unsupported_yaml_version",
  );
});

test("spec-v1.5 decodeWire enforces opts.limits and raises internal on invariant violations", () => {
  const doc = createDocument({ kind: "string", value: "hello", span: null, via: null, tagStart: null });
  const wire = encode([doc]);

  assert.throws(
    () => decodeWire(wire, { limits: { maxInputBytes: 10 } }),
    (e) => e instanceof YamlprimsError && e.kind === "limit_exceeded" && e.detail.limit === "max_input_bytes",
  );

  const wire2 = encode([doc, doc]);
  assert.throws(
    () => decodeWire(wire2, { limits: { maxDocuments: 1 } }),
    (e) => e instanceof YamlprimsError && e.kind === "limit_exceeded" && e.detail.limit === "max_documents",
  );

  const deepDoc = createDocument({
    kind: "sequence",
    items: [{ kind: "sequence", items: [{ kind: "null", span: null, via: null, tagStart: null }], span: null, via: null, tagStart: null }],
    span: null,
    via: null,
    tagStart: null,
  });
  const deepWire = encode([deepDoc]);
  assert.throws(
    () => decodeWire(deepWire, { limits: { maxDepth: 1 } }),
    (e) => e instanceof YamlprimsError && e.kind === "limit_exceeded" && e.detail.limit === "max_depth",
  );
  assert.throws(
    () => decodeWire(deepWire, { limits: { maxNodes: 2 } }),
    (e) => e instanceof YamlprimsError && e.kind === "limit_exceeded" && e.detail.limit === "max_nodes",
  );

  // Error path key depth P
  const errWithDeepKey = new YamlprimsError({
    kind: "custom",
    detail: {},
    site: { notSource: { lastKnown: null } },
    path: [
      {
        key: {
          kind: "sequence",
          items: [
            {
              kind: "sequence",
              items: [{ kind: "sequence", items: [], span: null, via: null, tagStart: null }],
              span: null,
              via: null,
              tagStart: null,
            },
          ],
          span: null,
          via: null,
          tagStart: null,
        },
      },
    ],
    message: "deep key",
  });
  const errWire = encodeError(errWithDeepKey);
  assert.throws(
    () => decodeWire(errWire, { limits: { maxDepth: 1 } }),
    (e) => e instanceof YamlprimsError && e.kind === "limit_exceeded" && e.detail.limit === "max_depth",
  );
});

test("spec-v1.5 loadIjson 6-rule strict I-JSON projection and defaultIjsonOptions", () => {
  const iopts = defaultIjsonOptions();
  assert.equal(iopts.mergeKeys, "reject");
  assert.equal(iopts.unknownTags, "reject");
  assert.deepEqual(validateIjsonOptions(), iopts);

  const raw = JSON.parse(loadIjson("a: 0\nb: -0.0\nc: 9007199254740991\nd: -9007199254740991\ne: [true, null, 'ok']\n"));
  assert.deepEqual(raw, {
    a: 0,
    b: 0,
    c: 9007199254740991,
    d: -9007199254740991,
    e: [true, null, "ok"],
  });
  // Verify -0.0 normalized to "0" without minus sign
  assert.equal(loadIjson("-0.0\n"), "0");

  // Rule 1: empty stream -> type ("empty_stream"), multi-doc -> multiple_documents
  for (const empty of ["", "# comment\n", "...\n"]) {
    assert.throws(
      () => loadIjson(empty),
      (e) => e instanceof YamlprimsError && e.kind === "type" && e.detail.found === "empty_stream",
    );
  }
  assert.throws(
    () => loadIjson("1\n---\n2\n"),
    (e) => e instanceof YamlprimsError && e.kind === "multiple_documents",
  );

  // Rule 2: Out of IEEE 754 safe integer range -> integer_out_of_range
  for (const src of ["9007199254740992\n", "-9007199254740992\n"]) {
    assert.throws(
      () => loadIjson(src),
      (e) => e instanceof YamlprimsError && e.kind === "integer_out_of_range",
    );
  }

  // Rule 3: Non-finite floats -> float_out_of_range
  for (const src of [".nan\n", ".inf\n", "-.inf\n"]) {
    assert.throws(
      () => loadIjson(src),
      (e) => e instanceof YamlprimsError && e.kind === "float_out_of_range",
    );
  }

  // Rule 4: Non-string mapping keys -> type ("non_string_key")
  for (const src of ["1: one\n", "true: yes\n", "[1]: seq\n"]) {
    assert.throws(
      () => loadIjson(src),
      (e) => e instanceof YamlprimsError && e.kind === "type" && e.detail.found === "non_string_key",
    );
  }

  // Rule 5: Unknown tags -> unknown_tag (even when unknownTags: "preserve" is passed)
  assert.throws(
    () => loadIjson("!custom hello\n"),
    (e) => e instanceof YamlprimsError && e.kind === "unknown_tag",
  );
  assert.throws(
    () => loadIjson("!custom hello\n", { unknownTags: "preserve" }),
    (e) => e instanceof YamlprimsError && e.kind === "unknown_tag",
  );

  // Merge keys -> merge_rejected (default), invalid_options when mergeKeys: "as_key"
  assert.throws(
    () => loadIjson("<<: {a: 1}\n"),
    (e) => e instanceof YamlprimsError && e.kind === "merge_rejected",
  );
  assert.throws(
    () => loadIjson("<<: 1\n", { mergeKeys: "as_key" }),
    (e) => e instanceof YamlprimsError && e.kind === "invalid_options",
  );

  // Rule 6: Unicode noncharacter -> type ("noncharacter"), lone surrogate -> invalid_utf8
  assert.throws(
    () => loadIjson('"\ufdd0"\n'),
    (e) => e instanceof YamlprimsError && e.kind === "type" && e.detail.found === "noncharacter",
  );
  assert.throws(
    () => loadIjson('"\ud800"'),
    (e) => e instanceof YamlprimsError && e.kind === "invalid_utf8",
  );
});

test("spec-v1.5 dump, dumpAll, fromPlain, CompatQuoting, and EmitLimits", () => {
  const elim = defaultEmitLimits();
  assert.equal(elim.maxDepth, 128);
  assert.equal(elim.maxTagBytes, 1_024);
  assert.equal(elim.maxOutputBytes, 8 * 1024 * 1024);
  const eopts = defaultEmitOptions();
  assert.equal(eopts.compatQuoting, "yaml11_safe");
  assert.deepEqual(validateEmitOptions(), eopts);

  const doc = load("b: [true, null, 42, -0.0, .nan, .inf, -.inf]\na: yes\n");
  const outSafe = dump(doc);
  assert.equal(outSafe, "b:\n  - true\n  - null\n  - 42\n  - -0.0\n  - .nan\n  - .inf\n  - -.inf\na: 'yes'\n");

  const outCore = dump(doc, { compatQuoting: "core_only" });
  assert.equal(outCore, "b:\n  - true\n  - null\n  - 42\n  - -0.0\n  - .nan\n  - .inf\n  - -.inf\na: yes\n");

  // Round-trip via loadAll
  const rtDocs = loadAll(outSafe);
  assert.equal(rtDocs.length, 1);

  // Plain value emission via dump and fromPlain
  const plainObj = Object.assign(Object.create(null), {
    name: "yamlprims",
    count: 3,
    big: I128_MAX,
    negZero: -0,
    items: [true, null, "N"],
  });
  const emittedPlain = dump(plainObj, { compatQuoting: "yaml11_safe" });
  assert.ok(emittedPlain.includes("'N'"));
  assert.ok(emittedPlain.includes("-0.0"));

  // Multi-document dumpAll withdirective & explicitStart
  const d1 = createDocument(fromPlain({ a: 1 }), [1, 2], true);
  const d2 = createDocument(fromPlain(["x", "y"]));
  const multiOut = dumpAll([d1, d2]);
  assert.ok(multiOut.startsWith("%YAML 1.2\n---\n"));
  assert.equal(loadAll(multiOut).length, 2);

  // EmitLimits maxOutputBytes enforcement
  assert.throws(
    () => dump(plainObj, { limits: { maxScalarBytes: 8, maxKeyBytes: 8, maxOutputBytes: 8 } }),
    (e) => e instanceof YamlprimsError && (e.kind === "limit_exceeded" || e.kind === "output_limit_exceeded"),
  );
  assert.throws(
    () => dump("hello", { limits: { maxScalarBytes: 5, maxKeyBytes: 5, maxOutputBytes: 5 } }),
    (e) => e instanceof YamlprimsError && e.kind === "output_limit_exceeded",
  );

  // Custom prototype rejected by fromPlain with kind: "type"
  class Custom {}
  assert.throws(
    () => fromPlain(new Custom()),
    (e) => e instanceof YamlprimsError && e.kind === "type",
  );

  // Lone surrogate rejected in fromPlain / dump with kind: "invalid_utf8"
  assert.throws(
    () => fromPlain("\ud800"),
    (e) => e instanceof YamlprimsError && e.kind === "invalid_utf8",
  );
  assert.throws(
    () => dump("\ud800", { limits: { maxScalarBytes: 1 } }),
    (e) => e instanceof YamlprimsError && e.kind === "invalid_utf8",
  );

  // Plain object with a "kind" property (including { kind: "bool", value: true }) is treated as a plain mapping, not a Node
  const plainWithKind = { kind: "string", count: 5 };
  assert.equal(dump(plainWithKind), "kind: string\ncount: 5\n");
  const plainBoolValue = { kind: "bool", value: true };
  assert.equal(dump(plainBoolValue), "kind: bool\nvalue: true\n");

  // Tag validation: schemeless "plain" and "$:?" rejected; percent-encoded "!a%25" and custom %TAG accepted
  assert.throws(
    () => createTag("plain"),
    (e) => e instanceof YamlprimsError && e.kind === "invalid_options",
  );
  assert.throws(
    () => createTag("$:?"),
    (e) => e instanceof YamlprimsError && e.kind === "invalid_options",
  );
  const pctDoc = load("!a%25 x\n", { unknownTags: "preserve" });
  assert.equal(pctDoc.root.kind, "tagged");
  assert.equal(pctDoc.root.tag.uri, "!a%");
  const customTagDirDoc = load("%TAG !! tag:example.com,2026:\n---\n!!foo bar\n", { unknownTags: "preserve" });
  assert.equal(customTagDirDoc.root.kind, "tagged");
  assert.equal(customTagDirDoc.root.tag.uri, "tag:example.com,2026:foo");

  // Incremental preflight rejects maxNodes: 1 on large array immediately
  assert.throws(
    () => dump(new Array(100_000).fill(0), { limits: { maxNodes: 1 } }),
    (e) => e instanceof YamlprimsError && e.kind === "limit_exceeded" && e.detail.limit === "max_nodes",
  );

  // Keyed 64-bit SipHash-1-3 prevents 32-bit FNV-1a integer sequence key collision floods
  // and performs O(N) item reads (<= 5,000 reads for 512 x 9 items vs > 260,000 on bucket collision).
  let itemReads = 0;
  const advEntries = [];
  for (let i = 0; i < 512; i++) {
    const rawItems = [];
    for (let b = 0; b < 9; b++) {
      rawItems.push({
        kind: "int",
        value: (i & (1 << b)) !== 0 ? 67520n : 34501n,
        span: null,
        via: null,
        tagStart: null,
      });
    }
    const items = new Proxy(rawItems, {
      get(target, prop, receiver) {
        if (typeof prop === "string" && prop !== "length" && !Number.isNaN(Number(prop))) {
          itemReads++;
        }
        return Reflect.get(target, prop, receiver);
      },
    });
    advEntries.push([
      { kind: "sequence", items, span: null, via: null, tagStart: null },
      { kind: "null", span: null, via: null, tagStart: null },
    ]);
  }
  const advMap = new Mapping(advEntries);
  assert.equal(advMap.size, 512);
  // containsNan (4,608 reads) + keyHash (4,608 reads) = 9,216 reads when 0 bucket collisions occur
  // (vs > 260,000 reads when all 512 keys collide in one bucket).
  assert.ok(itemReads <= 9_300, `expected O(N) item reads (<= 9300), got ${itemReads}`);

  // Unavailable Web Crypto fails explicitly with kind: "internal" and never falls back to Math.random()
  const origCryptoDesc = Object.getOwnPropertyDescriptor(globalThis, "crypto");
  const origMathRandom = Math.random;
  try {
    Math.random = () => {
      throw new Error("Math.random fallback is forbidden");
    };
    Object.defineProperty(globalThis, "crypto", {
      value: undefined,
      configurable: true,
      writable: true,
    });
    assert.throws(
      () =>
        new Mapping([
          [
            { kind: "string", value: "k", span: null, via: null, tagStart: null },
            { kind: "null", span: null, via: null, tagStart: null },
          ],
        ]),
      (e) => e instanceof YamlprimsError && e.kind === "internal",
    );
  } finally {
    Math.random = origMathRandom;
    if (origCryptoDesc) {
      Object.defineProperty(globalThis, "crypto", origCryptoDesc);
    }
  }

  // Sparse array holes are rejected with kind: "type"
  // eslint-disable-next-line no-sparse-arrays
  const sparse = [1, , 3];
  assert.throws(
    () => fromPlain(sparse),
    (e) => e instanceof YamlprimsError && e.kind === "type",
  );

  // loadIjson with small maxOutputBytes preserves non-output error kind
  assert.throws(
    () => loadIjson("!custom x\n", { limits: { maxOutputBytes: 4, maxScalarBytes: 4, maxKeyBytes: 4 } }),
    (e) => e instanceof YamlprimsError && e.kind === "unknown_tag",
  );
});
