// SPDX-License-Identifier: MIT OR Apache-2.0
// WASM boundary negative controls: zero imports, pre-copy reservation bounds,
// reserve-failure invalidation, post-failure/post-load reuse rejection,
// status-2 non-recursive OutputLimitExceeded fallback, non-reentrancy, and
// trap/recreate recovery without calling release() on a trapped instance.
import assert from "node:assert/strict";
import { test } from "node:test";

import {
  MAX_OPTIONS_BYTES,
  MAX_WASM_INPUT_CEILING,
  MAX_WASM_MEMORY_BYTES,
  MAX_WASM_MEMORY_PAGES,
  WASM_HIGH_WATER_RECYCLE_BYTES,
  YamlprimsError,
  load,
  loadAll,
  loadPlain,
  toPlain,
} from "@3leaps/yamlprims";

const harness = load[Symbol.for("yamlprims.internal.wasmTestHarness")]();
const encoder = new TextEncoder();

test("WASM module has zero imports, reports ABI version 1, and enforces hard 128 MiB (2048-page) linear-memory ceiling", () => {
  const imports = WebAssembly.Module.imports(harness.module);
  assert.deepEqual(imports, []);
  const raw = harness.createRawInstance();
  assert.equal(raw.abi_version() >>> 0, 1);

  const currentPages = raw.memory.buffer.byteLength / 65536;
  assert.ok(currentPages > 0 && currentPages < MAX_WASM_MEMORY_PAGES);

  // Attempting to grow even 1 page past 2048 pages (128 MiB) must throw RangeError.
  assert.throws(
    () => raw.memory.grow(MAX_WASM_MEMORY_PAGES + 1 - currentPages),
    RangeError,
  );

  // Growing up to exactly 2048 pages (134,217,728 bytes) succeeds, and growing 1 more page fails.
  raw.memory.grow(MAX_WASM_MEMORY_PAGES - currentPages);
  assert.equal(raw.memory.buffer.byteLength, MAX_WASM_MEMORY_BYTES);
  assert.throws(() => raw.memory.grow(1), RangeError);
});

test("pre-copy options_reserve and input_reserve bounds", () => {
  const raw = harness.createRawInstance();

  // options_reserve(0) returns a valid non-null sentinel pointer (1).
  assert.equal(raw.options_reserve(0) >>> 0, 1);
  // options_reserve within MAX_OPTIONS_BYTES returns a non-zero pointer.
  assert.ok((raw.options_reserve(MAX_OPTIONS_BYTES) >>> 0) > 0);
  // options_reserve > MAX_OPTIONS_BYTES returns 0 before allocating and invalidates state.
  assert.equal(raw.options_reserve(MAX_OPTIONS_BYTES + 1) >>> 0, 0);
  // input_reserve requires an active options reservation.
  assert.equal(raw.input_reserve(0, 16) >>> 0, 0);

  // Establish a valid options reservation, then test input_reserve bounds.
  assert.equal(raw.options_reserve(0) >>> 0, 1);
  assert.equal(raw.input_reserve(0, 16) >>> 0, 1);
  assert.ok((raw.input_reserve(16, 16) >>> 0) > 0);
  // input_reserve > max_input_bytes returns 0 before allocating and invalidates state.
  assert.equal(raw.input_reserve(17, 16) >>> 0, 0);
  // input_reserve > MAX_WASM_INPUT_CEILING returns 0 even if max_input_bytes is 0xffffffff.
  assert.equal(raw.options_reserve(0) >>> 0, 1);
  assert.equal(raw.input_reserve(MAX_WASM_INPUT_CEILING + 1, 0xffffffff) >>> 0, 0);
});

test("reserve-failure invalidation and post-failure / post-load reuse rejection", () => {
  const raw = harness.createRawInstance();
  const yamlBytes = encoder.encode("k: 42\n");

  // 1. Calling load_wire or load_plain before any reservation is rejected with status 2.
  assert.equal(raw.load_wire(0, 0, 1) >>> 0, 2);
  assert.equal(raw.load_plain(0, 0) >>> 0, 2);

  // 2. Establish a valid reservation and produce output.
  assert.equal(raw.options_reserve(0) >>> 0, 1);
  const ptr1 = raw.input_reserve(yamlBytes.byteLength, 1024) >>> 0;
  assert.ok(ptr1 > 0);
  new Uint8Array(raw.memory.buffer, ptr1, yamlBytes.byteLength).set(yamlBytes);
  assert.equal(raw.load_wire(yamlBytes.byteLength, 0, 1) >>> 0, 0);
  assert.ok((raw.output_len() >>> 0) > 0);

  // 3. Post-load reuse without a fresh reservation is rejected with status 2.
  assert.equal(raw.load_wire(yamlBytes.byteLength, 0, 1) >>> 0, 2);

  // 4. Failed options_reserve invalidates an earlier valid reservation and clears output.
  assert.equal(raw.options_reserve(0) >>> 0, 1);
  const ptr2 = raw.input_reserve(yamlBytes.byteLength, 1024) >>> 0;
  assert.ok(ptr2 > 0);
  new Uint8Array(raw.memory.buffer, ptr2, yamlBytes.byteLength).set(yamlBytes);
  assert.equal(raw.options_reserve(MAX_OPTIONS_BYTES + 1) >>> 0, 0);
  assert.equal(raw.output_len() >>> 0, 0);
  assert.equal(raw.load_wire(yamlBytes.byteLength, 0, 1) >>> 0, 2);

  // 5. Failed input_reserve invalidates the active options reservation and clears output.
  assert.equal(raw.options_reserve(0) >>> 0, 1);
  assert.equal(raw.input_reserve(100, 10) >>> 0, 0);
  assert.equal(raw.output_len() >>> 0, 0);
  assert.equal(raw.load_wire(0, 0, 1) >>> 0, 2);

  raw.release();
  assert.equal(raw.output_len() >>> 0, 0);
});

test("host rejects input exceeding maxInputBytes with exact limit_exceeded diagnostics", () => {
  const big = new Uint8Array(1024).fill(0x61);
  for (const fn of [load, loadAll, loadPlain]) {
    assert.throws(
      () => fn(big, { limits: { maxInputBytes: 4 } }),
      (e) =>
        e instanceof YamlprimsError &&
        e.kind === "limit_exceeded" &&
        e.detail.limit === "max_input_bytes" &&
        e.detail.configured === 4 &&
        e.message === "max_input_bytes (4) exceeded" &&
        e.location()?.byteOffset === 0,
    );
  }
});

test("status-2 non-recursive OutputLimitExceeded fallback when maxOutputBytes cannot hold wire envelope", () => {
  for (const fn of [load, loadAll, loadPlain]) {
    assert.throws(
      () => fn("a: [1, 2, 3, 4]\n", { limits: { maxOutputBytes: 8 } }),
      (e) =>
        e instanceof YamlprimsError &&
        e.kind === "output_limit_exceeded" &&
        e.detail.configured === 8 &&
        e.location() === null,
    );
  }
});

test("single-owner non-reentrant execution guard rejects concurrent/reentrant WASM calls", () => {
  harness.runWithBusyInstance(() => {
    assert.throws(
      () => load("a: 1\n"),
      (e) => e instanceof YamlprimsError && e.kind === "internal" && /reentrant/.test(e.message),
    );
  });
  // Subsequent normal call succeeds after the busy guard exits.
  assert.deepEqual({ ...toPlain(load("a: 1\n")) }, { a: 1 });
});

test("trap recovery skips release() on the trapped instance and recreates WebAssembly.Instance on the next call", () => {
  // Ensure at least one instance exists.
  assert.deepEqual({ ...toPlain(load("ok: true\n")) }, { ok: true });
  const before = harness.stats();

  assert.throws(
    () => harness.simulateTrapOnLoad("trap: 1\n"),
    (e) => e instanceof YamlprimsError && e.kind === "internal" && /trap/i.test(e.message),
  );

  const afterTrap = harness.stats();
  // release() MUST NOT have been called in finally on the trapped instance.
  assert.equal(afterTrap.releaseCallCount, before.releaseCallCount);
  assert.equal(afterTrap.instanceTrapped, true);

  // Next call discards the trapped instance, instantiates a fresh one, and succeeds.
  const doc = load("recovered: 123\n");
  assert.deepEqual({ ...toPlain(doc) }, { recovered: 123 });

  const afterRecovery = harness.stats();
  assert.equal(afterRecovery.instanceTrapped, false);
  assert.equal(afterRecovery.recreateCount, before.recreateCount + 1);
  assert.equal(afterRecovery.releaseCallCount, before.releaseCallCount + 1);
});

test("real WASM linear-memory exhaustion on input_reserve and in-flight load_wire OOM trap with clean recovery", () => {
  // 1. Real allocator exhaustion inside input_reserve when linear memory is at the 2048-page ceiling.
  assert.deepEqual({ ...toPlain(load("warmup: 1\n")) }, { warmup: 1 });
  const mem1 = harness.getActiveMemory();
  const curPages1 = mem1.buffer.byteLength / 65536;
  mem1.grow(MAX_WASM_MEMORY_PAGES - curPages1);
  assert.equal(mem1.buffer.byteLength, MAX_WASM_MEMORY_BYTES);

  // 512 KiB input is well within maxInputBytes (8 MiB) and MAX_WASM_INPUT_CEILING (64 MiB),
  // so it passes numeric bounds and fails inside s.input.try_reserve_exact(len) at the 128 MiB ceiling.
  const mediumYaml = "k: " + "a".repeat(512 * 1024) + "\n";
  assert.throws(
    () => load(mediumYaml),
    (e) =>
      e instanceof YamlprimsError &&
      e.kind === "internal" &&
      /linear memory ceiling exceeded/.test(e.message),
  );

  // Subsequent call automatically recreates a fresh instance and succeeds.
  assert.deepEqual({ ...toPlain(load("after_reserve_oom: 42\n")) }, { after_reserve_oom: 42 });

  // 2. Real in-flight WASM OOM trap during load_wire when input_reserve fits in existing free heap
  // but parser/output expansion exceeds the remaining heap at the 2048-page ceiling.
  const beforeOom = harness.stats();
  const mem2 = harness.getActiveMemory();
  const curPages2 = mem2.buffer.byteLength / 65536;
  mem2.grow(MAX_WASM_MEMORY_PAGES - curPages2);
  assert.equal(mem2.buffer.byteLength, MAX_WASM_MEMORY_BYTES);

  const smallExpandingYaml = [
    'a0: &a0 "' + "x".repeat(64) + '"',
    "a1: &a1 [*a0,*a0,*a0,*a0,*a0,*a0,*a0,*a0]",
    "a2: &a2 [*a1,*a1,*a1,*a1,*a1,*a1,*a1,*a1]",
    "a3: [*a2,*a2,*a2,*a2,*a2,*a2,*a2,*a2]",
    "",
  ].join("\n");

  assert.throws(
    () => load(smallExpandingYaml),
    (e) => e instanceof YamlprimsError && e.kind === "internal" && /WASM runtime trap/i.test(e.message),
  );

  const afterRealOom = harness.stats();
  // Real OOM trap in load_wire must NOT call release() on the trapped instance.
  assert.equal(afterRealOom.instanceTrapped, true);
  assert.equal(afterRealOom.releaseCallCount, beforeOom.releaseCallCount);

  // Re-running the exact same expanding YAML now succeeds on the freshly recreated instance!
  const recoveredDoc = load(smallExpandingYaml);
  assert.equal(recoveredDoc.root.kind, "mapping");
  const afterRealRecovery = harness.stats();
  assert.equal(afterRealRecovery.instanceTrapped, false);
  assert.equal(afterRealRecovery.recreateCount, beforeOom.recreateCount + 1);
});

test("active WebAssembly.Instance is recycled after release() when linear memory exceeds WASM_HIGH_WATER_RECYCLE_BYTES", () => {
  assert.deepEqual({ ...toPlain(load("init: 1\n")) }, { init: 1 });
  const initialMem = harness.getActiveMemory();
  assert.ok(initialMem.buffer.byteLength < WASM_HIGH_WATER_RECYCLE_BYTES);

  // Grow active linear memory just above the 16 MiB high-water threshold (257 pages = 16,842,752 bytes).
  const curPages = initialMem.buffer.byteLength / 65536;
  const targetPages = WASM_HIGH_WATER_RECYCLE_BYTES / 65536 + 1;
  initialMem.grow(targetPages - curPages);
  assert.ok(initialMem.buffer.byteLength > WASM_HIGH_WATER_RECYCLE_BYTES);

  const beforeLoad = harness.stats();
  // Normal load completes, calls release(), and recycles the high-water instance in finally.
  assert.deepEqual({ ...toPlain(load("after_high_water: 2\n")) }, { after_high_water: 2 });
  const afterLoad = harness.stats();
  assert.equal(afterLoad.releaseCallCount, beforeLoad.releaseCallCount + 1);
  assert.equal(afterLoad.instanceTrapped, false);

  // Next call instantiates a fresh WebAssembly.Instance with small linear memory (< 16 MiB).
  assert.deepEqual({ ...toPlain(load("fresh_instance: 3\n")) }, { fresh_instance: 3 });
  const afterFresh = harness.stats();
  assert.equal(afterFresh.recreateCount, beforeLoad.recreateCount + 1);
  assert.ok(harness.getActiveMemory().buffer.byteLength < WASM_HIGH_WATER_RECYCLE_BYTES);
});
