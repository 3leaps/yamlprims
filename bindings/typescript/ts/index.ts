// SPDX-License-Identifier: MIT OR Apache-2.0
import { WASM_BASE64 } from "./wasm.js";

function wasmBytes(): Uint8Array {
  const g = globalThis as { atob?: (s: string) => string };
  if (typeof g.atob !== "function") {
    throw new Error("yamlprims: atob is unavailable");
  }
  const raw = g.atob(WASM_BASE64);
  const out = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i++) {
    out[i] = raw.charCodeAt(i);
  }
  return out;
}

export const ABI_VERSION = 1;
export const MAX_OPTIONS_BYTES = 64 * 1024;
export const MAX_WASM_INPUT_CEILING = 64 * 1024 * 1024;
export const MAX_WASM_MEMORY_BYTES = 128 * 1024 * 1024;
export const MAX_WASM_MEMORY_PAGES = 2048;
export const MAX_SAFE_METADATA_INT = 9_007_199_254_740_991; // 2^53 - 1
export const I128_MIN = -170141183460469231731687303715884105728n;
export const I128_MAX = 170141183460469231731687303715884105727n;
export const U128_MAX = 340282366920938463463374607431768211455n;
const CANONICAL_NAN_BITS = "7ff8000000000000";
const CANONICAL_NAN_BIGINT = 0x7ff8000000000000n;
const NODE_OVERHEAD = 64n;
const ENTRY_OVERHEAD = 24n;
const MAX_MESSAGE_BYTES = 4096;
const MAX_FIELD_BYTES = 1024;

export type MergeKeys = "reject" | "apply" | "as_key" | "as-key";
export type UnknownTags = "preserve" | "reject";

export interface Limits {
  maxInputBytes: number;
  maxDepth: number;
  maxNodes: number;
  maxExpandedNodes: number;
  maxAliasEvents: number;
  maxOwnedBytes: number;
  maxScalarBytes: number;
  maxKeyBytes: number;
  maxMappingEntries: number;
  maxDocuments: number;
  maxOutputBytes: number;
  maxWarnings: number;
}

export interface LimitsInput {
  maxInputBytes?: number;
  maxDepth?: number;
  maxNodes?: number;
  maxExpandedNodes?: number;
  maxAliasEvents?: number;
  maxOwnedBytes?: number;
  maxScalarBytes?: number;
  maxKeyBytes?: number;
  maxMappingEntries?: number;
  maxDocuments?: number;
  maxOutputBytes?: number;
  maxWarnings?: number;
  // Also accept snake_case keys from conformance vectors/options JSON.
  max_input_bytes?: number;
  max_depth?: number;
  max_nodes?: number;
  max_expanded_nodes?: number;
  max_alias_events?: number;
  max_owned_bytes?: number;
  max_scalar_bytes?: number;
  max_key_bytes?: number;
  max_mapping_entries?: number;
  max_documents?: number;
  max_output_bytes?: number;
  max_warnings?: number;
}

export interface Options {
  mergeKeys: MergeKeys;
  unknownTags: UnknownTags;
  limits: Limits;
}

export interface OptionsInput {
  mergeKeys?: MergeKeys;
  unknownTags?: UnknownTags;
  limits?: LimitsInput;
  merge_keys?: MergeKeys;
  unknown_tags?: UnknownTags;
}

export interface Location {
  byteOffset: number;
  utf16Offset: number;
  line: number;
  column: number;
}

export interface Span {
  start: Location;
  end: Location;
}

export type Via =
  | { kind: "alias"; alias: Span; useSite: Span }
  | { kind: "merge"; merge: Span; directive: Span };

export interface Tag {
  uri: string;
  handle: string;
  suffix: string;
}

export interface NodeMetadata {
  span: Span | null;
  via: Via | null;
  tagStart: Location | null;
}

export interface NullNode extends NodeMetadata {
  kind: "null";
  value?: null;
}

export interface BoolNode extends NodeMetadata {
  kind: "bool";
  value: boolean;
}

export interface IntNode extends NodeMetadata {
  kind: "int";
  value: bigint;
}

export interface FloatNode extends NodeMetadata {
  kind: "float";
  value: number;
  bits: string;
}

export interface StringNode extends NodeMetadata {
  kind: "string";
  value: string;
}

export interface SequenceNode extends NodeMetadata {
  kind: "sequence";
  items: Node[];
}

export interface MappingNode extends NodeMetadata {
  kind: "mapping";
  entries: Array<[Node, Node]>;
}

export interface TaggedNode extends NodeMetadata {
  kind: "tagged";
  tag: Tag;
  value: Node;
}

export type Node =
  | NullNode
  | BoolNode
  | IntNode
  | FloatNode
  | StringNode
  | SequenceNode
  | MappingNode
  | TaggedNode;

export interface Document {
  yamlVersion: [number, number] | null;
  explicitStart: boolean;
  root: Node;
}

export type ErrorKind =
  | "invalid_utf8"
  | "syntax"
  | "unsupported_yaml_version"
  | "limit_exceeded"
  | "invalid_options"
  | "duplicate_key"
  | "invalid_key"
  | "unknown_anchor"
  | "recursive_alias"
  | "merge_rejected"
  | "merge_source"
  | "unknown_tag"
  | "tag_kind_mismatch"
  | "tag_value_mismatch"
  | "integer_out_of_range"
  | "float_out_of_range"
  | "multiple_documents"
  | "type"
  | "missing_field"
  | "unknown_field"
  | "custom"
  | "output_limit_exceeded"
  | "internal";

export type LimitKind =
  | "max_input_bytes"
  | "max_depth"
  | "max_nodes"
  | "max_expanded_nodes"
  | "max_alias_events"
  | "max_owned_bytes"
  | "max_scalar_bytes"
  | "max_key_bytes"
  | "max_mapping_entries"
  | "max_documents";

export type RelatedRole =
  | "first_definition"
  | "alias_use"
  | "merge_directive"
  | "anchor_definition";

export interface Related {
  role: RelatedRole;
  span: Span;
}

export type PathSegment = { key: Node } | { index: number };

export type ErrorSite =
  | { source: Location; notSource?: undefined }
  | { notSource: { lastKnown: Location | null }; source?: undefined };

export type ErrorDetail =
  | { invalid_len: number | null; invalidLen?: number | null }
  | { invalidLen: number | null; invalid_len?: number | null }
  | { syntax_kind: string; syntaxKind?: string }
  | { syntaxKind: string; syntax_kind?: string }
  | { limit: LimitKind; configured: number }
  | { expected: string; found: string }
  | { field: string }
  | { configured: number }
  | Record<string, never>;

export const SYNTAX_KINDS: ReadonlySet<string> = new Set([
  "unexpected_eof_flow_sequence",
  "unexpected_eof_flow_mapping",
  "unexpected_eof_implicit_flow_mapping",
  "unexpected_eof_block_sequence",
  "unexpected_eof_block_mapping",
  "unexpected_eof",
  "expected_stream_start",
  "duplicate_version_directive",
  "duplicate_tag_directive",
  "expected_document_start",
  "missing_document_end_before_directive",
  "anchor_count_overflow",
  "expected_node_content",
  "expected_block_mapping_key",
  "expected_flow_mapping_separator",
  "expected_flow_sequence_separator",
  "expected_block_sequence_entry",
  "undeclared_tag_handle",
  "expected_tag_bang",
  "expected_tag_directive_bang",
  "invalid_global_tag_character",
  "simple_key_expected",
  "invalid_simple_key",
  "invalid_document_end",
  "invalid_indentation",
  "bom_inside_document",
  "unexpected_character",
  "tab_not_allowed",
  "tab_in_block_indentation",
  "comment_intercepted_scalar",
  "expected_whitespace",
  "comment_not_separated",
  "invalid_directive_terminator",
  "missing_yaml_version_separator",
  "missing_directive_name",
  "invalid_directive_name",
  "directive_byte_limit_exceeded",
  "too_many_reserved_directive_params",
  "yaml_version_too_long",
  "missing_yaml_version",
  "invalid_tag_directive_terminator",
  "invalid_tag_terminator",
  "missing_tag_uri",
  "unclosed_verbatim_tag",
  "invalid_tag_escape",
  "invalid_tag_utf8_leading_byte",
  "invalid_tag_utf8_trailing_byte",
  "invalid_tag_utf8",
  "missing_anchor_or_alias_name",
  "misplaced_flow_collection_end",
  "mismatched_flow_collection_end",
  "unclosed_flow_collection",
  "block_entry_in_flow_collection",
  "block_sequence_entry_not_allowed",
  "invalid_block_entry_whitespace",
  "zero_block_scalar_indent",
  "invalid_block_scalar_header",
  "tab_at_block_scalar_start",
  "invalid_block_scalar_indent",
  "document_indicator_in_quoted_scalar",
  "unclosed_quoted_scalar",
  "tab_in_indentation",
  "invalid_quoted_scalar_indent",
  "invalid_trailing_single_quoted_scalar",
  "invalid_trailing_double_quoted_scalar",
  "unknown_quoted_scalar_escape",
  "invalid_quoted_scalar_hex_escape",
  "invalid_low_surrogate_hex_escape",
  "invalid_low_surrogate",
  "missing_low_surrogate",
  "unpaired_low_surrogate",
  "invalid_unicode_escape",
  "invalid_flow_scalar_indent",
  "plain_scalar_starts_with_dash_flow_indicator",
  "tab_in_plain_scalar",
  "unexpected_end_of_plain_scalar",
  "mapping_key_not_allowed",
  "flow_mapping_value_adjacent_collection",
  "invalid_mapping_value_whitespace",
  "invalid_colon_placement",
  "mapping_value_not_allowed",
]);

const LIMIT_KINDS: ReadonlySet<string> = new Set([
  "max_input_bytes",
  "max_depth",
  "max_nodes",
  "max_expanded_nodes",
  "max_alias_events",
  "max_owned_bytes",
  "max_scalar_bytes",
  "max_key_bytes",
  "max_mapping_entries",
  "max_documents",
]);

const EMPTY_DETAIL_ERROR_KINDS: ReadonlySet<string> = new Set([
  "unsupported_yaml_version",
  "invalid_options",
  "duplicate_key",
  "invalid_key",
  "unknown_anchor",
  "recursive_alias",
  "merge_rejected",
  "merge_source",
  "unknown_tag",
  "tag_kind_mismatch",
  "tag_value_mismatch",
  "integer_out_of_range",
  "float_out_of_range",
  "multiple_documents",
  "custom",
  "internal",
]);

export class YamlprimsError extends Error {
  readonly kind: ErrorKind;
  readonly detail: ErrorDetail;
  readonly site: ErrorSite;
  readonly byteOffset: number | null;
  readonly utf16Offset: number | null;
  readonly line: number | null;
  readonly column: number | null;
  readonly related: Related[];
  readonly path: PathSegment[];
  readonly rawMessage: string;

  constructor(init: {
    kind: ErrorKind;
    detail: ErrorDetail;
    site: ErrorSite;
    related?: Related[];
    path?: PathSegment[];
    message: string;
  }) {
    const src = init.site.source ?? null;
    super(init.message);
    this.name = "YamlprimsError";
    this.kind = init.kind;
    const rawDetail = init.detail as Record<string, unknown>;
    if (init.kind === "syntax" && typeof rawDetail["syntaxKind"] === "string" && !("syntax_kind" in rawDetail)) {
      this.detail = { syntax_kind: rawDetail["syntaxKind"] };
    } else if (init.kind === "invalid_utf8" && "invalidLen" in rawDetail && !("invalid_len" in rawDetail)) {
      this.detail = { invalid_len: (rawDetail["invalidLen"] as number | null) ?? null };
    } else {
      this.detail = init.detail;
    }
    this.site = init.site;
    this.byteOffset = src !== null ? src.byteOffset : null;
    this.utf16Offset = src !== null ? src.utf16Offset : null;
    this.line = src !== null ? src.line : null;
    this.column = src !== null ? src.column : null;
    this.related = init.related ?? [];
    this.path = init.path ?? [];
    this.rawMessage = init.message;
  }

  location(): Location | null {
    return this.site.source ?? null;
  }
}

export function defaultLimits(): Limits {
  return {
    maxInputBytes: 8 * 1024 * 1024,
    maxDepth: 128,
    maxNodes: 250_000,
    maxExpandedNodes: 100_000,
    maxAliasEvents: 10_000,
    maxOwnedBytes: 64 * 1024 * 1024,
    maxScalarBytes: 1 * 1024 * 1024,
    maxKeyBytes: 64 * 1024,
    maxMappingEntries: 100_000,
    maxDocuments: 1_024,
    maxOutputBytes: 64 * 1024 * 1024,
    maxWarnings: 1_000,
  };
}

export function defaultOptions(): Options {
  return {
    mergeKeys: "reject",
    unknownTags: "preserve",
    limits: defaultLimits(),
  };
}

function notSourceError(
  kind: ErrorKind,
  detail: ErrorDetail,
  message: string,
  lastKnown: Location | null = null,
): YamlprimsError {
  return new YamlprimsError({
    kind,
    detail,
    site: { notSource: { lastKnown } },
    message,
  });
}

function invalidOptionsError(message: string): YamlprimsError {
  return notSourceError("invalid_options", {}, message);
}

function pickLimit(
  input: LimitsInput | undefined,
  camel: keyof Limits,
  snake: keyof LimitsInput,
  fallback: number,
): number {
  if (!input) return fallback;
  const c = input[camel];
  const s = input[snake];
  if (c !== undefined && s !== undefined && c !== s) {
    throw invalidOptionsError(`conflicting limit values for ${String(camel)}`);
  }
  const chosen = c !== undefined ? c : s !== undefined ? s : fallback;
  if (typeof chosen !== "number" || !Number.isInteger(chosen) || !Number.isSafeInteger(chosen)) {
    throw invalidOptionsError(`limit ${String(camel)} must be a safe integer`);
  }
  return chosen;
}

const KNOWN_OPTION_KEYS: ReadonlySet<string> = new Set([
  "mergeKeys",
  "unknownTags",
  "limits",
  "merge_keys",
  "unknown_tags",
]);

const KNOWN_LIMIT_KEYS: ReadonlySet<string> = new Set([
  "maxInputBytes",
  "maxDepth",
  "maxNodes",
  "maxExpandedNodes",
  "maxAliasEvents",
  "maxOwnedBytes",
  "maxScalarBytes",
  "maxKeyBytes",
  "maxMappingEntries",
  "maxDocuments",
  "maxOutputBytes",
  "maxWarnings",
  "max_input_bytes",
  "max_depth",
  "max_nodes",
  "max_expanded_nodes",
  "max_alias_events",
  "max_owned_bytes",
  "max_scalar_bytes",
  "max_key_bytes",
  "max_mapping_entries",
  "max_documents",
  "max_output_bytes",
  "max_warnings",
]);

export function validateOptions(options?: OptionsInput): Options {
  if (options === undefined) {
    return defaultOptions();
  }
  if (typeof options !== "object" || options === null || Array.isArray(options)) {
    throw invalidOptionsError("options must be an object");
  }
  for (const k of Object.keys(options)) {
    if (!KNOWN_OPTION_KEYS.has(k)) {
      throw invalidOptionsError(`unknown option key: ${k}`);
    }
  }
  if (
    options.mergeKeys !== undefined &&
    options.merge_keys !== undefined &&
    options.mergeKeys !== options.merge_keys
  ) {
    throw invalidOptionsError("conflicting mergeKeys options");
  }
  if (
    options.unknownTags !== undefined &&
    options.unknown_tags !== undefined &&
    options.unknownTags !== options.unknown_tags
  ) {
    throw invalidOptionsError("conflicting unknownTags options");
  }
  const rawMergeKeys = options.mergeKeys ?? options.merge_keys ?? "reject";
  const mergeKeys: MergeKeys = rawMergeKeys === "as-key" ? "as_key" : rawMergeKeys;
  if (mergeKeys !== "reject" && mergeKeys !== "apply" && mergeKeys !== "as_key") {
    throw invalidOptionsError("invalid mergeKeys option");
  }
  const unknownTags = options.unknownTags ?? options.unknown_tags ?? "preserve";
  if (unknownTags !== "preserve" && unknownTags !== "reject") {
    throw invalidOptionsError("invalid unknownTags option");
  }
  const limIn = options.limits;
  if (limIn !== undefined) {
    if (typeof limIn !== "object" || limIn === null || Array.isArray(limIn)) {
      throw invalidOptionsError("limits must be an object");
    }
    for (const k of Object.keys(limIn)) {
      if (!KNOWN_LIMIT_KEYS.has(k)) {
        throw invalidOptionsError(`unknown limit key: ${k}`);
      }
    }
  }
  const def = defaultLimits();
  const limits: Limits = {
    maxInputBytes: pickLimit(limIn, "maxInputBytes", "max_input_bytes", def.maxInputBytes),
    maxDepth: pickLimit(limIn, "maxDepth", "max_depth", def.maxDepth),
    maxNodes: pickLimit(limIn, "maxNodes", "max_nodes", def.maxNodes),
    maxExpandedNodes: pickLimit(limIn, "maxExpandedNodes", "max_expanded_nodes", def.maxExpandedNodes),
    maxAliasEvents: pickLimit(limIn, "maxAliasEvents", "max_alias_events", def.maxAliasEvents),
    maxOwnedBytes: pickLimit(limIn, "maxOwnedBytes", "max_owned_bytes", def.maxOwnedBytes),
    maxScalarBytes: pickLimit(limIn, "maxScalarBytes", "max_scalar_bytes", def.maxScalarBytes),
    maxKeyBytes: pickLimit(limIn, "maxKeyBytes", "max_key_bytes", def.maxKeyBytes),
    maxMappingEntries: pickLimit(limIn, "maxMappingEntries", "max_mapping_entries", def.maxMappingEntries),
    maxDocuments: pickLimit(limIn, "maxDocuments", "max_documents", def.maxDocuments),
    maxOutputBytes: pickLimit(limIn, "maxOutputBytes", "max_output_bytes", def.maxOutputBytes),
    maxWarnings: pickLimit(limIn, "maxWarnings", "max_warnings", def.maxWarnings),
  };

  const all = [
    limits.maxInputBytes,
    limits.maxDepth,
    limits.maxNodes,
    limits.maxExpandedNodes,
    limits.maxAliasEvents,
    limits.maxOwnedBytes,
    limits.maxScalarBytes,
    limits.maxKeyBytes,
    limits.maxMappingEntries,
    limits.maxDocuments,
    limits.maxOutputBytes,
    limits.maxWarnings,
  ];
  if (all.some((v) => v < 1)) {
    throw invalidOptionsError("every limit must be at least 1");
  }
  if (limits.maxDepth > 255) {
    throw invalidOptionsError("max_depth must be at most 255");
  }
  if (limits.maxScalarBytes > limits.maxOwnedBytes || limits.maxKeyBytes > limits.maxOwnedBytes) {
    throw invalidOptionsError("max_scalar_bytes and max_key_bytes must not exceed max_owned_bytes");
  }

  return { mergeKeys, unknownTags, limits };
}

function isDefaultOptions(opts: Options): boolean {
  const def = defaultOptions();
  return (
    opts.mergeKeys === def.mergeKeys &&
    opts.unknownTags === def.unknownTags &&
    opts.limits.maxInputBytes === def.limits.maxInputBytes &&
    opts.limits.maxDepth === def.limits.maxDepth &&
    opts.limits.maxNodes === def.limits.maxNodes &&
    opts.limits.maxExpandedNodes === def.limits.maxExpandedNodes &&
    opts.limits.maxAliasEvents === def.limits.maxAliasEvents &&
    opts.limits.maxOwnedBytes === def.limits.maxOwnedBytes &&
    opts.limits.maxScalarBytes === def.limits.maxScalarBytes &&
    opts.limits.maxKeyBytes === def.limits.maxKeyBytes &&
    opts.limits.maxMappingEntries === def.limits.maxMappingEntries &&
    opts.limits.maxDocuments === def.limits.maxDocuments &&
    opts.limits.maxOutputBytes === def.limits.maxOutputBytes &&
    opts.limits.maxWarnings === def.limits.maxWarnings
  );
}

const encoder = new TextEncoder();
const decoder = new TextDecoder("utf-8", { fatal: true });

function clampU32(n: number): number {
  return n > 0xffffffff ? 0xffffffff : n;
}

function serializeWasmOptions(opts: Options): Uint8Array {
  if (isDefaultOptions(opts)) {
    return new Uint8Array(0);
  }
  const l = opts.limits;
  const json = `{"merge_keys":"${opts.mergeKeys}","unknown_tags":"${opts.unknownTags}","limits":{"max_input_bytes":${clampU32(l.maxInputBytes)},"max_depth":${clampU32(l.maxDepth)},"max_nodes":${clampU32(l.maxNodes)},"max_expanded_nodes":${clampU32(l.maxExpandedNodes)},"max_alias_events":${clampU32(l.maxAliasEvents)},"max_owned_bytes":${clampU32(l.maxOwnedBytes)},"max_scalar_bytes":${clampU32(l.maxScalarBytes)},"max_key_bytes":${clampU32(l.maxKeyBytes)},"max_mapping_entries":${clampU32(l.maxMappingEntries)},"max_documents":${clampU32(l.maxDocuments)},"max_output_bytes":${clampU32(l.maxOutputBytes)},"max_warnings":${clampU32(l.maxWarnings)}}}`;
  return encoder.encode(json);
}

// ---- WASM lifecycle and trap recovery ------------------------------------

interface WasmExports {
  memory: WebAssembly.Memory;
  abi_version: () => number;
  options_reserve: (len: number) => number;
  input_reserve: (len: number, maxInputBytes: number) => number;
  load_wire: (inputLen: number, optionsLen: number, singleDoc: number) => number;
  load_plain: (inputLen: number, optionsLen: number) => number;
  output_ptr: () => number;
  output_len: () => number;
  release: () => void;
}

const wasmModule = new WebAssembly.Module(wasmBytes());
let activeInstance: { inst: WebAssembly.Instance; exports: WasmExports } | null = null;
let instanceBusy = false;
let instanceTrapped = false;
let releaseCallCount = 0;
let recreateCount = 0;

function getWasmExports(): WasmExports {
  if (activeInstance === null || instanceTrapped) {
    const inst = new WebAssembly.Instance(wasmModule, {});
    const exports = inst.exports as unknown as WasmExports;
    const ver = exports.abi_version() >>> 0;
    if (ver !== ABI_VERSION) {
      throw notSourceError("internal", {}, `unexpected WASM abi_version: ${ver}`);
    }
    activeInstance = { inst, exports };
    instanceTrapped = false;
    recreateCount += 1;
  }
  return activeInstance.exports;
}

function assertWellFormedUnicode(s: string, label: string): void {
  if (typeof s !== "string") {
    throw notSourceError("internal", {}, `${label} must be a string`);
  }
  for (let i = 0; i < s.length; i++) {
    const code = s.charCodeAt(i);
    if (code >= 0xd800 && code <= 0xdbff) {
      const next = i + 1 < s.length ? s.charCodeAt(i + 1) : 0;
      if (next < 0xdc00 || next > 0xdfff) {
        throw notSourceError("internal", {}, `${label} contains an unpaired high surrogate`);
      }
      i++;
    } else if (code >= 0xdc00 && code <= 0xdfff) {
      throw notSourceError("internal", {}, `${label} contains an unpaired low surrogate`);
    }
  }
}

function toInputBytes(input: string | Uint8Array): Uint8Array {
  if (typeof input === "string") {
    for (let i = 0; i < input.length; i++) {
      const code = input.charCodeAt(i);
      if (code >= 0xd800 && code <= 0xdbff) {
        const next = i + 1 < input.length ? input.charCodeAt(i + 1) : 0;
        if (next < 0xdc00 || next > 0xdfff) {
          throw new TypeError("input string contains an unpaired high surrogate");
        }
        i++;
      } else if (code >= 0xdc00 && code <= 0xdfff) {
        throw new TypeError("input string contains an unpaired low surrogate");
      }
    }
    return encoder.encode(input);
  }
  if (input instanceof Uint8Array) {
    return input;
  }
  throw new TypeError("input must be a string or Uint8Array");
}

function decodeFallbackStatus2(bytes: Uint8Array, defaultMaxOutput: number): YamlprimsError {
  try {
    const text = decoder.decode(bytes);
    const parsed = JSON.parse(text) as {
      kind?: string;
      configured?: number;
      message?: string;
    };
    if (parsed.kind === "output_limit_exceeded") {
      const configured =
        typeof parsed.configured === "number" && Number.isSafeInteger(parsed.configured)
          ? parsed.configured
          : defaultMaxOutput;
      return notSourceError(
        "output_limit_exceeded",
        { configured },
        parsed.message ?? `encoded output exceeds max_output_bytes (${configured})`,
      );
    }
    return notSourceError("internal", {}, parsed.message ?? "WASM internal error");
  } catch {
    return notSourceError("internal", {}, "WASM status 2 fallback error");
  }
}

function runWasmLoad(
  inputBytes: Uint8Array,
  opts: Options,
  mode: "wire-single" | "wire-all" | "plain",
  simulateTrap = false,
): { status: number; output: Uint8Array } {
  if (instanceBusy) {
    throw notSourceError("internal", {}, "reentrant WASM execution is forbidden");
  }
  const optsBytes = serializeWasmOptions(opts);
  const exports = getWasmExports();
  instanceBusy = true;
  let localTrapped = false;
  try {
    const optPtr = exports.options_reserve(optsBytes.byteLength) >>> 0;
    if (optPtr === 0) {
      if (optsBytes.byteLength > MAX_OPTIONS_BYTES) {
        throw invalidOptionsError("options payload exceeds WASM options limit");
      }
      activeInstance = null;
      throw notSourceError("internal", {}, "WASM options reservation failed (linear memory ceiling exceeded)");
    }
    if (optsBytes.byteLength > 0) {
      new Uint8Array(exports.memory.buffer, optPtr, optsBytes.byteLength).set(optsBytes);
    }

    const wasmMaxInput = clampU32(opts.limits.maxInputBytes);
    const inPtr = exports.input_reserve(inputBytes.byteLength, wasmMaxInput) >>> 0;
    if (inPtr === 0) {
      if (inputBytes.byteLength > opts.limits.maxInputBytes) {
        throw new YamlprimsError({
          kind: "limit_exceeded",
          detail: { limit: "max_input_bytes", configured: opts.limits.maxInputBytes },
          site: { source: { byteOffset: 0, utf16Offset: 0, line: 1, column: 1 } },
          message: `max_input_bytes (${opts.limits.maxInputBytes}) exceeded`,
        });
      }
      if (inputBytes.byteLength > MAX_WASM_INPUT_CEILING) {
        throw new YamlprimsError({
          kind: "limit_exceeded",
          detail: { limit: "max_input_bytes", configured: MAX_WASM_INPUT_CEILING },
          site: { source: { byteOffset: 0, utf16Offset: 0, line: 1, column: 1 } },
          message: `max_input_bytes (${MAX_WASM_INPUT_CEILING}) exceeded`,
        });
      }
      activeInstance = null;
      throw notSourceError("internal", {}, "WASM input reservation failed (linear memory ceiling exceeded)");
    }
    if (inputBytes.byteLength > 0) {
      new Uint8Array(exports.memory.buffer, inPtr, inputBytes.byteLength).set(inputBytes);
    }

    if (simulateTrap) {
      throw new WebAssembly.RuntimeError("simulated unreachable trap");
    }

    const status =
      mode === "plain"
        ? exports.load_plain(inputBytes.byteLength, optsBytes.byteLength) >>> 0
        : exports.load_wire(
            inputBytes.byteLength,
            optsBytes.byteLength,
            mode === "wire-single" ? 1 : 0,
          ) >>> 0;
    const outPtr = exports.output_ptr() >>> 0;
    const outLen = exports.output_len() >>> 0;
    const output = new Uint8Array(exports.memory.buffer, outPtr, outLen).slice();
    return { status, output };
  } catch (err) {
    if (err instanceof WebAssembly.RuntimeError) {
      localTrapped = true;
      instanceTrapped = true;
      activeInstance = null;
      throw notSourceError("internal", {}, `WASM runtime trap: ${err.message}`);
    }
    throw err;
  } finally {
    instanceBusy = false;
    // Never call release() when the instance trapped: discard and recreate directly.
    if (!localTrapped && !instanceTrapped && activeInstance !== null) {
      exports.release();
      releaseCallCount += 1;
    }
  }
}

/**
 * Internal diagnostic & negative-control test hook for verifying WASM module
 * zero-imports, raw reservation invalidation, and trap/recreate recovery.
 */
export function _wasmTestHarness(): {
  module: WebAssembly.Module;
  createRawInstance: () => WasmExports;
  getActiveMemory: () => WebAssembly.Memory;
  simulateTrapOnLoad: (input: string) => void;
  runWithBusyInstance: (fn: () => void) => void;
  stats: () => { releaseCallCount: number; recreateCount: number; instanceTrapped: boolean };
} {
  return {
    module: wasmModule,
    createRawInstance: () => {
      const inst = new WebAssembly.Instance(wasmModule, {});
      return inst.exports as unknown as WasmExports;
    },
    getActiveMemory: () => getWasmExports().memory,
    simulateTrapOnLoad: (input: string) => {
      const opts = defaultOptions();
      runWasmLoad(toInputBytes(input), opts, "wire-single", true);
    },
    runWithBusyInstance: (fn: () => void) => {
      const prev = instanceBusy;
      instanceBusy = true;
      try {
        fn();
      } finally {
        instanceBusy = prev;
      }
    },
    stats: () => ({
      releaseCallCount,
      recreateCount,
      instanceTrapped,
    }),
  };
}

// ---- Single-pass O(N) UTF-16 offset index --------------------------------

class Utf16Index {
  private readonly bytes: Uint8Array;
  private readonly limit: number;
  private readonly pureAscii: boolean;
  private readonly cpByte: Uint32Array | null;
  private readonly cpUtf16: Uint32Array | null;

  constructor(bytes: Uint8Array, limitByteOffset?: number) {
    this.bytes = bytes;
    const lim =
      limitByteOffset !== undefined
        ? Math.min(Math.max(0, limitByteOffset), bytes.byteLength)
        : bytes.byteLength;
    this.limit = lim;

    let i = 0;
    while (i < lim && (bytes[i] ?? 0) < 0x80) {
      i++;
    }
    if (i === lim) {
      this.pureAscii = true;
      this.cpByte = null;
      this.cpUtf16 = null;
      return;
    }

    this.pureAscii = false;
    const numBuckets = (lim >>> 6) + 2;
    const cpByte = new Uint32Array(numBuckets);
    const cpUtf16 = new Uint32Array(numBuckets);
    const firstNonAsciiBucket = i >>> 6;
    for (let k = 0; k <= firstNonAsciiBucket; k++) {
      const b = k << 6;
      cpByte[k] = b;
      cpUtf16[k] = b;
    }

    let pos = firstNonAsciiBucket << 6;
    let u16 = pos;
    let nextBucket = firstNonAsciiBucket + 1;

    while (pos < lim) {
      while (nextBucket < numBuckets && pos >= nextBucket << 6) {
        cpByte[nextBucket] = pos;
        cpUtf16[nextBucket] = u16;
        nextBucket++;
      }
      const b0 = bytes[pos] ?? 0;
      if (b0 < 0x80) {
        pos += 1;
        u16 += 1;
      } else if ((b0 & 0xe0) === 0xc0) {
        if (pos + 2 > lim) break;
        pos += 2;
        u16 += 1;
      } else if ((b0 & 0xf0) === 0xe0) {
        if (pos + 3 > lim) break;
        pos += 3;
        u16 += 1;
      } else if ((b0 & 0xf8) === 0xf0) {
        if (pos + 4 > lim) break;
        pos += 4;
        u16 += 2;
      } else {
        break;
      }
    }
    while (nextBucket < numBuckets) {
      cpByte[nextBucket] = pos;
      cpUtf16[nextBucket] = u16;
      nextBucket++;
    }
    this.cpByte = cpByte;
    this.cpUtf16 = cpUtf16;
  }

  utf16At(byteOffset: number): number {
    if (byteOffset < 0 || byteOffset > this.limit) {
      throw notSourceError(
        "internal",
        {},
        `location byte_offset ${byteOffset} exceeds valid prefix (${this.limit})`,
      );
    }
    if (this.pureAscii) {
      return byteOffset;
    }
    const cpByte = this.cpByte!;
    const cpUtf16 = this.cpUtf16!;
    let k = byteOffset >>> 6;
    while (k > 0 && (cpByte[k] ?? 0) > byteOffset) {
      k--;
    }
    let pos = cpByte[k] ?? 0;
    let u16 = cpUtf16[k] ?? 0;
    const bytes = this.bytes;
    while (pos < byteOffset) {
      const b0 = bytes[pos] ?? 0;
      if (b0 < 0x80) {
        pos += 1;
        u16 += 1;
      } else if ((b0 & 0xe0) === 0xc0) {
        pos += 2;
        u16 += 1;
      } else if ((b0 & 0xf0) === 0xe0) {
        pos += 3;
        u16 += 1;
      } else if ((b0 & 0xf8) === 0xf0) {
        pos += 4;
        u16 += 2;
      } else {
        throw notSourceError("internal", {}, "invalid UTF-8 inside valid prefix");
      }
    }
    if (pos !== byteOffset) {
      throw notSourceError(
        "internal",
        {},
        `location byte_offset ${byteOffset} is not on a UTF-8 character boundary`,
      );
    }
    return u16;
  }
}

// ---- Float bits & Key equality (§5.1, §11.1) -----------------------------

const floatBuf = new ArrayBuffer(8);
const floatView = new DataView(floatBuf);

function bitsToFloat(bitsHex: string): { value: number; canonicalBits: string } {
  const raw = BigInt("0x" + bitsHex);
  // IEEE 754 binary64 NaN: exponent all 1s (0x7ff0_0000_0000_0000) and mantissa != 0.
  const isNan = (raw & 0x7ff0000000000000n) === 0x7ff0000000000000n && (raw & 0x000fffffffffffffn) !== 0n;
  const canonicalBig = isNan ? CANONICAL_NAN_BIGINT : raw;
  floatView.setBigUint64(0, canonicalBig, false);
  const value = floatView.getFloat64(0, false);
  const canonicalBits = isNan ? CANONICAL_NAN_BITS : bitsHex;
  return { value, canonicalBits };
}

export function floatToBits(f: number): string {
  if (Number.isNaN(f)) {
    return CANONICAL_NAN_BITS;
  }
  floatView.setFloat64(0, f, false);
  const u = floatView.getBigUint64(0, false);
  return u.toString(16).padStart(16, "0");
}

function normFloatBits(f: number): bigint {
  if (f === 0) {
    return 0n;
  }
  if (Number.isNaN(f)) {
    return CANONICAL_NAN_BIGINT;
  }
  floatView.setFloat64(0, f, false);
  return floatView.getBigUint64(0, false);
}

export function containsNan(node: Node): boolean {
  const stack: Node[] = [node];
  const visited = new Set<object>();
  while (stack.length > 0) {
    const cur = stack.pop()!;
    if (visited.has(cur)) continue;
    visited.add(cur);
    switch (cur.kind) {
      case "float":
        if (Number.isNaN(cur.value)) return true;
        break;
      case "sequence":
        for (const item of cur.items) stack.push(item);
        break;
      case "mapping":
        for (const [k, v] of cur.entries) {
          stack.push(k, v);
        }
        break;
      case "tagged":
        stack.push(cur.value);
        break;
    }
  }
  return false;
}

export function keyEq(a: Node, b: Node): boolean {
  if (a.kind !== b.kind) return false;
  switch (a.kind) {
    case "null":
      return true;
    case "bool":
      return a.value === (b as BoolNode).value;
    case "int":
      return a.value === (b as IntNode).value;
    case "float": {
      const bf = b as FloatNode;
      if (Number.isNaN(a.value) || Number.isNaN(bf.value)) return false;
      return normFloatBits(a.value) === normFloatBits(bf.value);
    }
    case "string":
      return a.value === (b as StringNode).value;
    case "sequence": {
      const bs = b as SequenceNode;
      if (a.items.length !== bs.items.length) return false;
      for (let i = 0; i < a.items.length; i++) {
        if (!keyEq(a.items[i]!, bs.items[i]!)) return false;
      }
      return true;
    }
    case "mapping": {
      const bm = b as MappingNode;
      if (a.entries.length !== bm.entries.length) return false;
      const used = new Uint8Array(bm.entries.length);
      for (const [ak, av] of a.entries) {
        let found = false;
        for (let j = 0; j < bm.entries.length; j++) {
          if (used[j]) continue;
          const [bk, bv] = bm.entries[j]!;
          if (keyEq(ak, bk)) {
            if (!keyEq(av, bv)) return false;
            used[j] = 1;
            found = true;
            break;
          }
        }
        if (!found) return false;
      }
      return true;
    }
    case "tagged": {
      const bt = b as TaggedNode;
      return a.tag.uri === bt.tag.uri && keyEq(a.value, bt.value);
    }
  }
}

export function mappingGet(
  mapping: MappingNode | ReadonlyArray<[Node, Node]>,
  key: string | Node,
): Node | undefined {
  const entries = Array.isArray(mapping) ? mapping : (mapping as MappingNode).entries;
  if (typeof key === "string") {
    for (const [k, v] of entries) {
      if (k.kind === "string" && k.value === key) {
        return v;
      }
    }
    return undefined;
  }
  for (const [k, v] of entries) {
    if (keyEq(k, key)) {
      return v;
    }
  }
  return undefined;
}

// ---- Strict `yamlprims-value/1` wire decoder -----------------------------

type JsonVal =
  | { t: "null" }
  | { t: "bool"; v: boolean }
  | { t: "int"; raw: string; v: number }
  | { t: "str"; v: string }
  | { t: "arr"; items: JsonVal[] }
  | { t: "obj"; map: Map<string, JsonVal> };

class WireJsonParser {
  private readonly s: string;
  private pos = 0;
  private readonly maxDepth: number;

  constructor(s: string, maxDepth: number) {
    this.s = s;
    this.maxDepth = maxDepth;
  }

  parseRoot(): JsonVal {
    const val = this.parseValue(0);
    this.skipWs();
    if (this.pos !== this.s.length) {
      throw notSourceError("internal", {}, "trailing characters after wire JSON");
    }
    return val;
  }

  private skipWs(): void {
    while (this.pos < this.s.length) {
      const c = this.s.charCodeAt(this.pos);
      if (c === 0x20 || c === 0x09 || c === 0x0a || c === 0x0d) {
        this.pos++;
      } else {
        break;
      }
    }
  }

  private expectChar(code: number): void {
    this.skipWs();
    if (this.pos >= this.s.length || this.s.charCodeAt(this.pos) !== code) {
      throw notSourceError("internal", {}, "malformed wire JSON");
    }
    this.pos++;
  }

  private parseValue(depth: number): JsonVal {
    if (depth > this.maxDepth) {
      throw notSourceError("internal", {}, "wire JSON nesting exceeds maximum depth");
    }
    this.skipWs();
    if (this.pos >= this.s.length) {
      throw notSourceError("internal", {}, "unexpected end of wire JSON");
    }
    const c = this.s.charCodeAt(this.pos);
    if (c === 0x7b /* { */) return this.parseObject(depth + 1);
    if (c === 0x5b /* [ */) return this.parseArray(depth + 1);
    if (c === 0x22 /* " */) return { t: "str", v: this.parseString() };
    if (c === 0x74 /* t */) {
      if (this.s.startsWith("true", this.pos)) {
        this.pos += 4;
        return { t: "bool", v: true };
      }
      throw notSourceError("internal", {}, "invalid literal in wire JSON");
    }
    if (c === 0x66 /* f */) {
      if (this.s.startsWith("false", this.pos)) {
        this.pos += 5;
        return { t: "bool", v: false };
      }
      throw notSourceError("internal", {}, "invalid literal in wire JSON");
    }
    if (c === 0x6e /* n */) {
      if (this.s.startsWith("null", this.pos)) {
        this.pos += 4;
        return { t: "null" };
      }
      throw notSourceError("internal", {}, "invalid literal in wire JSON");
    }
    if (c >= 0x30 && c <= 0x39) {
      return this.parseUnsignedInt();
    }
    throw notSourceError("internal", {}, "unexpected token in wire JSON");
  }

  private parseUnsignedInt(): JsonVal {
    const start = this.pos;
    while (this.pos < this.s.length) {
      const c = this.s.charCodeAt(this.pos);
      if (c >= 0x30 && c <= 0x39) {
        this.pos++;
      } else if (c === 0x2e || c === 0x65 || c === 0x45 || c === 0x2b || c === 0x2d) {
        throw notSourceError("internal", {}, "non-canonical number in wire JSON");
      } else {
        break;
      }
    }
    const raw = this.s.slice(start, this.pos);
    if (raw.length > 1 && raw.charCodeAt(0) === 0x30) {
      throw notSourceError("internal", {}, "leading zero in wire metadata integer");
    }
    const big = BigInt(raw);
    if (big > BigInt(MAX_SAFE_METADATA_INT)) {
      throw notSourceError("internal", {}, "wire metadata integer exceeds 2^53-1");
    }
    return { t: "int", raw, v: Number(big) };
  }

  private parseString(): string {
    this.expectChar(0x22);
    const start = this.pos;
    let out = "";
    let chunkStart = start;
    while (this.pos < this.s.length) {
      const c = this.s.charCodeAt(this.pos);
      if (c === 0x22) {
        out += this.s.slice(chunkStart, this.pos);
        this.pos++;
        return out;
      }
      if (c < 0x20) {
        throw notSourceError("internal", {}, "unescaped control character in wire JSON string");
      }
      if (c >= 0xd800 && c <= 0xdbff) {
        const next = this.pos + 1 < this.s.length ? this.s.charCodeAt(this.pos + 1) : 0;
        if (next < 0xdc00 || next > 0xdfff) {
          throw notSourceError("internal", {}, "unpaired high surrogate in wire JSON string");
        }
        this.pos += 2;
        continue;
      }
      if (c >= 0xdc00 && c <= 0xdfff) {
        throw notSourceError("internal", {}, "unpaired low surrogate in wire JSON string");
      }
      if (c === 0x5c /* \ */) {
        out += this.s.slice(chunkStart, this.pos);
        this.pos++;
        if (this.pos >= this.s.length) {
          throw notSourceError("internal", {}, "unterminated escape in wire JSON string");
        }
        const esc = this.s.charCodeAt(this.pos++);
        switch (esc) {
          case 0x22:
            out += '"';
            break;
          case 0x5c:
            out += "\\";
            break;
          case 0x2f:
            out += "/";
            break;
          case 0x62:
            out += "\b";
            break;
          case 0x66:
            out += "\f";
            break;
          case 0x6e:
            out += "\n";
            break;
          case 0x72:
            out += "\r";
            break;
          case 0x74:
            out += "\t";
            break;
          case 0x75: {
            const hex = this.s.slice(this.pos, this.pos + 4);
            if (!/^[0-9a-fA-F]{4}$/.test(hex)) {
              throw notSourceError("internal", {}, "invalid \\uXXXX escape in wire JSON");
            }
            this.pos += 4;
            const code = parseInt(hex, 16);
            if (code >= 0xd800 && code <= 0xdbff) {
              if (
                this.s.charCodeAt(this.pos) !== 0x5c ||
                this.s.charCodeAt(this.pos + 1) !== 0x75
              ) {
                throw notSourceError("internal", {}, "unpaired high surrogate in wire JSON");
              }
              const lowHex = this.s.slice(this.pos + 2, this.pos + 6);
              if (!/^[0-9a-fA-F]{4}$/.test(lowHex)) {
                throw notSourceError("internal", {}, "invalid low surrogate escape in wire JSON");
              }
              const low = parseInt(lowHex, 16);
              if (low < 0xdc00 || low > 0xdfff) {
                throw notSourceError("internal", {}, "invalid surrogate pair in wire JSON");
              }
              this.pos += 6;
              out += String.fromCharCode(code, low);
            } else if (code >= 0xdc00 && code <= 0xdfff) {
              throw notSourceError("internal", {}, "unpaired low surrogate in wire JSON");
            } else {
              out += String.fromCharCode(code);
            }
            break;
          }
          default:
            throw notSourceError("internal", {}, "unknown escape in wire JSON string");
        }
        chunkStart = this.pos;
        continue;
      }
      this.pos++;
    }
    throw notSourceError("internal", {}, "unterminated string in wire JSON");
  }

  private parseArray(depth: number): JsonVal {
    this.expectChar(0x5b);
    this.skipWs();
    const items: JsonVal[] = [];
    if (this.pos < this.s.length && this.s.charCodeAt(this.pos) === 0x5d) {
      this.pos++;
      return { t: "arr", items };
    }
    while (true) {
      items.push(this.parseValue(depth));
      this.skipWs();
      if (this.pos < this.s.length && this.s.charCodeAt(this.pos) === 0x2c) {
        this.pos++;
        continue;
      }
      this.expectChar(0x5d);
      return { t: "arr", items };
    }
  }

  private parseObject(depth: number): JsonVal {
    this.expectChar(0x7b);
    this.skipWs();
    const map = new Map<string, JsonVal>();
    if (this.pos < this.s.length && this.s.charCodeAt(this.pos) === 0x7d) {
      this.pos++;
      return { t: "obj", map };
    }
    while (true) {
      const key = this.parseString();
      if (map.has(key)) {
        throw notSourceError("internal", {}, `duplicate JSON object key: ${key}`);
      }
      this.expectChar(0x3a);
      const val = this.parseValue(depth);
      map.set(key, val);
      this.skipWs();
      if (this.pos < this.s.length && this.s.charCodeAt(this.pos) === 0x2c) {
        this.pos++;
        continue;
      }
      this.expectChar(0x7d);
      return { t: "obj", map };
    }
  }
}

function expectExactKeys(
  obj: Map<string, JsonVal>,
  required: readonly string[],
): void {
  if (obj.size !== required.length) {
    throw notSourceError("internal", {}, "unexpected or missing fields in wire object");
  }
  for (const k of required) {
    if (!obj.has(k)) {
      throw notSourceError("internal", {}, `missing required wire field: ${k}`);
    }
  }
}

function expectStr(val: JsonVal | undefined, field: string): string {
  if (!val || val.t !== "str") {
    throw notSourceError("internal", {}, `expected string for ${field}`);
  }
  return val.v;
}

function expectBool(val: JsonVal | undefined, field: string): boolean {
  if (!val || val.t !== "bool") {
    throw notSourceError("internal", {}, `expected boolean for ${field}`);
  }
  return val.v;
}

function expectInt(val: JsonVal | undefined, field: string): number {
  if (!val || val.t !== "int") {
    throw notSourceError("internal", {}, `expected unsigned integer for ${field}`);
  }
  return val.v;
}

function decodeLoc(val: JsonVal, idx: Utf16Index | null): Location {
  if (val.t !== "arr" || val.items.length !== 3) {
    throw notSourceError("internal", {}, "Loc must be [byte, line, col]");
  }
  const byteOffset = expectInt(val.items[0], "Loc.byte");
  const line = expectInt(val.items[1], "Loc.line");
  const column = expectInt(val.items[2], "Loc.col");
  if (line < 1 || column < 1) {
    throw notSourceError("internal", {}, "Loc line and column must be >= 1");
  }
  const utf16Offset = idx !== null ? idx.utf16At(byteOffset) : byteOffset;
  return { byteOffset, utf16Offset, line, column };
}

function decodeSpan(val: JsonVal, idx: Utf16Index | null): Span {
  if (val.t !== "arr" || val.items.length !== 6) {
    throw notSourceError("internal", {}, "Span must be 6-element array");
  }
  const sb = expectInt(val.items[0], "Span.start_byte");
  const sl = expectInt(val.items[1], "Span.start_line");
  const sc = expectInt(val.items[2], "Span.start_col");
  const eb = expectInt(val.items[3], "Span.end_byte");
  const el = expectInt(val.items[4], "Span.end_line");
  const ec = expectInt(val.items[5], "Span.end_col");
  if (sl < 1 || sc < 1 || el < 1 || ec < 1 || eb < sb) {
    throw notSourceError("internal", {}, "invalid Span coordinates");
  }
  return {
    start: {
      byteOffset: sb,
      utf16Offset: idx !== null ? idx.utf16At(sb) : sb,
      line: sl,
      column: sc,
    },
    end: {
      byteOffset: eb,
      utf16Offset: idx !== null ? idx.utf16At(eb) : eb,
      line: el,
      column: ec,
    },
  };
}

interface DecodeContext {
  opts: Options;
  idx: Utf16Index | null;
  nodesRemaining: number;
  forbidLocations: boolean;
}

function decodeNode(val: JsonVal, ctx: DecodeContext, depth: number): Node {
  if (val.t !== "obj") {
    throw notSourceError("internal", {}, "Node must be a JSON object");
  }
  if (ctx.nodesRemaining <= 0) {
    throw notSourceError("internal", {}, "wire Node count exceeds max_nodes");
  }
  ctx.nodesRemaining -= 1;

  const map = val.map;
  const kind = expectStr(map.get("kind"), "Node.kind");

  const spanVal = map.get("span");
  const viaVal = map.get("via");
  const tagStartVal = map.get("tag_start");
  if (!spanVal || !viaVal || !tagStartVal) {
    throw notSourceError("internal", {}, "Node missing location fields");
  }
  if (ctx.forbidLocations) {
    if (spanVal.t !== "null" || viaVal.t !== "null" || tagStartVal.t !== "null") {
      throw notSourceError("internal", {}, "PathKey must have null span, via and tag_start");
    }
  }
  const span = spanVal.t === "null" ? null : decodeSpan(spanVal, ctx.idx);
  let via: Via | null = null;
  if (viaVal.t !== "null") {
    if (viaVal.t !== "obj" || viaVal.map.size !== 1) {
      throw notSourceError("internal", {}, "invalid Node.via");
    }
    const aliasSpan = viaVal.map.get("alias");
    const mergeSpan = viaVal.map.get("merge");
    if (aliasSpan) {
      const s = decodeSpan(aliasSpan, ctx.idx);
      via = { kind: "alias", alias: s, useSite: s };
    } else if (mergeSpan) {
      const s = decodeSpan(mergeSpan, ctx.idx);
      via = { kind: "merge", merge: s, directive: s };
    } else {
      throw notSourceError("internal", {}, "unknown Via variant");
    }
  }
  const tagStart = tagStartVal.t === "null" ? null : decodeLoc(tagStartVal, ctx.idx);

  switch (kind) {
    case "null": {
      expectExactKeys(map, ["kind", "span", "via", "tag_start"]);
      return { kind: "null", span, via, tagStart };
    }
    case "bool": {
      expectExactKeys(map, ["kind", "value", "span", "via", "tag_start"]);
      return {
        kind: "bool",
        value: expectBool(map.get("value"), "bool.value"),
        span,
        via,
        tagStart,
      };
    }
    case "int": {
      expectExactKeys(map, ["kind", "value", "span", "via", "tag_start"]);
      const raw = expectStr(map.get("value"), "int.value");
      if (!/^(0|-?[1-9][0-9]*)$/.test(raw)) {
        throw notSourceError("internal", {}, `non-canonical wire int: ${raw}`);
      }
      const big = BigInt(raw);
      if (big < I128_MIN || big > U128_MAX) {
        throw notSourceError("internal", {}, `wire int out of 128-bit range: ${raw}`);
      }
      return { kind: "int", value: big, span, via, tagStart };
    }
    case "float": {
      expectExactKeys(map, ["kind", "bits", "span", "via", "tag_start"]);
      const rawBits = expectStr(map.get("bits"), "float.bits");
      if (!/^[0-9a-f]{16}$/.test(rawBits)) {
        throw notSourceError("internal", {}, `invalid wire float bits: ${rawBits}`);
      }
      const { value, canonicalBits } = bitsToFloat(rawBits);
      return { kind: "float", value, bits: canonicalBits, span, via, tagStart };
    }
    case "string": {
      expectExactKeys(map, ["kind", "value", "span", "via", "tag_start"]);
      const strVal = expectStr(map.get("value"), "string.value");
      return { kind: "string", value: strVal, span, via, tagStart };
    }
    case "sequence": {
      expectExactKeys(map, ["kind", "items", "span", "via", "tag_start"]);
      if (depth + 1 > ctx.opts.limits.maxDepth) {
        throw notSourceError("internal", {}, "wire sequence depth exceeds max_depth");
      }
      const itemsVal = map.get("items")!;
      if (itemsVal.t !== "arr") {
        throw notSourceError("internal", {}, "sequence.items must be an array");
      }
      const items = itemsVal.items.map((it) => decodeNode(it, ctx, depth + 1));
      return { kind: "sequence", items, span, via, tagStart };
    }
    case "mapping": {
      expectExactKeys(map, ["kind", "entries", "span", "via", "tag_start"]);
      if (depth + 1 > ctx.opts.limits.maxDepth) {
        throw notSourceError("internal", {}, "wire mapping depth exceeds max_depth");
      }
      const entriesVal = map.get("entries")!;
      if (entriesVal.t !== "arr") {
        throw notSourceError("internal", {}, "mapping.entries must be an array");
      }
      if (entriesVal.items.length > ctx.opts.limits.maxMappingEntries) {
        throw notSourceError("internal", {}, "wire mapping entries exceed max_mapping_entries");
      }
      const entries: Array<[Node, Node]> = [];
      for (const item of entriesVal.items) {
        if (item.t !== "obj") {
          throw notSourceError("internal", {}, "mapping entry must be an object");
        }
        expectExactKeys(item.map, ["key", "value"]);
        const k = decodeNode(item.map.get("key")!, ctx, depth + 1);
        const v = decodeNode(item.map.get("value")!, ctx, depth + 1);
        if (containsNan(k)) {
          throw notSourceError("internal", {}, "wire mapping key contains NaN");
        }
        for (const [ek] of entries) {
          if (keyEq(ek, k)) {
            throw notSourceError("internal", {}, "wire mapping contains duplicate key");
          }
        }
        entries.push([k, v]);
      }
      return { kind: "mapping", entries, span, via, tagStart };
    }
    case "tagged": {
      expectExactKeys(map, ["kind", "tag", "value", "span", "via", "tag_start"]);
      const tagVal = map.get("tag")!;
      if (tagVal.t !== "obj") {
        throw notSourceError("internal", {}, "tagged.tag must be an object");
      }
      expectExactKeys(tagVal.map, ["uri", "handle", "suffix"]);
      const uri = expectStr(tagVal.map.get("uri"), "tag.uri");
      const handle = expectStr(tagVal.map.get("handle"), "tag.handle");
      const suffix = expectStr(tagVal.map.get("suffix"), "tag.suffix");
      if (uri.length === 0) {
        throw notSourceError("internal", {}, "tag.uri must not be empty");
      }
      // Tagged wrapper does not add collection depth; nested Tagged-in-Tagged is forbidden.
      const inner = decodeNode(map.get("value")!, ctx, depth);
      if (inner.kind === "tagged") {
        throw notSourceError("internal", {}, "nested Tagged-in-Tagged is invalid in yamlprims-value/1");
      }
      return {
        kind: "tagged",
        tag: { uri, handle, suffix },
        value: inner,
        span,
        via,
        tagStart,
      };
    }
    default:
      throw notSourceError("internal", {}, `unknown wire Node kind: ${kind}`);
  }
}

function decodeErrorEnvelope(
  errObj: Map<string, JsonVal>,
  opts: Options,
  sourceBytes?: Uint8Array,
): YamlprimsError {
  expectExactKeys(errObj, ["kind", "detail", "site", "related", "path", "message"]);
  const kindStr = expectStr(errObj.get("kind"), "error.kind");
  const detailVal = errObj.get("detail")!;
  if (detailVal.t !== "obj") {
    throw notSourceError("internal", {}, "error.detail must be an object");
  }

  let detail: ErrorDetail;
  if (kindStr === "invalid_utf8") {
    expectExactKeys(detailVal.map, ["invalid_len"]);
    const il = detailVal.map.get("invalid_len")!;
    if (il.t === "null") {
      detail = { invalid_len: null };
    } else if (il.t === "int") {
      detail = { invalid_len: il.v };
    } else {
      throw notSourceError("internal", {}, "invalid invalid_utf8.invalid_len");
    }
  } else if (kindStr === "syntax") {
    expectExactKeys(detailVal.map, ["syntax_kind"]);
    const sk = expectStr(detailVal.map.get("syntax_kind"), "detail.syntax_kind");
    if (!SYNTAX_KINDS.has(sk)) {
      throw notSourceError("internal", {}, `unknown syntax_kind: ${sk}`);
    }
    detail = { syntax_kind: sk };
  } else if (kindStr === "limit_exceeded") {
    expectExactKeys(detailVal.map, ["limit", "configured"]);
    const lim = expectStr(detailVal.map.get("limit"), "detail.limit");
    if (!LIMIT_KINDS.has(lim)) {
      throw notSourceError("internal", {}, `unknown limit kind: ${lim}`);
    }
    const configured = expectInt(detailVal.map.get("configured"), "detail.configured");
    detail = { limit: lim as LimitKind, configured };
  } else if (kindStr === "type") {
    expectExactKeys(detailVal.map, ["expected", "found"]);
    const expected = expectStr(detailVal.map.get("expected"), "detail.expected");
    const found = expectStr(detailVal.map.get("found"), "detail.found");
    detail = { expected, found };
  } else if (kindStr === "missing_field" || kindStr === "unknown_field") {
    expectExactKeys(detailVal.map, ["field"]);
    const field = expectStr(detailVal.map.get("field"), "detail.field");
    detail = { field };
  } else if (kindStr === "output_limit_exceeded") {
    expectExactKeys(detailVal.map, ["configured"]);
    const configured = expectInt(detailVal.map.get("configured"), "detail.configured");
    detail = { configured };
  } else if (EMPTY_DETAIL_ERROR_KINDS.has(kindStr)) {
    expectExactKeys(detailVal.map, []);
    detail = {};
  } else {
    throw notSourceError("internal", {}, `unknown error kind: ${kindStr}`);
  }

  const siteVal = errObj.get("site")!;
  if (siteVal.t !== "obj" || siteVal.map.size !== 1) {
    throw notSourceError("internal", {}, "invalid error.site");
  }

  // For invalid_utf8, only the prefix up to the site's byte_offset is guaranteed valid UTF-8.
  let validPrefixLimit: number | undefined;
  const srcLocRaw = siteVal.map.get("source");
  if (srcLocRaw && srcLocRaw.t === "arr" && srcLocRaw.items.length === 3) {
    validPrefixLimit = expectInt(srcLocRaw.items[0], "site.source.byte");
  }
  const idx =
    sourceBytes !== undefined
      ? new Utf16Index(
          sourceBytes,
          kindStr === "invalid_utf8" ? validPrefixLimit : sourceBytes.byteLength,
        )
      : null;

  let site: ErrorSite;
  if (srcLocRaw) {
    site = { source: decodeLoc(srcLocRaw, idx) };
  } else {
    const ns = siteVal.map.get("not_source");
    if (!ns || ns.t !== "obj") {
      throw notSourceError("internal", {}, "invalid error.site.not_source");
    }
    expectExactKeys(ns.map, ["last_known"]);
    const lk = ns.map.get("last_known")!;
    site = {
      notSource: {
        lastKnown: lk.t === "null" ? null : decodeLoc(lk, idx),
      },
    };
  }

  const relVal = errObj.get("related")!;
  if (relVal.t !== "arr") {
    throw notSourceError("internal", {}, "error.related must be an array");
  }
  const related: Related[] = [];
  for (const r of relVal.items) {
    if (r.t !== "obj") {
      throw notSourceError("internal", {}, "related entry must be an object");
    }
    expectExactKeys(r.map, ["role", "span"]);
    const role = expectStr(r.map.get("role"), "related.role");
    if (
      role !== "first_definition" &&
      role !== "alias_use" &&
      role !== "merge_directive" &&
      role !== "anchor_definition"
    ) {
      throw notSourceError("internal", {}, `unknown related.role: ${role}`);
    }
    const span = decodeSpan(r.map.get("span")!, idx);
    related.push({ role, span });
  }

  const pathVal = errObj.get("path")!;
  if (pathVal.t !== "arr") {
    throw notSourceError("internal", {}, "error.path must be an array");
  }
  if (pathVal.items.length > opts.limits.maxDepth) {
    throw notSourceError("internal", {}, "error.path length exceeds max_depth");
  }
  const path: PathSegment[] = [];
  for (const seg of pathVal.items) {
    if (seg.t !== "obj" || seg.map.size !== 1) {
      throw notSourceError("internal", {}, "invalid path segment");
    }
    const idxSeg = seg.map.get("index");
    const keySeg = seg.map.get("key");
    if (idxSeg) {
      path.push({ index: expectInt(idxSeg, "path.index") });
    } else if (keySeg) {
      const ctx: DecodeContext = {
        opts,
        idx: null,
        nodesRemaining: opts.limits.maxNodes,
        forbidLocations: true,
      };
      const keyNode = decodeNode(keySeg, ctx, 0);
      preflightKeyNode(keyNode, opts.limits);
      path.push({ key: keyNode });
    } else {
      throw notSourceError("internal", {}, "unknown path segment variant");
    }
  }

  const message = expectStr(errObj.get("message"), "error.message");
  return new YamlprimsError({
    kind: kindStr as ErrorKind,
    detail,
    site,
    related,
    path,
    message,
  });
}

export function decodeWire(
  wireJson: string,
  options?: OptionsInput,
  sourceInput?: string | Uint8Array,
): { kind: "documents"; documents: Document[] } | { kind: "error"; error: YamlprimsError } {
  const opts = validateOptions(options);
  assertWellFormedUnicode(wireJson, "wireJson");
  const wireByteLen = encoder.encode(wireJson).byteLength;
  if (wireByteLen > opts.limits.maxOutputBytes) {
    throw notSourceError(
      "output_limit_exceeded",
      { configured: opts.limits.maxOutputBytes },
      `wire input exceeds max_output_bytes (${opts.limits.maxOutputBytes})`,
    );
  }
  const sourceBytes = sourceInput !== undefined ? toInputBytes(sourceInput) : undefined;
  const maxJsonDepth = opts.limits.maxDepth * 4 + 32;
  const root = new WireJsonParser(wireJson, maxJsonDepth).parseRoot();
  if (root.t !== "obj") {
    throw notSourceError("internal", {}, "wire root must be a JSON object");
  }
  const fmt = expectStr(root.map.get("format"), "format");
  if (fmt !== "yamlprims-value") {
    throw notSourceError("internal", {}, `unsupported wire format: ${fmt}`);
  }
  const ver = expectInt(root.map.get("version"), "version");
  if (ver !== 1) {
    throw notSourceError("internal", {}, `unsupported wire version: ${ver}`);
  }

  if (root.map.has("documents")) {
    expectExactKeys(root.map, ["format", "version", "documents"]);
    const docsVal = root.map.get("documents")!;
    if (docsVal.t !== "arr") {
      throw notSourceError("internal", {}, "documents must be an array");
    }
    if (docsVal.items.length > opts.limits.maxDocuments) {
      throw notSourceError("internal", {}, "wire document count exceeds max_documents");
    }
    const idx = sourceBytes !== undefined ? new Utf16Index(sourceBytes) : null;
    const ctx: DecodeContext = {
      opts,
      idx,
      nodesRemaining: opts.limits.maxNodes,
      forbidLocations: false,
    };
    const documents: Document[] = [];
    for (const d of docsVal.items) {
      if (d.t !== "obj") {
        throw notSourceError("internal", {}, "Document must be an object");
      }
      expectExactKeys(d.map, ["yaml_version", "explicit_start", "root"]);
      const yvVal = d.map.get("yaml_version")!;
      let yamlVersion: [number, number] | null = null;
      if (yvVal.t !== "null") {
        if (yvVal.t !== "arr" || yvVal.items.length !== 2) {
          throw notSourceError("internal", {}, "yaml_version must be null or [major, minor]");
        }
        const major = expectInt(yvVal.items[0], "yaml_version.major");
        const minor = expectInt(yvVal.items[1], "yaml_version.minor");
        if (major > 0xffffffff || minor > 0xffffffff) {
          throw notSourceError("internal", {}, "yaml_version exceeds u32 range");
        }
        yamlVersion = [major, minor];
      }
      const explicitStart = expectBool(d.map.get("explicit_start"), "explicit_start");
      const rootNode = decodeNode(d.map.get("root")!, ctx, 0);
      documents.push({ yamlVersion, explicitStart, root: rootNode });
    }
    return { kind: "documents", documents };
  }

  if (root.map.has("error")) {
    expectExactKeys(root.map, ["format", "version", "error"]);
    const errVal = root.map.get("error")!;
    if (errVal.t !== "obj") {
      throw notSourceError("internal", {}, "error must be an object");
    }
    return {
      kind: "error",
      error: decodeErrorEnvelope(errVal.map, opts, sourceBytes),
    };
  }

  throw notSourceError("internal", {}, "wire envelope must contain documents or error");
}

// ---- Canonical wire encoder & plain projection ---------------------------

class BoundedWriter {
  private readonly chunks: string[] = [];
  private byteLen = 0;
  private readonly maxBytes: number;

  constructor(maxBytes: number) {
    this.maxBytes = maxBytes;
  }

  pushRaw(ascii: string): void {
    const len = ascii.length;
    if (this.byteLen + len > this.maxBytes) {
      this.throwOver();
    }
    this.byteLen += len;
    this.chunks.push(ascii);
  }

  pushMetaInt(n: number, label: string): void {
    if (
      typeof n !== "number" ||
      !Number.isInteger(n) ||
      n < 0 ||
      n > MAX_SAFE_METADATA_INT
    ) {
      throw notSourceError("internal", {}, `metadata integer out of range for ${label}`);
    }
    this.pushRaw(String(n));
  }

  pushString(s: string): void {
    assertWellFormedUnicode(s, "string");
    this.pushStringParts(s, "");
  }

  pushBoundedString(s: string, maxUtf8Bytes: number): void {
    assertWellFormedUnicode(s, "string");
    const utf8 = encoder.encode(s);
    if (utf8.byteLength <= maxUtf8Bytes) {
      this.pushStringParts(s, "");
      return;
    }
    // Ellipsis '…' is 3 UTF-8 bytes.
    let cut = Math.max(0, maxUtf8Bytes - 3);
    while (cut > 0 && ((utf8[cut] ?? 0) & 0xc0) === 0x80) {
      cut--;
    }
    const prefix = decoder.decode(utf8.subarray(0, cut));
    this.pushStringParts(prefix, "…");
  }

  private pushStringParts(a: string, b: string): void {
    assertWellFormedUnicode(a, "string");
    assertWellFormedUnicode(b, "string");
    const aBytes = encoder.encode(a).byteLength;
    const bBytes = encoder.encode(b).byteLength;
    if (this.byteLen + aBytes + bBytes + 2 > this.maxBytes) {
      this.throwOver();
    }
    this.pushRaw('"');
    for (const part of [a, b]) {
      let run = 0;
      for (let i = 0; i < part.length; i++) {
        const c = part.charCodeAt(i);
        let esc: string | null = null;
        if (c === 0x22) esc = '\\"';
        else if (c === 0x5c) esc = "\\\\";
        else if (c === 0x08) esc = "\\b";
        else if (c === 0x0c) esc = "\\f";
        else if (c === 0x0a) esc = "\\n";
        else if (c === 0x0d) esc = "\\r";
        else if (c === 0x09) esc = "\\t";
        else if (c < 0x20) esc = `\\u${c.toString(16).padStart(4, "0")}`;
        if (esc !== null) {
          if (i > run) {
            this.pushUtf8Slice(part.slice(run, i));
          }
          this.pushRaw(esc);
          run = i + 1;
        }
      }
      if (run < part.length) {
        this.pushUtf8Slice(part.slice(run));
      }
    }
    this.pushRaw('"');
  }

  private pushUtf8Slice(sub: string): void {
    const len = encoder.encode(sub).byteLength;
    if (this.byteLen + len > this.maxBytes) {
      this.throwOver();
    }
    this.byteLen += len;
    this.chunks.push(sub);
  }

  pushLoc(loc: Location): void {
    if (loc.line < 1 || loc.column < 1) {
      throw notSourceError("internal", {}, "Location line and column must be >= 1");
    }
    this.pushRaw("[");
    this.pushMetaInt(loc.byteOffset, "Location.byteOffset");
    this.pushRaw(",");
    this.pushMetaInt(loc.line, "Location.line");
    this.pushRaw(",");
    this.pushMetaInt(loc.column, "Location.column");
    this.pushRaw("]");
  }

  pushSpan(span: Span): void {
    if (
      span.start.line < 1 ||
      span.start.column < 1 ||
      span.end.line < 1 ||
      span.end.column < 1 ||
      span.end.byteOffset < span.start.byteOffset
    ) {
      throw notSourceError("internal", {}, "invalid Span coordinates");
    }
    this.pushRaw("[");
    this.pushMetaInt(span.start.byteOffset, "Span.start.byteOffset");
    this.pushRaw(",");
    this.pushMetaInt(span.start.line, "Span.start.line");
    this.pushRaw(",");
    this.pushMetaInt(span.start.column, "Span.start.column");
    this.pushRaw(",");
    this.pushMetaInt(span.end.byteOffset, "Span.end.byteOffset");
    this.pushRaw(",");
    this.pushMetaInt(span.end.line, "Span.end.line");
    this.pushRaw(",");
    this.pushMetaInt(span.end.column, "Span.end.column");
    this.pushRaw("]");
  }

  finish(): string {
    return this.chunks.join("");
  }

  private throwOver(): never {
    throw notSourceError(
      "output_limit_exceeded",
      { configured: this.maxBytes },
      `encoded output exceeds max_output_bytes (${this.maxBytes})`,
    );
  }
}

interface EncodeState {
  opts: Options;
  nodesRemaining: number;
  activeAncestors: Set<object>;
}

function writeNodeValue(
  w: BoundedWriter,
  n: Node,
  withSpans: boolean,
  st: EncodeState,
  depth: number,
): void {
  if (st.nodesRemaining <= 0) {
    throw notSourceError(
      "limit_exceeded",
      { limit: "max_nodes", configured: st.opts.limits.maxNodes },
      "tree exceeds max_nodes",
    );
  }
  st.nodesRemaining -= 1;
  if (st.activeAncestors.has(n)) {
    throw notSourceError(
      "limit_exceeded",
      { limit: "max_depth", configured: st.opts.limits.maxDepth },
      "cyclic node graph exceeds max_depth",
    );
  }

  w.pushRaw('{"kind":');
  w.pushString(n.kind);
  switch (n.kind) {
    case "null":
      break;
    case "bool":
      w.pushRaw(n.value ? ',"value":true' : ',"value":false');
      break;
    case "int": {
      if (typeof n.value !== "bigint" || n.value < I128_MIN || n.value > U128_MAX) {
        throw notSourceError("integer_out_of_range", {}, "integer node out of 128-bit range");
      }
      w.pushRaw(',"value":');
      w.pushString(n.value.toString(10));
      break;
    }
    case "float": {
      w.pushRaw(',"bits":');
      w.pushString(floatToBits(n.value));
      break;
    }
    case "string": {
      w.pushRaw(',"value":');
      w.pushString(n.value);
      break;
    }
    case "sequence": {
      if (depth + 1 > st.opts.limits.maxDepth) {
        throw notSourceError(
          "limit_exceeded",
          { limit: "max_depth", configured: st.opts.limits.maxDepth },
          "sequence nesting exceeds max_depth",
        );
      }
      st.activeAncestors.add(n);
      w.pushRaw(',"items":[');
      for (let i = 0; i < n.items.length; i++) {
        if (i > 0) w.pushRaw(",");
        writeNode(w, n.items[i]!, withSpans, st, depth + 1);
      }
      w.pushRaw("]");
      st.activeAncestors.delete(n);
      break;
    }
    case "mapping": {
      if (depth + 1 > st.opts.limits.maxDepth) {
        throw notSourceError(
          "limit_exceeded",
          { limit: "max_depth", configured: st.opts.limits.maxDepth },
          "mapping nesting exceeds max_depth",
        );
      }
      if (n.entries.length > st.opts.limits.maxMappingEntries) {
        throw notSourceError(
          "limit_exceeded",
          { limit: "max_mapping_entries", configured: st.opts.limits.maxMappingEntries },
          "mapping entries exceed max_mapping_entries",
        );
      }
      st.activeAncestors.add(n);
      w.pushRaw(',"entries":[');
      for (let i = 0; i < n.entries.length; i++) {
        const [k, v] = n.entries[i]!;
        if (containsNan(k)) {
          throw notSourceError("invalid_key", {}, "mapping key contains NaN");
        }
        for (let j = 0; j < i; j++) {
          if (keyEq(n.entries[j]![0], k)) {
            throw notSourceError("duplicate_key", {}, "duplicate mapping key");
          }
        }
        if (i > 0) w.pushRaw(",");
        w.pushRaw('{"key":');
        writeNode(w, k, withSpans, st, depth + 1);
        w.pushRaw(',"value":');
        writeNode(w, v, withSpans, st, depth + 1);
        w.pushRaw("}");
      }
      w.pushRaw("]");
      st.activeAncestors.delete(n);
      break;
    }
    case "tagged": {
      if (!n.tag || typeof n.tag.uri !== "string" || n.tag.uri.length === 0) {
        throw notSourceError("internal", {}, "tagged node must have a non-empty tag.uri");
      }
      if (n.value.kind === "tagged") {
        throw notSourceError("internal", {}, "nested Tagged-in-Tagged is invalid in yamlprims-value/1");
      }
      st.activeAncestors.add(n);
      w.pushRaw(',"tag":{"uri":');
      w.pushString(n.tag.uri);
      w.pushRaw(',"handle":');
      w.pushString(n.tag.handle ?? "");
      w.pushRaw(',"suffix":');
      w.pushString(n.tag.suffix ?? "");
      w.pushRaw('},"value":');
      writeNode(w, n.value, withSpans, st, depth);
      st.activeAncestors.delete(n);
      break;
    }
  }
}

function writeNode(
  w: BoundedWriter,
  n: Node,
  withSpans: boolean,
  st: EncodeState,
  depth: number,
): void {
  writeNodeValue(w, n, withSpans, st, depth);
  w.pushRaw(',"span":');
  if (withSpans && n.span !== null && n.span !== undefined) {
    w.pushSpan(n.span);
  } else {
    w.pushRaw("null");
  }
  w.pushRaw(',"via":');
  if (withSpans && n.via !== null && n.via !== undefined) {
    if (n.via.kind === "alias") {
      w.pushRaw('{"alias":');
      w.pushSpan(n.via.alias ?? n.via.useSite);
      w.pushRaw("}");
    } else {
      w.pushRaw('{"merge":');
      w.pushSpan(n.via.merge ?? n.via.directive);
      w.pushRaw("}");
    }
  } else {
    w.pushRaw("null");
  }
  w.pushRaw(',"tag_start":');
  if (withSpans && n.tagStart !== null && n.tagStart !== undefined) {
    w.pushLoc(n.tagStart);
  } else {
    w.pushRaw("null");
  }
  w.pushRaw("}");
}

export function encode(docs: readonly Document[], options?: OptionsInput): string {
  const opts = validateOptions(options);
  if (docs.length > opts.limits.maxDocuments) {
    throw notSourceError(
      "limit_exceeded",
      { limit: "max_documents", configured: opts.limits.maxDocuments },
      `document count exceeds max_documents (${opts.limits.maxDocuments})`,
    );
  }
  const w = new BoundedWriter(opts.limits.maxOutputBytes);
  const st: EncodeState = {
    opts,
    nodesRemaining: opts.limits.maxNodes,
    activeAncestors: new Set<object>(),
  };
  w.pushRaw('{"format":"yamlprims-value","version":1,"documents":[');
  for (let i = 0; i < docs.length; i++) {
    if (i > 0) w.pushRaw(",");
    const d = docs[i]!;
    w.pushRaw('{"yaml_version":');
    if (d.yamlVersion !== null && d.yamlVersion !== undefined) {
      const [maj, min] = d.yamlVersion;
      if (
        !Number.isInteger(maj) ||
        !Number.isInteger(min) ||
        maj < 0 ||
        min < 0 ||
        maj > 0xffffffff ||
        min > 0xffffffff
      ) {
        throw notSourceError("internal", {}, "invalid Document.yamlVersion");
      }
      w.pushRaw(`[${maj},${min}]`);
    } else {
      w.pushRaw("null");
    }
    w.pushRaw(d.explicitStart ? ',"explicit_start":true,"root":' : ',"explicit_start":false,"root":');
    writeNode(w, d.root, true, st, 0);
    w.pushRaw("}");
  }
  w.pushRaw("]}");
  return w.finish();
}

function tagWeightBytes(tag: Tag): bigint {
  assertWellFormedUnicode(tag.uri, "tag.uri");
  assertWellFormedUnicode(tag.handle ?? "", "tag.handle");
  assertWellFormedUnicode(tag.suffix ?? "", "tag.suffix");
  return BigInt(
    encoder.encode(tag.uri).byteLength +
      encoder.encode(tag.handle ?? "").byteLength +
      encoder.encode(tag.suffix ?? "").byteLength,
  );
}

function preflightKeyNode(rootKey: Node, limits: Limits): void {
  const maxKey = BigInt(limits.maxKeyBytes);
  let weight = NODE_OVERHEAD;
  if (weight > maxKey) {
    throw notSourceError(
      "limit_exceeded",
      { limit: "max_key_bytes", configured: limits.maxKeyBytes },
      "error path key exceeds max_key_bytes",
    );
  }
  const stack: Array<{ node: Node; depth: number }> = [{ node: rootKey, depth: 0 }];
  while (stack.length > 0) {
    const { node, depth } = stack.pop()!;
    let more = 0n;
    let children = 0;
    if (node.kind === "string") {
      assertWellFormedUnicode(node.value, "string node");
      more = BigInt(encoder.encode(node.value).byteLength);
    } else if (node.kind === "tagged") {
      more = tagWeightBytes(node.tag) + NODE_OVERHEAD;
      children = 1;
    } else if (node.kind === "sequence") {
      more = NODE_OVERHEAD * BigInt(node.items.length);
      children = node.items.length;
    } else if (node.kind === "mapping") {
      more = (2n * NODE_OVERHEAD + ENTRY_OVERHEAD) * BigInt(node.entries.length);
      children = 2 * node.entries.length;
    }
    weight += more;
    if (weight > maxKey) {
      throw notSourceError(
        "limit_exceeded",
        { limit: "max_key_bytes", configured: limits.maxKeyBytes },
        "error path key exceeds max_key_bytes",
      );
    }
    if (children > 0 && node.kind !== "tagged" && depth + 1 > limits.maxDepth) {
      throw notSourceError(
        "limit_exceeded",
        { limit: "max_depth", configured: limits.maxDepth },
        "error path key nests deeper than max_depth",
      );
    }
    if (node.kind === "tagged") {
      stack.push({ node: node.value, depth });
    } else if (node.kind === "sequence") {
      for (const it of node.items) {
        stack.push({ node: it, depth: depth + 1 });
      }
    } else if (node.kind === "mapping") {
      for (const [k, v] of node.entries) {
        stack.push({ node: k, depth: depth + 1 });
        stack.push({ node: v, depth: depth + 1 });
      }
    }
  }
}

export function encodeError(err: YamlprimsError, options?: OptionsInput): string {
  const opts = validateOptions(options);
  if (err.path.length > opts.limits.maxDepth) {
    throw notSourceError(
      "limit_exceeded",
      { limit: "max_depth", configured: opts.limits.maxDepth },
      `error path has ${err.path.length} segments, more than max_depth`,
    );
  }
  for (const seg of err.path) {
    if ("key" in seg) {
      preflightKeyNode(seg.key, opts.limits);
    }
  }

  const w = new BoundedWriter(opts.limits.maxOutputBytes);
  w.pushRaw('{"format":"yamlprims-value","version":1,"error":{"kind":');
  w.pushString(err.kind);
  w.pushRaw(',"detail":');

  const d = err.detail as Record<string, unknown>;
  if (err.kind === "invalid_utf8") {
    if (Object.keys(d).length !== 1 || !("invalid_len" in d)) {
      throw notSourceError("internal", {}, "invalid invalid_utf8 detail");
    }
    const il = d["invalid_len"];
    if (il === null) {
      w.pushRaw('{"invalid_len":null}');
    } else if (typeof il === "number") {
      w.pushRaw('{"invalid_len":');
      w.pushMetaInt(il, "detail.invalid_len");
      w.pushRaw("}");
    } else {
      throw notSourceError("internal", {}, "invalid invalid_utf8.invalid_len");
    }
  } else if (err.kind === "syntax") {
    if (Object.keys(d).length !== 1 || typeof d["syntax_kind"] !== "string") {
      throw notSourceError("internal", {}, "invalid syntax detail");
    }
    const sk = d["syntax_kind"];
    if (!SYNTAX_KINDS.has(sk)) {
      throw notSourceError("internal", {}, `unknown syntax_kind: ${sk}`);
    }
    w.pushRaw('{"syntax_kind":');
    w.pushString(sk);
    w.pushRaw("}");
  } else if (err.kind === "limit_exceeded") {
    if (
      Object.keys(d).length !== 2 ||
      typeof d["limit"] !== "string" ||
      typeof d["configured"] !== "number" ||
      !LIMIT_KINDS.has(d["limit"])
    ) {
      throw notSourceError("internal", {}, "invalid limit_exceeded detail");
    }
    w.pushRaw('{"limit":');
    w.pushString(d["limit"]);
    w.pushRaw(',"configured":');
    w.pushMetaInt(d["configured"], "detail.configured");
    w.pushRaw("}");
  } else if (err.kind === "type") {
    if (
      Object.keys(d).length !== 2 ||
      typeof d["expected"] !== "string" ||
      typeof d["found"] !== "string"
    ) {
      throw notSourceError("internal", {}, "invalid type detail");
    }
    w.pushRaw('{"expected":');
    w.pushBoundedString(d["expected"], MAX_FIELD_BYTES);
    w.pushRaw(',"found":');
    w.pushBoundedString(d["found"], MAX_FIELD_BYTES);
    w.pushRaw("}");
  } else if (err.kind === "missing_field" || err.kind === "unknown_field") {
    if (Object.keys(d).length !== 1 || typeof d["field"] !== "string") {
      throw notSourceError("internal", {}, `invalid ${err.kind} detail`);
    }
    w.pushRaw('{"field":');
    w.pushBoundedString(d["field"], MAX_FIELD_BYTES);
    w.pushRaw("}");
  } else if (err.kind === "output_limit_exceeded") {
    if (Object.keys(d).length !== 1 || typeof d["configured"] !== "number") {
      throw notSourceError("internal", {}, "invalid output_limit_exceeded detail");
    }
    w.pushRaw('{"configured":');
    w.pushMetaInt(d["configured"], "detail.configured");
    w.pushRaw("}");
  } else if (EMPTY_DETAIL_ERROR_KINDS.has(err.kind)) {
    if (Object.keys(d).length !== 0) {
      throw notSourceError("internal", {}, `non-empty detail for ${err.kind}`);
    }
    w.pushRaw("{}");
  } else {
    throw notSourceError("internal", {}, `unknown error kind: ${String(err.kind)}`);
  }

  w.pushRaw(',"site":');
  if (err.site.source !== undefined) {
    w.pushRaw('{"source":');
    w.pushLoc(err.site.source);
    w.pushRaw("}");
  } else {
    w.pushRaw('{"not_source":{"last_known":');
    if (err.site.notSource.lastKnown !== null) {
      w.pushLoc(err.site.notSource.lastKnown);
    } else {
      w.pushRaw("null");
    }
    w.pushRaw("}}");
  }

  w.pushRaw(',"related":[');
  for (let i = 0; i < err.related.length; i++) {
    if (i > 0) w.pushRaw(",");
    const r = err.related[i]!;
    w.pushRaw('{"role":');
    w.pushString(r.role);
    w.pushRaw(',"span":');
    w.pushSpan(r.span);
    w.pushRaw("}");
  }

  const st: EncodeState = {
    opts,
    nodesRemaining: opts.limits.maxNodes,
    activeAncestors: new Set<object>(),
  };
  w.pushRaw('],"path":[');
  for (let i = 0; i < err.path.length; i++) {
    if (i > 0) w.pushRaw(",");
    const p = err.path[i]!;
    if ("index" in p) {
      w.pushRaw('{"index":');
      w.pushMetaInt(p.index, "path.index");
      w.pushRaw("}");
    } else {
      w.pushRaw('{"key":');
      writeNodeValue(w, p.key, false, st, 0);
      w.pushRaw(',"span":null,"via":null,"tag_start":null}}');
    }
  }

  w.pushRaw('],"message":');
  w.pushBoundedString(err.rawMessage, MAX_MESSAGE_BYTES);
  w.pushRaw("}}");
  return w.finish();
}

function isDocument(val: Node | Document): val is Document {
  return "root" in val && !("kind" in val);
}

function lossError(node: Node, what: string, expected = "plain JSON value"): YamlprimsError {
  const site: ErrorSite =
    node.span !== null && node.span !== undefined
      ? { source: node.span.start }
      : { notSource: { lastKnown: null } };
  return new YamlprimsError({
    kind: "type",
    detail: { expected, found: what },
    site,
    message: `cannot write ${what} as plain JSON without loss`,
  });
}

/**
 * Lossy projection to plain JavaScript values (`spec-v1.4 §11.3`):
 * - `Int` within `±(2^53-1)` -> `number`; outside -> throws `YamlprimsError` (`kind: "type"`).
 * - `Float` (including `-0.0`, `NaN`, `±Infinity`) -> `number` in memory.
 * - `Mapping` with `String` keys -> null-prototype object (`Object.create(null)`);
 *   non-`String` keys -> throws `YamlprimsError` (`kind: "type"`).
 * - `Tagged` -> throws `YamlprimsError` (`kind: "type"`).
 */
export function toPlain(nodeOrDoc: Node | Document, options?: OptionsInput): unknown {
  const opts = validateOptions(options);
  const root = isDocument(nodeOrDoc) ? nodeOrDoc.root : nodeOrDoc;
  let nodesRemaining = opts.limits.maxNodes;
  const active = new Set<object>();

  function walk(n: Node, depth: number): unknown {
    if (nodesRemaining <= 0) {
      throw notSourceError(
        "limit_exceeded",
        { limit: "max_nodes", configured: opts.limits.maxNodes },
        "tree exceeds max_nodes",
      );
    }
    nodesRemaining -= 1;
    if (active.has(n)) {
      throw notSourceError(
        "limit_exceeded",
        { limit: "max_depth", configured: opts.limits.maxDepth },
        "cyclic node graph exceeds max_depth",
      );
    }
    switch (n.kind) {
      case "null":
        return null;
      case "bool":
        return n.value;
      case "int": {
        const maxSafe = BigInt(MAX_SAFE_METADATA_INT);
        if (n.value < -maxSafe || n.value > maxSafe) {
          throw lossError(n, "integer outside safe integer range");
        }
        return n.value === 0n ? 0 : Number(n.value);
      }
      case "float":
        return n.value;
      case "string":
        assertWellFormedUnicode(n.value, "string node");
        return n.value;
      case "sequence": {
        if (depth + 1 > opts.limits.maxDepth) {
          throw notSourceError(
            "limit_exceeded",
            { limit: "max_depth", configured: opts.limits.maxDepth },
            "sequence nesting exceeds max_depth",
          );
        }
        active.add(n);
        const out = n.items.map((it) => walk(it, depth + 1));
        active.delete(n);
        return out;
      }
      case "mapping": {
        if (depth + 1 > opts.limits.maxDepth) {
          throw notSourceError(
            "limit_exceeded",
            { limit: "max_depth", configured: opts.limits.maxDepth },
            "mapping nesting exceeds max_depth",
          );
        }
        active.add(n);
        const obj = Object.create(null) as Record<string, unknown>;
        for (const [k, v] of n.entries) {
          if (k.kind !== "string") {
            active.delete(n);
            throw lossError(k, "non-string key");
          }
          try {
            assertWellFormedUnicode(k.value, "mapping key");
          } catch (err) {
            active.delete(n);
            throw err;
          }
          if (Object.prototype.hasOwnProperty.call(obj, k.value)) {
            active.delete(n);
            throw notSourceError("duplicate_key", {}, "duplicate mapping key");
          }
          obj[k.value] = walk(v, depth + 1);
        }
        active.delete(n);
        return obj;
      }
      case "tagged":
        throw lossError(n, "tagged value");
    }
  }

  return walk(root, 0);
}

function formatPlainFloat(f: number): string {
  if (Object.is(f, -0)) {
    return "-0.0";
  }
  const s = String(f);
  if (!s.includes(".") && !s.includes("e") && !s.includes("E")) {
    return s + ".0";
  }
  return s.replace("e+", "e");
}

export function encodePlain(nodeOrDoc: Node | Document, options?: OptionsInput): string {
  const opts = validateOptions(options);
  const root = isDocument(nodeOrDoc) ? nodeOrDoc.root : nodeOrDoc;
  const w = new BoundedWriter(opts.limits.maxOutputBytes);
  let nodesRemaining = opts.limits.maxNodes;
  const active = new Set<object>();

  function walk(n: Node, depth: number): void {
    if (nodesRemaining <= 0) {
      throw notSourceError(
        "limit_exceeded",
        { limit: "max_nodes", configured: opts.limits.maxNodes },
        "tree exceeds max_nodes",
      );
    }
    nodesRemaining -= 1;
    if (active.has(n)) {
      throw notSourceError(
        "limit_exceeded",
        { limit: "max_depth", configured: opts.limits.maxDepth },
        "cyclic node graph exceeds max_depth",
      );
    }
    switch (n.kind) {
      case "null":
        w.pushRaw("null");
        break;
      case "bool":
        w.pushRaw(n.value ? "true" : "false");
        break;
      case "int":
        w.pushRaw(n.value.toString(10));
        break;
      case "float":
        if (!Number.isFinite(n.value)) {
          throw lossError(n, "non-finite float");
        }
        w.pushRaw(formatPlainFloat(n.value));
        break;
      case "string":
        w.pushString(n.value);
        break;
      case "sequence":
        if (depth + 1 > opts.limits.maxDepth) {
          throw notSourceError(
            "limit_exceeded",
            { limit: "max_depth", configured: opts.limits.maxDepth },
            "sequence nesting exceeds max_depth",
          );
        }
        active.add(n);
        w.pushRaw("[");
        for (let i = 0; i < n.items.length; i++) {
          if (i > 0) w.pushRaw(",");
          walk(n.items[i]!, depth + 1);
        }
        w.pushRaw("]");
        active.delete(n);
        break;
      case "mapping":
        if (depth + 1 > opts.limits.maxDepth) {
          throw notSourceError(
            "limit_exceeded",
            { limit: "max_depth", configured: opts.limits.maxDepth },
            "mapping nesting exceeds max_depth",
          );
        }
        active.add(n);
        w.pushRaw("{");
        for (let i = 0; i < n.entries.length; i++) {
          const [k, v] = n.entries[i]!;
          if (k.kind !== "string") {
            active.delete(n);
            throw lossError(k, "non-string key");
          }
          if (i > 0) w.pushRaw(",");
          w.pushString(k.value);
          w.pushRaw(":");
          walk(v, depth + 1);
        }
        w.pushRaw("}");
        active.delete(n);
        break;
      case "tagged":
        throw lossError(n, "tagged value");
    }
  }

  walk(root, 0);
  return w.finish();
}

// ---- Primary entrypoints -------------------------------------------------

export function loadAll(input: string | Uint8Array, options?: OptionsInput): Document[] {
  const opts = validateOptions(options);
  const inputBytes = toInputBytes(input);
  const { status, output } = runWasmLoad(inputBytes, opts, "wire-all");
  if (status === 2) {
    throw decodeFallbackStatus2(output, opts.limits.maxOutputBytes);
  }
  const wireText = decoder.decode(output);
  const decoded = decodeWire(wireText, opts, inputBytes);
  if (decoded.kind === "error") {
    throw decoded.error;
  }
  return decoded.documents;
}

export function load(input: string | Uint8Array, options?: OptionsInput): Document {
  const opts = validateOptions(options);
  const inputBytes = toInputBytes(input);
  const { status, output } = runWasmLoad(inputBytes, opts, "wire-single");
  if (status === 2) {
    throw decodeFallbackStatus2(output, opts.limits.maxOutputBytes);
  }
  const wireText = decoder.decode(output);
  const decoded = decodeWire(wireText, opts, inputBytes);
  if (decoded.kind === "error") {
    throw decoded.error;
  }
  if (decoded.documents.length !== 1) {
    throw notSourceError("internal", {}, "expected single document from load_wire");
  }
  return decoded.documents[0]!;
}

export function loadBytes(input: Uint8Array, options?: OptionsInput): Document {
  if (!(input instanceof Uint8Array)) {
    throw new TypeError("loadBytes requires a Uint8Array");
  }
  return load(input, options);
}

export function loadAllBytes(input: Uint8Array, options?: OptionsInput): Document[] {
  if (!(input instanceof Uint8Array)) {
    throw new TypeError("loadAllBytes requires a Uint8Array");
  }
  return loadAll(input, options);
}

export function loadPlain(input: string | Uint8Array, options?: OptionsInput): string {
  const opts = validateOptions(options);
  const inputBytes = toInputBytes(input);
  const { status, output } = runWasmLoad(inputBytes, opts, "plain");
  if (status === 2) {
    throw decodeFallbackStatus2(output, opts.limits.maxOutputBytes);
  }
  const text = decoder.decode(output);
  if (status === 1) {
    const decoded = decodeWire(text, opts, inputBytes);
    if (decoded.kind === "error") {
      throw decoded.error;
    }
    throw notSourceError("internal", {}, "expected error envelope from load_plain status 1");
  }
  return text;
}
