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
export const WASM_HIGH_WATER_RECYCLE_BYTES = 16 * 1024 * 1024;
export const MAX_SAFE_METADATA_INT = 9_007_199_254_740_991; // 2^53 - 1
export const DEFAULT_IJSON_MAX_OUTPUT_BYTES = 16 * 1024 * 1024;
export const DEFAULT_EMIT_MAX_TAG_BYTES = 1_024;
export const DEFAULT_EMIT_MAX_OUTPUT_BYTES = 8 * 1024 * 1024;
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
export type CompatQuoting = "yaml11_safe" | "core_only" | "yaml11-safe" | "core-only";

export interface EmitLimits {
  maxDepth: number;
  maxNodes: number;
  maxScalarBytes: number;
  maxKeyBytes: number;
  maxMappingEntries: number;
  maxTagBytes: number;
  maxDocuments: number;
  maxOutputBytes: number;
}

export interface EmitLimitsInput {
  maxDepth?: number;
  maxNodes?: number;
  maxScalarBytes?: number;
  maxKeyBytes?: number;
  maxMappingEntries?: number;
  maxTagBytes?: number;
  maxDocuments?: number;
  maxOutputBytes?: number;
  max_depth?: number;
  max_nodes?: number;
  max_scalar_bytes?: number;
  max_key_bytes?: number;
  max_mapping_entries?: number;
  max_tag_bytes?: number;
  max_documents?: number;
  max_output_bytes?: number;
}

export interface EmitOptions {
  compatQuoting: "yaml11_safe" | "core_only";
  limits: EmitLimits;
}

export interface EmitOptionsInput {
  compatQuoting?: CompatQuoting;
  compat_quoting?: CompatQuoting;
  limits?: EmitLimitsInput;
}

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

export function defaultIjsonOptions(): Options {
  return {
    mergeKeys: "reject",
    unknownTags: "reject",
    limits: {
      ...defaultLimits(),
      maxOutputBytes: DEFAULT_IJSON_MAX_OUTPUT_BYTES,
    },
  };
}

export function validateIjsonOptions(options?: OptionsInput): Options {
  if (options === undefined) {
    return defaultIjsonOptions();
  }
  if (typeof options !== "object" || options === null || Array.isArray(options)) {
    throw invalidOptionsError("options must be an object");
  }
  const hasMaxOut =
    options.limits !== undefined &&
    typeof options.limits === "object" &&
    options.limits !== null &&
    (options.limits.maxOutputBytes !== undefined || options.limits.max_output_bytes !== undefined);
  const hasUnknownTags = options.unknownTags !== undefined || options.unknown_tags !== undefined;
  const validated = validateOptions(options);
  return {
    mergeKeys: validated.mergeKeys,
    unknownTags: hasUnknownTags ? validated.unknownTags : "reject",
    limits: {
      ...validated.limits,
      maxOutputBytes: hasMaxOut ? validated.limits.maxOutputBytes : DEFAULT_IJSON_MAX_OUTPUT_BYTES,
    },
  };
}

export function defaultEmitLimits(): EmitLimits {
  return {
    maxDepth: 128,
    maxNodes: 250_000,
    maxScalarBytes: 1 * 1024 * 1024,
    maxKeyBytes: 64 * 1024,
    maxMappingEntries: 100_000,
    maxTagBytes: DEFAULT_EMIT_MAX_TAG_BYTES,
    maxDocuments: 1_024,
    maxOutputBytes: DEFAULT_EMIT_MAX_OUTPUT_BYTES,
  };
}

export function defaultEmitOptions(): EmitOptions {
  return {
    compatQuoting: "yaml11_safe",
    limits: defaultEmitLimits(),
  };
}

const KNOWN_EMIT_OPTION_KEYS: ReadonlySet<string> = new Set([
  "compatQuoting",
  "compat_quoting",
  "limits",
]);

const KNOWN_EMIT_LIMIT_KEYS: ReadonlySet<string> = new Set([
  "maxDepth",
  "maxNodes",
  "maxScalarBytes",
  "maxKeyBytes",
  "maxMappingEntries",
  "maxTagBytes",
  "maxDocuments",
  "maxOutputBytes",
  "max_depth",
  "max_nodes",
  "max_scalar_bytes",
  "max_key_bytes",
  "max_mapping_entries",
  "max_tag_bytes",
  "max_documents",
  "max_output_bytes",
]);

function pickEmitLimit(
  input: EmitLimitsInput | undefined,
  camel: keyof EmitLimits,
  snake: keyof EmitLimitsInput,
  fallback: number,
): number {
  if (!input) return fallback;
  const c = input[camel];
  const s = input[snake];
  if (c !== undefined && s !== undefined && c !== s) {
    throw invalidOptionsError(`conflicting emit limit values for ${String(camel)}`);
  }
  const chosen = c !== undefined ? c : s !== undefined ? s : fallback;
  if (typeof chosen !== "number" || !Number.isInteger(chosen) || !Number.isSafeInteger(chosen)) {
    throw invalidOptionsError(`emit limit ${String(camel)} must be a safe integer`);
  }
  return chosen;
}

export function validateEmitOptions(options?: EmitOptionsInput): EmitOptions {
  if (options === undefined) {
    return defaultEmitOptions();
  }
  if (typeof options !== "object" || options === null || Array.isArray(options)) {
    throw invalidOptionsError("emit options must be an object");
  }
  for (const k of Object.keys(options)) {
    if (!KNOWN_EMIT_OPTION_KEYS.has(k)) {
      throw invalidOptionsError(`unknown emit option key: ${k}`);
    }
  }
  if (
    options.compatQuoting !== undefined &&
    options.compat_quoting !== undefined &&
    options.compatQuoting !== options.compat_quoting
  ) {
    throw invalidOptionsError("conflicting compatQuoting options");
  }
  const rawCq = options.compatQuoting ?? options.compat_quoting ?? "yaml11_safe";
  const compatQuoting: "yaml11_safe" | "core_only" =
    rawCq === "yaml11-safe" ? "yaml11_safe" : rawCq === "core-only" ? "core_only" : (rawCq as "yaml11_safe" | "core_only");
  if (compatQuoting !== "yaml11_safe" && compatQuoting !== "core_only") {
    throw invalidOptionsError("invalid compatQuoting option");
  }
  const limIn = options.limits;
  if (limIn !== undefined) {
    if (typeof limIn !== "object" || limIn === null || Array.isArray(limIn)) {
      throw invalidOptionsError("emit limits must be an object");
    }
    for (const k of Object.keys(limIn)) {
      if (!KNOWN_EMIT_LIMIT_KEYS.has(k)) {
        throw invalidOptionsError(`unknown emit limit key: ${k}`);
      }
    }
  }
  const def = defaultEmitLimits();
  const limits: EmitLimits = {
    maxDepth: pickEmitLimit(limIn, "maxDepth", "max_depth", def.maxDepth),
    maxNodes: pickEmitLimit(limIn, "maxNodes", "max_nodes", def.maxNodes),
    maxScalarBytes: pickEmitLimit(limIn, "maxScalarBytes", "max_scalar_bytes", def.maxScalarBytes),
    maxKeyBytes: pickEmitLimit(limIn, "maxKeyBytes", "max_key_bytes", def.maxKeyBytes),
    maxMappingEntries: pickEmitLimit(limIn, "maxMappingEntries", "max_mapping_entries", def.maxMappingEntries),
    maxTagBytes: pickEmitLimit(limIn, "maxTagBytes", "max_tag_bytes", def.maxTagBytes),
    maxDocuments: pickEmitLimit(limIn, "maxDocuments", "max_documents", def.maxDocuments),
    maxOutputBytes: pickEmitLimit(limIn, "maxOutputBytes", "max_output_bytes", def.maxOutputBytes),
  };
  const all = [
    limits.maxDepth,
    limits.maxNodes,
    limits.maxScalarBytes,
    limits.maxKeyBytes,
    limits.maxMappingEntries,
    limits.maxTagBytes,
    limits.maxDocuments,
    limits.maxOutputBytes,
  ];
  if (all.some((v) => v < 1)) {
    throw invalidOptionsError("every emit limit must be at least 1");
  }
  if (limits.maxDepth > 255) {
    throw invalidOptionsError("max_depth must be at most 255");
  }
  if (limits.maxScalarBytes > limits.maxOutputBytes || limits.maxKeyBytes > limits.maxOutputBytes) {
    throw invalidOptionsError("max_scalar_bytes and max_key_bytes must not exceed max_output_bytes");
  }
  return { compatQuoting, limits };
}

function serializeWasmEmitOptions(opts: EmitOptions): Uint8Array {
  const l = opts.limits;
  const json = `{"compat_quoting":"${opts.compatQuoting}","limits":{"max_depth":${clampU32(l.maxDepth)},"max_nodes":${clampU32(l.maxNodes)},"max_scalar_bytes":${clampU32(l.maxScalarBytes)},"max_key_bytes":${clampU32(l.maxKeyBytes)},"max_mapping_entries":${clampU32(l.maxMappingEntries)},"max_tag_bytes":${clampU32(l.maxTagBytes)},"max_documents":${clampU32(l.maxDocuments)},"max_output_bytes":${clampU32(l.maxOutputBytes)}}}`;
  return encoder.encode(json);
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

function serializeWasmOptions(opts: Options, alwaysSerialize = false): Uint8Array {
  if (!alwaysSerialize && isDefaultOptions(opts)) {
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
  load_ijson: (inputLen: number, optionsLen: number) => number;
  emit_wire: (inputLen: number, optionsLen: number) => number;
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
  mode: "wire-single" | "wire-all" | "plain" | "ijson",
  simulateTrap = false,
): { status: number; output: Uint8Array } {
  if (instanceBusy) {
    throw notSourceError("internal", {}, "reentrant WASM execution is forbidden");
  }
  const optsBytes = serializeWasmOptions(opts, mode === "ijson");
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
        : mode === "ijson"
          ? exports.load_ijson(inputBytes.byteLength, optsBytes.byteLength) >>> 0
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
      if (exports.memory.buffer.byteLength > WASM_HIGH_WATER_RECYCLE_BYTES) {
        activeInstance = null;
      }
    }
  }
}

function runWasmEmit(
  wireBytes: Uint8Array,
  opts: EmitOptions,
): { status: number; output: Uint8Array } {
  if (instanceBusy) {
    throw notSourceError("internal", {}, "reentrant WASM execution is forbidden");
  }
  const optsBytes = serializeWasmEmitOptions(opts);
  const exports = getWasmExports();
  instanceBusy = true;
  let localTrapped = false;
  try {
    const optPtr = exports.options_reserve(optsBytes.byteLength) >>> 0;
    if (optPtr === 0) {
      if (optsBytes.byteLength > MAX_OPTIONS_BYTES) {
        throw invalidOptionsError("emit options payload exceeds WASM options limit");
      }
      activeInstance = null;
      throw notSourceError("internal", {}, "WASM options reservation failed (linear memory ceiling exceeded)");
    }
    if (optsBytes.byteLength > 0) {
      new Uint8Array(exports.memory.buffer, optPtr, optsBytes.byteLength).set(optsBytes);
    }

    const inPtr = exports.input_reserve(wireBytes.byteLength, MAX_WASM_INPUT_CEILING) >>> 0;
    if (inPtr === 0) {
      if (wireBytes.byteLength > MAX_WASM_INPUT_CEILING) {
        throw notSourceError(
          "limit_exceeded",
          { limit: "max_input_bytes", configured: MAX_WASM_INPUT_CEILING },
          `WASM emission wire envelope exceeds ${MAX_WASM_INPUT_CEILING} bytes`,
        );
      }
      activeInstance = null;
      throw notSourceError("internal", {}, "WASM input reservation failed (linear memory ceiling exceeded)");
    }
    if (wireBytes.byteLength > 0) {
      new Uint8Array(exports.memory.buffer, inPtr, wireBytes.byteLength).set(wireBytes);
    }

    const status = exports.emit_wire(wireBytes.byteLength, optsBytes.byteLength) >>> 0;
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
    if (!localTrapped && !instanceTrapped && activeInstance !== null) {
      exports.release();
      releaseCallCount += 1;
      if (exports.memory.buffer.byteLength > WASM_HIGH_WATER_RECYCLE_BYTES) {
        activeInstance = null;
      }
    }
  }
}

function createWasmTestHarness(): {
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

const CORE_TAG_URIS: ReadonlySet<string> = new Set([
  "tag:yaml.org,2002:null",
  "tag:yaml.org,2002:bool",
  "tag:yaml.org,2002:int",
  "tag:yaml.org,2002:float",
  "tag:yaml.org,2002:str",
  "tag:yaml.org,2002:seq",
  "tag:yaml.org,2002:map",
]);

function isTagUriByte(b: number): boolean {
  return (
    (b >= 0x30 && b <= 0x39) ||
    (b >= 0x41 && b <= 0x5a) ||
    (b >= 0x61 && b <= 0x7a) ||
    b === 0x2d || // -
    b === 0x3b || // ;
    b === 0x2f || // /
    b === 0x3f || // ?
    b === 0x3a || // :
    b === 0x40 || // @
    b === 0x26 || // &
    b === 0x3d || // =
    b === 0x2b || // +
    b === 0x24 || // $
    b === 0x2c || // ,
    b === 0x5f || // _
    b === 0x2e || // .
    b === 0x21 || // !
    b === 0x7e || // ~
    b === 0x2a || // *
    b === 0x27 || // '
    b === 0x28 || // (
    b === 0x29 || // )
    b === 0x5b || // [
    b === 0x5d || // ]
    b === 0x23    // #
  );
}

function isValidTagChars(s: string): boolean {
  const bytes = encoder.encode(s);
  let i = 0;
  while (i < bytes.length) {
    const b = bytes[i]!;
    if (b === 0x25) {
      if (i + 2 >= bytes.length) return false;
      const h1 = bytes[i + 1]!;
      const h2 = bytes[i + 2]!;
      const isHex1 = (h1 >= 0x30 && h1 <= 0x39) || (h1 >= 0x41 && h1 <= 0x46) || (h1 >= 0x61 && h1 <= 0x66);
      const isHex2 = (h2 >= 0x30 && h2 <= 0x39) || (h2 >= 0x41 && h2 <= 0x46) || (h2 >= 0x61 && h2 <= 0x66);
      if (!isHex1 || !isHex2) return false;
      i += 3;
      continue;
    }
    if (b < 0x80) {
      if (!isTagUriByte(b)) return false;
      i += 1;
      continue;
    }
    i += 1;
  }
  for (const cp of s) {
    const code = cp.codePointAt(0)!;
    if (
      code <= 0x20 ||
      (code >= 0x7f && code <= 0x9f) ||
      code === 0xfeff ||
      code === 0xfffe ||
      code === 0xffff
    ) {
      return false;
    }
  }
  return true;
}

function isValidTagHandle(handle: string): boolean {
  if (handle === "" || handle === "!" || handle === "!!") return true;
  if (handle.length < 3 || !handle.startsWith("!") || !handle.endsWith("!")) return false;
  const inner = handle.slice(1, -1);
  for (let i = 0; i < inner.length; i++) {
    const c = inner.charCodeAt(i);
    const ok =
      (c >= 0x30 && c <= 0x39) ||
      (c >= 0x41 && c <= 0x5a) ||
      (c >= 0x61 && c <= 0x7a) ||
      c === 0x2d;
    if (!ok) return false;
  }
  return true;
}

function isValidNonCoreTagUri(uri: string): boolean {
  if (!uri || uri === "!" || CORE_TAG_URIS.has(uri)) return false;
  try {
    assertWellFormedUnicode(uri, "tag.uri");
  } catch {
    return false;
  }
  if (uri.startsWith("!")) {
    return uri.length >= 2;
  }
  return /^[A-Za-z][A-Za-z0-9+.-]*:/.test(uri);
}

function isConsistentTagSpelling(uri: string, handle: string, suffix: string): boolean {
  try {
    assertWellFormedUnicode(handle, "tag.handle");
    assertWellFormedUnicode(suffix, "tag.suffix");
  } catch {
    return false;
  }
  if (!isValidTagHandle(handle)) {
    return false;
  }
  if (handle === "") return suffix === uri;
  return suffix.length > 0 && uri.endsWith(suffix) && uri.length > suffix.length;
}

export function createTag(uri: string, handle = "", suffix?: string): Tag {
  if (typeof uri !== "string" || typeof handle !== "string") {
    throw invalidOptionsError("Tag fields must be strings");
  }
  const actualSuffix = suffix !== undefined ? suffix : handle === "" ? uri : "";
  if (typeof actualSuffix !== "string") {
    throw invalidOptionsError("Tag.suffix must be a string");
  }
  assertWellFormedUnicode(uri, "Tag.uri");
  assertWellFormedUnicode(handle, "Tag.handle");
  assertWellFormedUnicode(actualSuffix, "Tag.suffix");
  if (!isValidNonCoreTagUri(uri) || !isConsistentTagSpelling(uri, handle, actualSuffix)) {
    throw invalidOptionsError(`invalid Tag (${uri}, handle=${handle}, suffix=${actualSuffix})`);
  }
  return { uri, handle, suffix: actualSuffix };
}

export function createTaggedNode(
  tag: Tag | string,
  value: Node,
  metadata?: Partial<NodeMetadata>,
): TaggedNode {
  const tagObj =
    typeof tag === "string" ? createTag(tag) : createTag(tag.uri, tag.handle ?? "", tag.suffix ?? tag.uri);
  if (!value || typeof value !== "object" || !("kind" in value)) {
    throw invalidOptionsError("Tagged value must be a Node");
  }
  if (tagObj.uri === "tag:yaml.org,2002:merge") {
    if (value.kind !== "string") {
      throw notSourceError(
        "tag_kind_mismatch",
        {},
        "tag:yaml.org,2002:merge may only wrap a String node",
      );
    }
  } else if (value.kind !== "string" && value.kind !== "sequence" && value.kind !== "mapping") {
    throw notSourceError(
      "tag_kind_mismatch",
      {},
      `Tagged node must wrap string, sequence, or mapping (found ${value.kind})`,
    );
  }
  return {
    kind: "tagged",
    tag: tagObj,
    value,
    span: metadata?.span ?? null,
    via: metadata?.via ?? null,
    tagStart: metadata?.tagStart ?? null,
  };
}

export function createDocument(
  root: Node,
  yamlVersion: [number, number] | null = null,
  explicitStart = false,
): Document {
  if (!root || typeof root !== "object" || !("kind" in root)) {
    throw invalidOptionsError("Document.root must be a Node");
  }
  if (typeof explicitStart !== "boolean") {
    throw invalidOptionsError("Document.explicitStart must be a boolean");
  }
  if (yamlVersion !== null) {
    if (
      !Array.isArray(yamlVersion) ||
      yamlVersion.length !== 2 ||
      typeof yamlVersion[0] !== "number" ||
      typeof yamlVersion[1] !== "number" ||
      !Number.isInteger(yamlVersion[0]) ||
      !Number.isInteger(yamlVersion[1]) ||
      yamlVersion[0] !== 1 ||
      yamlVersion[1] < 0 ||
      yamlVersion[1] > 999_999_999
    ) {
      throw notSourceError(
        "unsupported_yaml_version",
        {},
        `unsupported YAML directive version: ${JSON.stringify(yamlVersion)}`,
      );
    }
    return { yamlVersion: [yamlVersion[0], yamlVersion[1]], explicitStart, root };
  }
  return { yamlVersion: null, explicitStart, root };
}

const MASK64 = 0xffffffffffffffffn;

function obtainCsprngKeys(): readonly [bigint, bigint] {
  const g = globalThis as { crypto?: { getRandomValues?: (arr: BigUint64Array) => void } };
  if (g.crypto === undefined || typeof g.crypto.getRandomValues !== "function") {
    throw notSourceError(
      "internal",
      {},
      "cryptographic random source (globalThis.crypto.getRandomValues) is unavailable",
    );
  }
  const buf = new BigUint64Array(2);
  g.crypto.getRandomValues(buf);
  return [buf[0]!, buf[1]!];
}

const SIP_KEYS: readonly [bigint, bigint] = obtainCsprngKeys();

function requireSipKeys(): readonly [bigint, bigint] {
  const g = globalThis as { crypto?: { getRandomValues?: (arr: BigUint64Array) => void } };
  if (g.crypto === undefined || typeof g.crypto.getRandomValues !== "function") {
    throw notSourceError(
      "internal",
      {},
      "cryptographic random source (globalThis.crypto.getRandomValues) is unavailable",
    );
  }
  return SIP_KEYS;
}

function rotl64(x: bigint, b: bigint): bigint {
  return ((x << b) | (x >> (64n - b))) & MASK64;
}

class SipHasher13 {
  private v0: bigint;
  private v1: bigint;
  private v2: bigint;
  private v3: bigint;

  constructor() {
    const [k0, k1] = requireSipKeys();
    this.v0 = (k0 ^ 0x736f6d6570736575n) & MASK64;
    this.v1 = (k1 ^ 0x646f72616e646f6dn) & MASK64;
    this.v2 = (k0 ^ 0x6c7967656e657261n) & MASK64;
    this.v3 = (k1 ^ 0x7465646279746573n) & MASK64;
  }

  private round(): void {
    this.v0 = (this.v0 + this.v1) & MASK64;
    this.v1 = rotl64(this.v1, 13n) ^ this.v0;
    this.v0 = rotl64(this.v0, 32n);
    this.v2 = (this.v2 + this.v3) & MASK64;
    this.v3 = rotl64(this.v3, 16n) ^ this.v2;
    this.v0 = (this.v0 + this.v3) & MASK64;
    this.v3 = rotl64(this.v3, 21n) ^ this.v0;
    this.v2 = (this.v2 + this.v1) & MASK64;
    this.v1 = rotl64(this.v1, 17n) ^ this.v2;
    this.v2 = rotl64(this.v2, 32n);
  }

  writeU64(m: bigint): void {
    const w = m & MASK64;
    this.v3 ^= w;
    this.round();
    this.v0 ^= w;
  }

  writeI128(v: bigint): void {
    const neg = v < 0n ? 1n : 0n;
    const mag = v < 0n ? -v : v;
    this.writeU64((3n << 56n) | neg);
    this.writeU64(mag & MASK64);
    this.writeU64((mag >> 64n) & MASK64);
  }

  writeFloat(v: number): void {
    if (Number.isNaN(v)) {
      this.writeU64((4n << 56n) | 1n);
      this.writeU64(0x7ff8000000000000n);
    } else {
      this.writeU64(4n << 56n);
      this.writeU64(normFloatBits(v));
    }
  }

  writeString(tag: bigint, s: string): void {
    const len = s.length;
    this.writeU64((tag << 56n) | BigInt(len));
    let i = 0;
    for (; i + 4 <= len; i += 4) {
      const w =
        BigInt(s.charCodeAt(i)) |
        (BigInt(s.charCodeAt(i + 1)) << 16n) |
        (BigInt(s.charCodeAt(i + 2)) << 32n) |
        (BigInt(s.charCodeAt(i + 3)) << 48n);
      this.writeU64(w);
    }
    if (i < len) {
      let rem = 0n;
      let shift = 0n;
      for (; i < len; i++) {
        rem |= BigInt(s.charCodeAt(i)) << shift;
        shift += 16n;
      }
      this.writeU64(rem);
    }
  }

  finish(): bigint {
    this.v2 ^= 0xffn;
    this.round();
    this.round();
    this.round();
    return (this.v0 ^ this.v1 ^ this.v2 ^ this.v3) & MASK64;
  }
}

function scalarKeyHash(n: Node): bigint | null {
  switch (n.kind) {
    case "null": {
      const h = new SipHasher13();
      h.writeU64(1n << 56n);
      return h.finish();
    }
    case "bool": {
      const h = new SipHasher13();
      h.writeU64((2n << 56n) | (n.value ? 1n : 0n));
      return h.finish();
    }
    case "int": {
      const h = new SipHasher13();
      h.writeI128(n.value);
      return h.finish();
    }
    case "float": {
      const h = new SipHasher13();
      h.writeFloat(n.value);
      return h.finish();
    }
    case "string": {
      const h = new SipHasher13();
      h.writeString(5n, n.value);
      return h.finish();
    }
    default:
      return null;
  }
}

function keyHash(root: Node): bigint {
  const fast = scalarKeyHash(root);
  if (fast !== null) return fast;

  type HashFrame =
    | { op: "eval"; node: Node }
    | { op: "finishTagged"; uri: string }
    | { op: "finishSeq"; count: number }
    | { op: "finishMap"; count: number };

  const work: HashFrame[] = [{ op: "eval", node: root }];
  const hashes: bigint[] = [];

  while (work.length > 0) {
    const frame = work.pop()!;
    if (frame.op === "finishTagged") {
      const innerH = hashes.pop()!;
      const h = new SipHasher13();
      h.writeString(6n, frame.uri);
      h.writeU64(innerH);
      hashes.push(h.finish());
      continue;
    }
    if (frame.op === "finishSeq") {
      const h = new SipHasher13();
      h.writeU64((7n << 56n) | BigInt(frame.count));
      const start = hashes.length - frame.count;
      for (let i = start; i < hashes.length; i++) {
        h.writeU64(hashes[i]!);
      }
      hashes.length = start;
      hashes.push(h.finish());
      continue;
    }
    if (frame.op === "finishMap") {
      const start = hashes.length - frame.count * 2;
      let sum = 0n;
      let xor = 0n;
      for (let i = 0; i < frame.count; i++) {
        const kh = hashes[start + i * 2]!;
        const vh = hashes[start + i * 2 + 1]!;
        const eh = new SipHasher13();
        eh.writeU64(8n << 56n);
        eh.writeU64(kh);
        eh.writeU64(vh);
        const d = eh.finish();
        sum = (sum + d) & MASK64;
        xor ^= d;
      }
      hashes.length = start;
      const mh = new SipHasher13();
      mh.writeU64((9n << 56n) | BigInt(frame.count));
      mh.writeU64(sum);
      mh.writeU64(xor);
      hashes.push(mh.finish());
      continue;
    }

    const n = frame.node;
    const sh = scalarKeyHash(n);
    if (sh !== null) {
      hashes.push(sh);
      continue;
    }
    switch (n.kind) {
      case "tagged":
        work.push({ op: "finishTagged", uri: n.tag.uri });
        work.push({ op: "eval", node: n.value });
        break;
      case "sequence":
        work.push({ op: "finishSeq", count: n.items.length });
        for (let i = n.items.length - 1; i >= 0; i--) {
          work.push({ op: "eval", node: n.items[i]! });
        }
        break;
      case "mapping":
        work.push({ op: "finishMap", count: n.entries.length });
        for (let i = n.entries.length - 1; i >= 0; i--) {
          const [k, v] = n.entries[i]!;
          work.push({ op: "eval", node: v });
          work.push({ op: "eval", node: k });
        }
        break;
    }
  }
  return hashes.pop()!;
}

class KeyIndex {
  private readonly buckets = new Map<bigint, Node[]>();

  checkAndInsert(key: Node): boolean {
    const h = keyHash(key);
    const bucket = this.buckets.get(h);
    if (bucket === undefined) {
      this.buckets.set(h, [key]);
      return true;
    }
    for (const existing of bucket) {
      if (keyEq(existing, key)) {
        return false;
      }
    }
    bucket.push(key);
    return true;
  }
}

export class Mapping {
  readonly entries: Array<[Node, Node]>;

  constructor(entries: ReadonlyArray<[Node, Node]> = []) {
    const validated: Array<[Node, Node]> = [];
    const keyIndex = new KeyIndex();
    for (const [k, v] of entries) {
      if (containsNan(k)) {
        throw notSourceError("invalid_key", {}, "mapping key contains NaN");
      }
      if (!keyIndex.checkAndInsert(k)) {
        throw notSourceError("duplicate_key", {}, "duplicate mapping key");
      }
      validated.push([k, v]);
    }
    this.entries = validated;
  }

  get size(): number {
    return this.entries.length;
  }

  get(key: string | Node): Node | undefined {
    return mappingGet(this.entries, key);
  }
}

export function keyEq(a: Node, b: Node): boolean {
  type KeyEqFrame =
    | { op: "cmp"; x: Node; y: Node }
    | {
        op: "mapColl";
        collEntries: ReadonlyArray<readonly [Node, Node]>;
        collIdx: number;
        ymEntries: ReadonlyArray<readonly [Node, Node]>;
        ymBuckets: Map<bigint, number[]>;
        used: Uint8Array;
      }
    | {
        op: "mapCand";
        collEntries: ReadonlyArray<readonly [Node, Node]>;
        collIdx: number;
        ymEntries: ReadonlyArray<readonly [Node, Node]>;
        ymBuckets: Map<bigint, number[]>;
        used: Uint8Array;
        cands: readonly number[];
        candIdx: number;
      };

  interface BacktrackEntry {
    savedLen: number;
    collEntries: ReadonlyArray<readonly [Node, Node]>;
    collIdx: number;
    ymEntries: ReadonlyArray<readonly [Node, Node]>;
    ymBuckets: Map<bigint, number[]>;
    used: Uint8Array;
    cands: readonly number[];
    nextCandIdx: number;
    prevJ: number;
  }

  const stack: KeyEqFrame[] = [{ op: "cmp", x: a, y: b }];
  const backtrackStack: BacktrackEntry[] = [];

  const backtrack = (): boolean => {
    while (backtrackStack.length > 0) {
      const bt = backtrackStack.pop()!;
      bt.used[bt.prevJ] = 0;
      stack.length = bt.savedLen;
      let idx = bt.nextCandIdx;
      while (idx < bt.cands.length && bt.used[bt.cands[idx]!]) {
        idx++;
      }
      if (idx < bt.cands.length) {
        stack.push({
          op: "mapCand",
          collEntries: bt.collEntries,
          collIdx: bt.collIdx,
          ymEntries: bt.ymEntries,
          ymBuckets: bt.ymBuckets,
          used: bt.used,
          cands: bt.cands,
          candIdx: idx,
        });
        return true;
      }
    }
    return false;
  };

  while (stack.length > 0) {
    const frame = stack.pop()!;
    if (frame.op === "mapColl") {
      if (frame.collIdx >= frame.collEntries.length) continue;
      const [xk] = frame.collEntries[frame.collIdx]!;
      const cands = frame.ymBuckets.get(keyHash(xk)) ?? [];
      let candIdx = 0;
      while (candIdx < cands.length && frame.used[cands[candIdx]!]) {
        candIdx++;
      }
      if (candIdx >= cands.length) {
        if (!backtrack()) return false;
        continue;
      }
      stack.push({
        op: "mapCand",
        collEntries: frame.collEntries,
        collIdx: frame.collIdx,
        ymEntries: frame.ymEntries,
        ymBuckets: frame.ymBuckets,
        used: frame.used,
        cands,
        candIdx,
      });
      continue;
    }

    if (frame.op === "mapCand") {
      let candIdx = frame.candIdx;
      while (candIdx < frame.cands.length && frame.used[frame.cands[candIdx]!]) {
        candIdx++;
      }
      if (candIdx >= frame.cands.length) {
        if (!backtrack()) return false;
        continue;
      }
      const j = frame.cands[candIdx]!;
      const [xk, xv] = frame.collEntries[frame.collIdx]!;
      const [yk, yv] = frame.ymEntries[j]!;
      let nextCandIdx = candIdx + 1;
      while (nextCandIdx < frame.cands.length && frame.used[frame.cands[nextCandIdx]!]) {
        nextCandIdx++;
      }
      if (nextCandIdx < frame.cands.length) {
        backtrackStack.push({
          savedLen: stack.length,
          collEntries: frame.collEntries,
          collIdx: frame.collIdx,
          ymEntries: frame.ymEntries,
          ymBuckets: frame.ymBuckets,
          used: frame.used,
          cands: frame.cands,
          nextCandIdx,
          prevJ: j,
        });
      }
      frame.used[j] = 1;
      if (frame.collIdx + 1 < frame.collEntries.length) {
        stack.push({
          op: "mapColl",
          collEntries: frame.collEntries,
          collIdx: frame.collIdx + 1,
          ymEntries: frame.ymEntries,
          ymBuckets: frame.ymBuckets,
          used: frame.used,
        });
      }
      stack.push({ op: "cmp", x: xv, y: yv });
      stack.push({ op: "cmp", x: xk, y: yk });
      continue;
    }

    const { x, y } = frame;
    if (x.kind !== y.kind) {
      if (!backtrack()) return false;
      continue;
    }
    switch (x.kind) {
      case "null":
        break;
      case "bool":
        if (x.value !== (y as BoolNode).value) {
          if (!backtrack()) return false;
        }
        break;
      case "int":
        if (x.value !== (y as IntNode).value) {
          if (!backtrack()) return false;
        }
        break;
      case "float": {
        const yf = y as FloatNode;
        if (Number.isNaN(x.value) || Number.isNaN(yf.value)) {
          if (!backtrack()) return false;
          break;
        }
        if (normFloatBits(x.value) !== normFloatBits(yf.value)) {
          if (!backtrack()) return false;
        }
        break;
      }
      case "string":
        if (x.value !== (y as StringNode).value) {
          if (!backtrack()) return false;
        }
        break;
      case "sequence": {
        const ys = y as SequenceNode;
        if (x.items.length !== ys.items.length) {
          if (!backtrack()) return false;
          break;
        }
        for (let i = 0; i < x.items.length; i++) {
          stack.push({ op: "cmp", x: x.items[i]!, y: ys.items[i]! });
        }
        break;
      }
      case "mapping": {
        const ym = y as MappingNode;
        if (x.entries.length !== ym.entries.length) {
          if (!backtrack()) return false;
          break;
        }
        const ymBuckets = new Map<bigint, number[]>();
        for (let j = 0; j < ym.entries.length; j++) {
          const [yk] = ym.entries[j]!;
          const h = keyHash(yk);
          const b = ymBuckets.get(h);
          if (b === undefined) {
            ymBuckets.set(h, [j]);
          } else {
            b.push(j);
          }
        }
        const used = new Uint8Array(ym.entries.length);
        const collEntries: Array<readonly [Node, Node]> = [];
        let failed = false;
        for (const [xk, xv] of x.entries) {
          const cands = ymBuckets.get(keyHash(xk));
          if (!cands || cands.length === 0) {
            failed = true;
            break;
          }
          if (cands.length === 1) {
            const j = cands[0]!;
            if (used[j]) {
              failed = true;
              break;
            }
            used[j] = 1;
            const [yk, yv] = ym.entries[j]!;
            stack.push({ op: "cmp", x: xv, y: yv });
            stack.push({ op: "cmp", x: xk, y: yk });
          } else {
            collEntries.push([xk, xv]);
          }
        }
        if (failed) {
          if (!backtrack()) return false;
          break;
        }
        if (collEntries.length > 0) {
          stack.push({
            op: "mapColl",
            collEntries,
            collIdx: 0,
            ymEntries: ym.entries,
            ymBuckets,
            used,
          });
        }
        break;
      }
      case "tagged": {
        const yt = y as TaggedNode;
        if (x.tag.uri !== yt.tag.uri) {
          if (!backtrack()) return false;
          break;
        }
        stack.push({ op: "cmp", x: x.value, y: yt.value });
        break;
      }
    }
  }
  return true;
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
  ownedBytes: bigint;
  forbidLocations: boolean;
}

function chargeOwnedBytes(ctx: DecodeContext, delta: bigint): void {
  ctx.ownedBytes += delta;
  if (ctx.ownedBytes > BigInt(ctx.opts.limits.maxOwnedBytes)) {
    throw notSourceError(
      "limit_exceeded",
      { limit: "max_owned_bytes", configured: ctx.opts.limits.maxOwnedBytes },
      `decoded tree exceeds max_owned_bytes (${ctx.opts.limits.maxOwnedBytes})`,
    );
  }
}

function preflightDocKeyWeight(keyRoot: Node, maxKeyBytes: number): void {
  const maxKey = BigInt(maxKeyBytes);
  let total = 0n;
  const stack: Node[] = [keyRoot];
  while (stack.length > 0) {
    const cur = stack.pop()!;
    total += NODE_OVERHEAD;
    if (total > maxKey) {
      throw notSourceError(
        "limit_exceeded",
        { limit: "max_key_bytes", configured: maxKeyBytes },
        `decoded mapping key exceeds max_key_bytes (${maxKeyBytes})`,
      );
    }
    if (cur.kind === "string") {
      total += BigInt(encoder.encode(cur.value).byteLength);
      if (total > maxKey) {
        throw notSourceError(
          "limit_exceeded",
          { limit: "max_key_bytes", configured: maxKeyBytes },
          `decoded mapping key exceeds max_key_bytes (${maxKeyBytes})`,
        );
      }
    } else if (cur.kind === "tagged") {
      total += tagWeightBytes(cur.tag);
      if (total > maxKey) {
        throw notSourceError(
          "limit_exceeded",
          { limit: "max_key_bytes", configured: maxKeyBytes },
          `decoded mapping key exceeds max_key_bytes (${maxKeyBytes})`,
        );
      }
      stack.push(cur.value);
    } else if (cur.kind === "sequence") {
      for (const it of cur.items) stack.push(it);
    } else if (cur.kind === "mapping") {
      total += BigInt(cur.entries.length) * ENTRY_OVERHEAD;
      if (total > maxKey) {
        throw notSourceError(
          "limit_exceeded",
          { limit: "max_key_bytes", configured: maxKeyBytes },
          `decoded mapping key exceeds max_key_bytes (${maxKeyBytes})`,
        );
      }
      for (const [k, v] of cur.entries) {
        stack.push(v, k);
      }
    }
  }
}

function decodeNode(rootVal: JsonVal, ctx: DecodeContext, rootDepth: number, rootInKey = false): Node {
  type DecodeFrame =
    | { op: "eval"; val: JsonVal; depth: number; inKey: boolean }
    | {
        op: "seqStep";
        rawItems: readonly JsonVal[];
        idx: number;
        nextDepth: number;
        inKey: boolean;
        items: Node[];
        span: Span | null;
        via: Via | null;
        tagStart: Location | null;
      }
    | {
        op: "mapStep";
        rawEntries: readonly JsonVal[];
        idx: number;
        nextDepth: number;
        inKey: boolean;
        entries: Array<[Node, Node]>;
        keyIndex: KeyIndex;
        span: Span | null;
        via: Via | null;
        tagStart: Location | null;
      }
    | {
        op: "mapEntryDone";
        rawEntries: readonly JsonVal[];
        nextIdx: number;
        nextDepth: number;
        inKey: boolean;
        entries: Array<[Node, Node]>;
        keyIndex: KeyIndex;
        span: Span | null;
        via: Via | null;
        tagStart: Location | null;
      }
    | {
        op: "finishTagged";
        tag: Tag;
        inKey: boolean;
        span: Span | null;
        via: Via | null;
        tagStart: Location | null;
      };

  const work: DecodeFrame[] = [{ op: "eval", val: rootVal, depth: rootDepth, inKey: rootInKey }];
  const results: Node[] = [];

  while (work.length > 0) {
    const frame = work.pop()!;
    if (frame.op === "seqStep") {
      if (frame.idx > 0) {
        frame.items.push(results.pop()!);
      }
      if (frame.idx >= frame.rawItems.length) {
        results.push({
          kind: "sequence",
          items: frame.items,
          span: frame.span,
          via: frame.via,
          tagStart: frame.tagStart,
        });
      } else {
        work.push({
          op: "seqStep",
          rawItems: frame.rawItems,
          idx: frame.idx + 1,
          nextDepth: frame.nextDepth,
          inKey: frame.inKey,
          items: frame.items,
          span: frame.span,
          via: frame.via,
          tagStart: frame.tagStart,
        });
        work.push({
          op: "eval",
          val: frame.rawItems[frame.idx]!,
          depth: frame.nextDepth,
          inKey: frame.inKey,
        });
      }
      continue;
    }

    if (frame.op === "mapStep") {
      if (frame.idx >= frame.rawEntries.length) {
        results.push({
          kind: "mapping",
          entries: frame.entries,
          span: frame.span,
          via: frame.via,
          tagStart: frame.tagStart,
        });
      } else {
        const item = frame.rawEntries[frame.idx]!;
        if (item.t !== "obj") {
          throw notSourceError("internal", {}, "mapping entry must be an object");
        }
        expectExactKeys(item.map, ["key", "value"]);
        work.push({
          op: "mapEntryDone",
          rawEntries: frame.rawEntries,
          nextIdx: frame.idx + 1,
          nextDepth: frame.nextDepth,
          inKey: frame.inKey,
          entries: frame.entries,
          keyIndex: frame.keyIndex,
          span: frame.span,
          via: frame.via,
          tagStart: frame.tagStart,
        });
        work.push({
          op: "eval",
          val: item.map.get("value")!,
          depth: frame.nextDepth,
          inKey: frame.inKey,
        });
        work.push({
          op: "eval",
          val: item.map.get("key")!,
          depth: frame.nextDepth,
          inKey: true,
        });
      }
      continue;
    }

    if (frame.op === "mapEntryDone") {
      const v = results.pop()!;
      const k = results.pop()!;
      if (!ctx.forbidLocations) {
        preflightDocKeyWeight(k, ctx.opts.limits.maxKeyBytes);
      }
      if (containsNan(k)) {
        throw notSourceError("internal", {}, "wire mapping key contains NaN");
      }
      if (!frame.keyIndex.checkAndInsert(k)) {
        throw notSourceError("internal", {}, "wire mapping contains duplicate key");
      }
      frame.entries.push([k, v]);
      work.push({
        op: "mapStep",
        rawEntries: frame.rawEntries,
        idx: frame.nextIdx,
        nextDepth: frame.nextDepth,
        inKey: frame.inKey,
        entries: frame.entries,
        keyIndex: frame.keyIndex,
        span: frame.span,
        via: frame.via,
        tagStart: frame.tagStart,
      });
      continue;
    }

    if (frame.op === "finishTagged") {
      const inner = results.pop()!;
      if (frame.tag.uri === "tag:yaml.org,2002:merge") {
        if (frame.inKey || inner.kind !== "string") {
          throw notSourceError(
            "internal",
            {},
            "tag:yaml.org,2002:merge may only wrap a non-key String node in yamlprims-value/1",
          );
        }
      } else if (inner.kind !== "string" && inner.kind !== "sequence" && inner.kind !== "mapping") {
        throw notSourceError(
          "internal",
          {},
          "Tagged node in yamlprims-value/1 must wrap string, sequence, or mapping",
        );
      }
      results.push({
        kind: "tagged",
        tag: frame.tag,
        value: inner,
        span: frame.span,
        via: frame.via,
        tagStart: frame.tagStart,
      });
      continue;
    }

    const { val, depth, inKey } = frame;
    if (val.t !== "obj") {
      throw notSourceError("internal", {}, "Node must be a JSON object");
    }
    if (ctx.nodesRemaining <= 0) {
      throw notSourceError(
        "limit_exceeded",
        { limit: "max_nodes", configured: ctx.opts.limits.maxNodes },
        `wire Node count exceeds max_nodes (${ctx.opts.limits.maxNodes})`,
      );
    }
    ctx.nodesRemaining -= 1;
    chargeOwnedBytes(ctx, NODE_OVERHEAD);

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
        results.push({ kind: "null", span, via, tagStart });
        break;
      }
      case "bool": {
        expectExactKeys(map, ["kind", "value", "span", "via", "tag_start"]);
        results.push({
          kind: "bool",
          value: expectBool(map.get("value"), "bool.value"),
          span,
          via,
          tagStart,
        });
        break;
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
        results.push({ kind: "int", value: big, span, via, tagStart });
        break;
      }
      case "float": {
        expectExactKeys(map, ["kind", "bits", "span", "via", "tag_start"]);
        const rawBits = expectStr(map.get("bits"), "float.bits");
        if (!/^[0-9a-f]{16}$/.test(rawBits)) {
          throw notSourceError("internal", {}, `invalid wire float bits: ${rawBits}`);
        }
        const { value, canonicalBits } = bitsToFloat(rawBits);
        results.push({ kind: "float", value, bits: canonicalBits, span, via, tagStart });
        break;
      }
      case "string": {
        expectExactKeys(map, ["kind", "value", "span", "via", "tag_start"]);
        const strVal = expectStr(map.get("value"), "string.value");
        const strBytes = encoder.encode(strVal).byteLength;
        if (strBytes > ctx.opts.limits.maxScalarBytes) {
          throw notSourceError(
            "limit_exceeded",
            { limit: "max_scalar_bytes", configured: ctx.opts.limits.maxScalarBytes },
            `wire string exceeds max_scalar_bytes (${ctx.opts.limits.maxScalarBytes})`,
          );
        }
        chargeOwnedBytes(ctx, BigInt(strBytes));
        results.push({ kind: "string", value: strVal, span, via, tagStart });
        break;
      }
      case "sequence": {
        expectExactKeys(map, ["kind", "items", "span", "via", "tag_start"]);
        const itemsVal = map.get("items")!;
        if (itemsVal.t !== "arr") {
          throw notSourceError("internal", {}, "sequence.items must be an array");
        }
        if (!ctx.forbidLocations || itemsVal.items.length > 0) {
          if (depth + 1 > ctx.opts.limits.maxDepth) {
            throw notSourceError(
              "limit_exceeded",
              { limit: "max_depth", configured: ctx.opts.limits.maxDepth },
              `wire sequence depth exceeds max_depth (${ctx.opts.limits.maxDepth})`,
            );
          }
        }
        work.push({
          op: "seqStep",
          rawItems: itemsVal.items,
          idx: 0,
          nextDepth: depth + 1,
          inKey,
          items: [],
          span,
          via,
          tagStart,
        });
        break;
      }
      case "mapping": {
        expectExactKeys(map, ["kind", "entries", "span", "via", "tag_start"]);
        const entriesVal = map.get("entries")!;
        if (entriesVal.t !== "arr") {
          throw notSourceError("internal", {}, "mapping.entries must be an array");
        }
        if (!ctx.forbidLocations || entriesVal.items.length > 0) {
          if (depth + 1 > ctx.opts.limits.maxDepth) {
            throw notSourceError(
              "limit_exceeded",
              { limit: "max_depth", configured: ctx.opts.limits.maxDepth },
              `wire mapping depth exceeds max_depth (${ctx.opts.limits.maxDepth})`,
            );
          }
        }
        if (entriesVal.items.length > ctx.opts.limits.maxMappingEntries) {
          throw notSourceError(
            "limit_exceeded",
            { limit: "max_mapping_entries", configured: ctx.opts.limits.maxMappingEntries },
            `wire mapping entries exceed max_mapping_entries (${ctx.opts.limits.maxMappingEntries})`,
          );
        }
        chargeOwnedBytes(ctx, BigInt(entriesVal.items.length) * ENTRY_OVERHEAD);
        work.push({
          op: "mapStep",
          rawEntries: entriesVal.items,
          idx: 0,
          nextDepth: depth + 1,
          inKey,
          entries: [],
          keyIndex: new KeyIndex(),
          span,
          via,
          tagStart,
        });
        break;
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
        if (!isValidNonCoreTagUri(uri) || !isConsistentTagSpelling(uri, handle, suffix)) {
          throw notSourceError("internal", {}, "invalid wire Tag on tagged node");
        }
        chargeOwnedBytes(ctx, tagWeightBytes({ uri, handle, suffix }));
        work.push({
          op: "finishTagged",
          tag: { uri, handle, suffix },
          inKey,
          span,
          via,
          tagStart,
        });
        work.push({ op: "eval", val: map.get("value")!, depth, inKey });
        break;
      }
      default:
        throw notSourceError("internal", {}, `unknown wire Node kind: ${kind}`);
    }
  }
  return results.pop()!;
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
    throw notSourceError(
      "limit_exceeded",
      { limit: "max_depth", configured: opts.limits.maxDepth },
      `error.path length exceeds max_depth (${opts.limits.maxDepth})`,
    );
  }
  const pathCtx: DecodeContext = {
    opts,
    idx: null,
    nodesRemaining: opts.limits.maxNodes,
    ownedBytes: 0n,
    forbidLocations: true,
  };
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
      const keyNode = decodeNode(keySeg, pathCtx, 0, true);
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
  if (wireByteLen > opts.limits.maxInputBytes) {
    throw notSourceError(
      "limit_exceeded",
      { limit: "max_input_bytes", configured: opts.limits.maxInputBytes },
      `wire input exceeds max_input_bytes (${opts.limits.maxInputBytes})`,
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
      throw notSourceError(
        "limit_exceeded",
        { limit: "max_documents", configured: opts.limits.maxDocuments },
        `wire document count exceeds max_documents (${opts.limits.maxDocuments})`,
      );
    }
    const idx = sourceBytes !== undefined ? new Utf16Index(sourceBytes) : null;
    const ctx: DecodeContext = {
      opts,
      idx,
      nodesRemaining: opts.limits.maxNodes,
      ownedBytes: 0n,
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
        if (major !== 1 || minor > 999_999_999) {
          throw notSourceError("internal", {}, "unsupported yaml_version in wire Document");
        }
        yamlVersion = [major, minor];
      }
      const explicitStart = expectBool(d.map.get("explicit_start"), "explicit_start");
      const rootNode = decodeNode(d.map.get("root")!, ctx, 0, false);
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

function finishNodeLocations(w: BoundedWriter, n: Node, withSpans: boolean): void {
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

type WriteFrame =
  | { op: "raw"; text: string }
  | { op: "finishLoc"; node: Node }
  | { op: "leaveObj"; obj: object }
  | { op: "node"; node: Node; depth: number; finishLoc: boolean }
  | { op: "seq"; items: readonly Node[]; idx: number; depth: number; owner: object }
  | {
      op: "map";
      entries: ReadonlyArray<readonly [Node, Node]>;
      idx: number;
      depth: number;
      owner: object;
      keyIndex: KeyIndex;
    };

function runWriteNodeFrames(
  w: BoundedWriter,
  rootNode: Node,
  withSpans: boolean,
  st: EncodeState,
  rootDepth: number,
  finishRootLocations: boolean,
): void {
  const stack: WriteFrame[] = [
    { op: "node", node: rootNode, depth: rootDepth, finishLoc: finishRootLocations },
  ];
  while (stack.length > 0) {
    const frame = stack.pop()!;
    switch (frame.op) {
      case "raw":
        w.pushRaw(frame.text);
        break;
      case "finishLoc":
        finishNodeLocations(w, frame.node, withSpans);
        break;
      case "leaveObj":
        st.activeAncestors.delete(frame.obj);
        break;
      case "seq": {
        if (frame.idx >= frame.items.length) {
          w.pushRaw("]");
          st.activeAncestors.delete(frame.owner);
        } else {
          if (frame.idx > 0) w.pushRaw(",");
          stack.push({
            op: "seq",
            items: frame.items,
            idx: frame.idx + 1,
            depth: frame.depth,
            owner: frame.owner,
          });
          stack.push({
            op: "node",
            node: frame.items[frame.idx]!,
            depth: frame.depth,
            finishLoc: true,
          });
        }
        break;
      }
      case "map": {
        if (frame.idx >= frame.entries.length) {
          w.pushRaw("]");
          st.activeAncestors.delete(frame.owner);
        } else {
          const [k, v] = frame.entries[frame.idx]!;
          if (containsNan(k)) {
            st.activeAncestors.delete(frame.owner);
            throw notSourceError("invalid_key", {}, "mapping key contains NaN");
          }
          if (!frame.keyIndex.checkAndInsert(k)) {
            st.activeAncestors.delete(frame.owner);
            throw notSourceError("duplicate_key", {}, "duplicate mapping key");
          }
          if (frame.idx > 0) w.pushRaw(",");
          w.pushRaw('{"key":');
          stack.push({
            op: "map",
            entries: frame.entries,
            idx: frame.idx + 1,
            depth: frame.depth,
            owner: frame.owner,
            keyIndex: frame.keyIndex,
          });
          stack.push({ op: "raw", text: "}" });
          stack.push({ op: "node", node: v, depth: frame.depth, finishLoc: true });
          stack.push({ op: "raw", text: ',"value":' });
          stack.push({ op: "node", node: k, depth: frame.depth, finishLoc: true });
        }
        break;
      }
      case "node": {
        const n = frame.node;
        const depth = frame.depth;
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
        if (frame.finishLoc) {
          stack.push({ op: "finishLoc", node: n });
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
            stack.push({
              op: "seq",
              items: n.items,
              idx: 0,
              depth: depth + 1,
              owner: n,
            });
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
            stack.push({
              op: "map",
              entries: n.entries,
              idx: 0,
              depth: depth + 1,
              owner: n,
              keyIndex: new KeyIndex(),
            });
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
            stack.push({ op: "leaveObj", obj: n });
            stack.push({ op: "node", node: n.value, depth, finishLoc: true });
            break;
          }
        }
        break;
      }
    }
  }
}

function writeNodeValue(
  w: BoundedWriter,
  n: Node,
  withSpans: boolean,
  st: EncodeState,
  depth: number,
): void {
  runWriteNodeFrames(w, n, withSpans, st, depth, false);
}

function writeNode(
  w: BoundedWriter,
  n: Node,
  withSpans: boolean,
  st: EncodeState,
  depth: number,
): void {
  runWriteNodeFrames(w, n, withSpans, st, depth, true);
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

  const pathOpts: Options = {
    mergeKeys: opts.mergeKeys,
    unknownTags: opts.unknownTags,
    limits: {
      ...opts.limits,
      maxDepth: 255,
      maxNodes: MAX_SAFE_METADATA_INT,
    },
  };
  const st: EncodeState = {
    opts: pathOpts,
    nodesRemaining: MAX_SAFE_METADATA_INT,
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

  type ToPlainFrame =
    | { op: "eval"; node: Node; depth: number }
    | { op: "seqStep"; items: readonly Node[]; idx: number; nextDepth: number; owner: object; out: unknown[] }
    | {
        op: "mapStep";
        entries: ReadonlyArray<readonly [Node, Node]>;
        idx: number;
        nextDepth: number;
        owner: object;
        obj: Record<string, unknown>;
        pendingKey: string | null;
      };

  const work: ToPlainFrame[] = [{ op: "eval", node: root, depth: 0 }];
  const results: unknown[] = [];

  while (work.length > 0) {
    const frame = work.pop()!;
    if (frame.op === "seqStep") {
      if (frame.idx > 0) {
        frame.out.push(results.pop());
      }
      if (frame.idx >= frame.items.length) {
        active.delete(frame.owner);
        results.push(frame.out);
      } else {
        work.push({
          op: "seqStep",
          items: frame.items,
          idx: frame.idx + 1,
          nextDepth: frame.nextDepth,
          owner: frame.owner,
          out: frame.out,
        });
        work.push({ op: "eval", node: frame.items[frame.idx]!, depth: frame.nextDepth });
      }
      continue;
    }

    if (frame.op === "mapStep") {
      if (frame.pendingKey !== null) {
        frame.obj[frame.pendingKey] = results.pop();
      }
      if (frame.idx >= frame.entries.length) {
        active.delete(frame.owner);
        results.push(frame.obj);
      } else {
        const [k, v] = frame.entries[frame.idx]!;
        if (k.kind !== "string") {
          active.delete(frame.owner);
          throw lossError(k, "non-string key");
        }
        try {
          assertWellFormedUnicode(k.value, "mapping key");
        } catch (err) {
          active.delete(frame.owner);
          throw err;
        }
        if (Object.prototype.hasOwnProperty.call(frame.obj, k.value)) {
          active.delete(frame.owner);
          throw notSourceError("duplicate_key", {}, "duplicate mapping key");
        }
        work.push({
          op: "mapStep",
          entries: frame.entries,
          idx: frame.idx + 1,
          nextDepth: frame.nextDepth,
          owner: frame.owner,
          obj: frame.obj,
          pendingKey: k.value,
        });
        work.push({ op: "eval", node: v, depth: frame.nextDepth });
      }
      continue;
    }

    const { node: n, depth } = frame;
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
        results.push(null);
        break;
      case "bool":
        results.push(n.value);
        break;
      case "int": {
        const maxSafe = BigInt(MAX_SAFE_METADATA_INT);
        if (n.value < -maxSafe || n.value > maxSafe) {
          throw lossError(n, "integer outside safe integer range");
        }
        results.push(n.value === 0n ? 0 : Number(n.value));
        break;
      }
      case "float":
        results.push(n.value);
        break;
      case "string":
        assertWellFormedUnicode(n.value, "string node");
        results.push(n.value);
        break;
      case "sequence": {
        if (depth + 1 > opts.limits.maxDepth) {
          throw notSourceError(
            "limit_exceeded",
            { limit: "max_depth", configured: opts.limits.maxDepth },
            "sequence nesting exceeds max_depth",
          );
        }
        active.add(n);
        work.push({
          op: "seqStep",
          items: n.items,
          idx: 0,
          nextDepth: depth + 1,
          owner: n,
          out: [],
        });
        break;
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
        work.push({
          op: "mapStep",
          entries: n.entries,
          idx: 0,
          nextDepth: depth + 1,
          owner: n,
          obj: Object.create(null) as Record<string, unknown>,
          pendingKey: null,
        });
        break;
      }
      case "tagged":
        throw lossError(n, "tagged value");
    }
  }

  return results.pop();
}

function formatPlainFloat(f: number): string {
  if (f === 0) {
    return Object.is(f, -0) ? "-0.0" : "0.0";
  }
  const prefix = f < 0 ? "-" : "";
  const s = String(Math.abs(f));
  let digits: string;
  let exp: number;
  const eIdx = s.search(/e/i);
  if (eIdx !== -1) {
    const mant = s.slice(0, eIdx);
    exp = Number.parseInt(s.slice(eIdx + 1), 10);
    digits = mant.replace(".", "");
  } else if (s.includes(".")) {
    if (s.startsWith("0.")) {
      let z = 0;
      while (2 + z < s.length && s.charCodeAt(2 + z) === 0x30) {
        z++;
      }
      digits = s.slice(2 + z);
      exp = -(z + 1);
    } else {
      const dotIdx = s.indexOf(".");
      digits = s.slice(0, dotIdx) + s.slice(dotIdx + 1);
      exp = dotIdx - 1;
    }
  } else {
    digits = s.replace(/0+$/, "");
    exp = s.length - 1;
  }
  if (exp >= -4 && exp < 16) {
    if (exp >= 0) {
      const intDigits = exp + 1;
      if (digits.length <= intDigits) {
        return prefix + digits + "0".repeat(intDigits - digits.length) + ".0";
      }
      return prefix + digits.slice(0, intDigits) + "." + digits.slice(intDigits);
    }
    return prefix + "0." + "0".repeat(-exp - 1) + digits;
  }
  const mant = digits.length === 1 ? digits : `${digits[0]}.${digits.slice(1)}`;
  return `${prefix}${mant}e${exp}`;
}

export function encodePlain(nodeOrDoc: Node | Document, options?: OptionsInput): string {
  const opts = validateOptions(options);
  const root = isDocument(nodeOrDoc) ? nodeOrDoc.root : nodeOrDoc;
  const w = new BoundedWriter(opts.limits.maxOutputBytes);
  let nodesRemaining = opts.limits.maxNodes;
  const active = new Set<object>();

  type EncodePlainFrame =
    | { op: "eval"; node: Node; depth: number }
    | { op: "seqStep"; items: readonly Node[]; idx: number; nextDepth: number; owner: object }
    | { op: "mapStep"; entries: ReadonlyArray<readonly [Node, Node]>; idx: number; nextDepth: number; owner: object };

  const stack: EncodePlainFrame[] = [{ op: "eval", node: root, depth: 0 }];

  while (stack.length > 0) {
    const frame = stack.pop()!;
    if (frame.op === "seqStep") {
      if (frame.idx >= frame.items.length) {
        w.pushRaw("]");
        active.delete(frame.owner);
      } else {
        if (frame.idx > 0) w.pushRaw(",");
        stack.push({
          op: "seqStep",
          items: frame.items,
          idx: frame.idx + 1,
          nextDepth: frame.nextDepth,
          owner: frame.owner,
        });
        stack.push({ op: "eval", node: frame.items[frame.idx]!, depth: frame.nextDepth });
      }
      continue;
    }

    if (frame.op === "mapStep") {
      if (frame.idx >= frame.entries.length) {
        w.pushRaw("}");
        active.delete(frame.owner);
      } else {
        const [k, v] = frame.entries[frame.idx]!;
        if (k.kind !== "string") {
          active.delete(frame.owner);
          throw lossError(k, "non-string key");
        }
        if (frame.idx > 0) w.pushRaw(",");
        w.pushString(k.value);
        w.pushRaw(":");
        stack.push({
          op: "mapStep",
          entries: frame.entries,
          idx: frame.idx + 1,
          nextDepth: frame.nextDepth,
          owner: frame.owner,
        });
        stack.push({ op: "eval", node: v, depth: frame.nextDepth });
      }
      continue;
    }

    const { node: n, depth } = frame;
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
        stack.push({
          op: "seqStep",
          items: n.items,
          idx: 0,
          nextDepth: depth + 1,
          owner: n,
        });
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
        stack.push({
          op: "mapStep",
          entries: n.entries,
          idx: 0,
          nextDepth: depth + 1,
          owner: n,
        });
        break;
      case "tagged":
        throw lossError(n, "tagged value");
    }
  }

  return w.finish();
}

// ---- Primary entrypoints -------------------------------------------------

function widenBridgeOptions(opts: Options, payloadByteLen: number): Options {
  const lim = opts.limits;
  const bridgeNodes = Math.max(lim.maxNodes, lim.maxExpandedNodes, 250_000);
  const bridgeOwned = Math.max(
    lim.maxOwnedBytes,
    Math.min(MAX_SAFE_METADATA_INT, payloadByteLen * 4 + 65_536),
  );
  const bridgeScalar = Math.min(bridgeOwned, Math.max(lim.maxScalarBytes, payloadByteLen));
  const bridgeKey = Math.min(bridgeOwned, Math.max(lim.maxKeyBytes, payloadByteLen));
  return {
    mergeKeys: opts.mergeKeys,
    unknownTags: opts.unknownTags,
    limits: {
      maxInputBytes: Math.max(lim.maxInputBytes, lim.maxOutputBytes, payloadByteLen, 1),
      maxDepth: 255,
      maxNodes: bridgeNodes,
      maxExpandedNodes: Math.max(lim.maxExpandedNodes, bridgeNodes),
      maxAliasEvents: lim.maxAliasEvents,
      maxOwnedBytes: bridgeOwned,
      maxScalarBytes: bridgeScalar,
      maxKeyBytes: bridgeKey,
      maxMappingEntries: Math.max(lim.maxMappingEntries, 250_000),
      maxDocuments: Math.max(lim.maxDocuments, 1_024),
      maxOutputBytes: Math.max(lim.maxOutputBytes, payloadByteLen, 1),
      maxWarnings: lim.maxWarnings,
    },
  };
}

export function loadAll(input: string | Uint8Array, options?: OptionsInput): Document[] {
  const opts = validateOptions(options);
  const inputBytes = toInputBytes(input);
  const { status, output } = runWasmLoad(inputBytes, opts, "wire-all");
  if (status === 2) {
    throw decodeFallbackStatus2(output, opts.limits.maxOutputBytes);
  }
  const wireText = decoder.decode(output);
  const decoded = decodeWire(wireText, widenBridgeOptions(opts, output.byteLength), inputBytes);
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
  const decoded = decodeWire(wireText, widenBridgeOptions(opts, output.byteLength), inputBytes);
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
    const decoded = decodeWire(text, widenBridgeOptions(opts, output.byteLength), inputBytes);
    if (decoded.kind === "error") {
      throw decoded.error;
    }
    throw notSourceError("internal", {}, "expected error envelope from load_plain status 1");
  }
  return text;
}

function toIjsonInputBytes(input: string | Uint8Array): Uint8Array {
  if (typeof input === "string") {
    for (let i = 0; i < input.length; i++) {
      const code = input.charCodeAt(i);
      if (code >= 0xd800 && code <= 0xdbff) {
        const next = i + 1 < input.length ? input.charCodeAt(i + 1) : 0;
        if (next < 0xdc00 || next > 0xdfff) {
          throw notSourceError("invalid_utf8", { invalid_len: null }, "input string contains an unpaired high surrogate");
        }
        i++;
      } else if (code >= 0xdc00 && code <= 0xdfff) {
        throw notSourceError("invalid_utf8", { invalid_len: null }, "input string contains an unpaired low surrogate");
      }
    }
    return encoder.encode(input);
  }
  if (input instanceof Uint8Array) {
    return input;
  }
  throw new TypeError("input must be a string or Uint8Array");
}

export function loadIjson(input: string | Uint8Array, options?: OptionsInput): string {
  const opts = validateIjsonOptions(options);
  const inputBytes = toIjsonInputBytes(input);
  const { status, output } = runWasmLoad(inputBytes, opts, "ijson");
  if (status === 2) {
    throw decodeFallbackStatus2(output, opts.limits.maxOutputBytes);
  }
  const text = decoder.decode(output);
  if (status === 1) {
    const decoded = decodeWire(text, widenBridgeOptions(opts, output.byteLength), inputBytes);
    if (decoded.kind === "error") {
      throw decoded.error;
    }
    throw notSourceError("internal", {}, "expected error envelope from load_ijson status 1");
  }
  return text;
}

// ---- Host emission & fromPlain (`spec-v1.5 §10.2`, `§11.3`, `§11.5`) -----

function validateUtf8Str(s: unknown): Uint8Array {
  if (typeof s !== "string") {
    throw notSourceError(
      "type",
      { expected: "string", found: typeof s },
      `expected string, found ${typeof s}`,
    );
  }
  for (let i = 0; i < s.length; i++) {
    const code = s.charCodeAt(i);
    if (code >= 0xd800 && code <= 0xdbff) {
      const next = i + 1 < s.length ? s.charCodeAt(i + 1) : 0;
      if (next < 0xdc00 || next > 0xdfff) {
        throw notSourceError("invalid_utf8", { invalid_len: null }, "string contains unpaired high surrogate");
      }
      i++;
    } else if (code >= 0xdc00 && code <= 0xdfff) {
      throw notSourceError("invalid_utf8", { invalid_len: null }, "string contains unpaired low surrogate");
    }
  }
  return encoder.encode(s);
}

function renderedTagMinBytes(uriBytes: Uint8Array, uri: string): bigint {
  const n = BigInt(uriBytes.byteLength);
  if (uri.startsWith("tag:yaml.org,2002:") && n > 18n) {
    return 2n + (n - 18n);
  }
  return n;
}

const NODE_META_KEYS = new Set(["kind", "span", "via", "tagStart"]);

function hasOwn(obj: object, prop: string): boolean {
  return Object.prototype.hasOwnProperty.call(obj, prop);
}

function isNodeObject(v: unknown): v is Node {
  if (!v || typeof v !== "object" || Array.isArray(v) || v instanceof Mapping || v instanceof Map) {
    return false;
  }
  const obj = v as Record<string, unknown>;
  if (!hasOwn(obj, "kind") || !hasOwn(obj, "span") || !hasOwn(obj, "via") || !hasOwn(obj, "tagStart")) {
    return false;
  }
  const k = obj.kind;
  if (typeof k !== "string") return false;
  const keys = Object.keys(obj);
  const checkKeys = (allowedExtra: readonly string[]): boolean => {
    for (const key of keys) {
      if (!NODE_META_KEYS.has(key) && !allowedExtra.includes(key)) {
        return false;
      }
    }
    return true;
  };
  switch (k) {
    case "null":
      return checkKeys(["value"]) && (!hasOwn(obj, "value") || obj.value === null);
    case "bool":
    case "int":
    case "string":
      return hasOwn(obj, "value") && checkKeys(["value"]);
    case "float":
      return hasOwn(obj, "value") && checkKeys(["value", "bits"]);
    case "sequence":
      return hasOwn(obj, "items") && Array.isArray(obj.items) && checkKeys(["items"]);
    case "mapping":
      return hasOwn(obj, "entries") && Array.isArray(obj.entries) && checkKeys(["entries"]);
    case "tagged":
      return hasOwn(obj, "tag") && hasOwn(obj, "value") && checkKeys(["tag", "value"]);
    default:
      return false;
  }
}

function isPlainObject(v: unknown): v is Record<string, unknown> {
  if (!v || typeof v !== "object") return false;
  const proto = Object.getPrototypeOf(v);
  return proto === Object.prototype || proto === null;
}

function checkEmitKeyWeight(rawKey: unknown, limits: EmitLimits): void {
  const maxKey = BigInt(limits.maxKeyBytes);
  let total = 0n;

  type KeyWeightFrame =
    | { op: "eval"; cur: unknown; ancestors: Set<object> }
    | { op: "seqStep"; items: readonly unknown[]; idx: number; checkHoles: boolean; ancestors: Set<object> }
    | { op: "mapPairsStep"; pairs: ReadonlyArray<readonly [unknown, unknown]>; idx: number; ancestors: Set<object> }
    | { op: "mapIterStep"; iter: Iterator<[unknown, unknown]>; ancestors: Set<object> }
    | { op: "objKeysStep"; obj: Record<string, unknown>; keys: readonly string[]; idx: number; ancestors: Set<object> };

  const stack: KeyWeightFrame[] = [{ op: "eval", cur: rawKey, ancestors: new Set() }];
  while (stack.length > 0) {
    const frame = stack.pop()!;
    if (frame.op === "seqStep") {
      if (frame.idx < frame.items.length) {
        if (frame.checkHoles && !(frame.idx in frame.items)) {
          throw notSourceError(
            "type",
            { expected: "YAML-serializable value", found: "undefined" },
            "sparse array holes (undefined) are not YAML-serializable",
          );
        }
        stack.push({
          op: "seqStep",
          items: frame.items,
          idx: frame.idx + 1,
          checkHoles: frame.checkHoles,
          ancestors: frame.ancestors,
        });
        stack.push({ op: "eval", cur: frame.items[frame.idx], ancestors: frame.ancestors });
      }
      continue;
    }
    if (frame.op === "mapPairsStep") {
      if (frame.idx < frame.pairs.length) {
        const [k, v] = frame.pairs[frame.idx]!;
        stack.push({
          op: "mapPairsStep",
          pairs: frame.pairs,
          idx: frame.idx + 1,
          ancestors: frame.ancestors,
        });
        stack.push({ op: "eval", cur: v, ancestors: frame.ancestors });
        stack.push({ op: "eval", cur: k, ancestors: frame.ancestors });
      }
      continue;
    }
    if (frame.op === "mapIterStep") {
      const next = frame.iter.next();
      if (!next.done) {
        const [k, v] = next.value;
        stack.push({ op: "mapIterStep", iter: frame.iter, ancestors: frame.ancestors });
        stack.push({ op: "eval", cur: v, ancestors: frame.ancestors });
        stack.push({ op: "eval", cur: k, ancestors: frame.ancestors });
      }
      continue;
    }
    if (frame.op === "objKeysStep") {
      if (frame.idx < frame.keys.length) {
        const k = frame.keys[frame.idx]!;
        const v = frame.obj[k];
        stack.push({
          op: "objKeysStep",
          obj: frame.obj,
          keys: frame.keys,
          idx: frame.idx + 1,
          ancestors: frame.ancestors,
        });
        stack.push({ op: "eval", cur: v, ancestors: frame.ancestors });
        stack.push({ op: "eval", cur: k, ancestors: frame.ancestors });
      }
      continue;
    }

    const { cur, ancestors } = frame;
    if (isNodeObject(cur)) {
      if (cur.kind === "string") {
        const sBytes = validateUtf8Str(cur.value);
        total += NODE_OVERHEAD + BigInt(sBytes.byteLength);
        if (total > maxKey) {
          throw notSourceError(
            "limit_exceeded",
            { limit: "max_key_bytes", configured: limits.maxKeyBytes },
            `mapping key exceeds max_key_bytes (${limits.maxKeyBytes})`,
          );
        }
        continue;
      }
      if (cur.kind === "tagged") {
        if (!cur.tag || typeof cur.tag !== "object") {
          throw invalidOptionsError("Tagged node must hold a Tag object");
        }
        const uriBytes = validateUtf8Str(cur.tag.uri);
        total += NODE_OVERHEAD + 2n * BigInt(uriBytes.byteLength);
        if (total > maxKey) {
          throw notSourceError(
            "limit_exceeded",
            { limit: "max_key_bytes", configured: limits.maxKeyBytes },
            `mapping key exceeds max_key_bytes (${limits.maxKeyBytes})`,
          );
        }
        if (ancestors.has(cur)) {
          throw notSourceError(
            "limit_exceeded",
            { limit: "max_depth", configured: limits.maxDepth },
            `cyclic mapping key exceeds max_depth (${limits.maxDepth})`,
          );
        }
        const nextAnc = new Set(ancestors);
        nextAnc.add(cur);
        stack.push({ op: "eval", cur: cur.value, ancestors: nextAnc });
        continue;
      }
      if (cur.kind === "null" || cur.kind === "bool" || cur.kind === "int" || cur.kind === "float") {
        total += NODE_OVERHEAD;
        if (total > maxKey) {
          throw notSourceError(
            "limit_exceeded",
            { limit: "max_key_bytes", configured: limits.maxKeyBytes },
            `mapping key exceeds max_key_bytes (${limits.maxKeyBytes})`,
          );
        }
        continue;
      }
      if (cur.kind === "sequence") {
        total += NODE_OVERHEAD;
        if (total > maxKey) {
          throw notSourceError(
            "limit_exceeded",
            { limit: "max_key_bytes", configured: limits.maxKeyBytes },
            `mapping key exceeds max_key_bytes (${limits.maxKeyBytes})`,
          );
        }
        if (ancestors.has(cur) || ancestors.has(cur.items)) {
          throw notSourceError(
            "limit_exceeded",
            { limit: "max_depth", configured: limits.maxDepth },
            `cyclic mapping key exceeds max_depth (${limits.maxDepth})`,
          );
        }
        const nextAnc = new Set(ancestors);
        nextAnc.add(cur);
        nextAnc.add(cur.items);
        stack.push({ op: "seqStep", items: cur.items, idx: 0, checkHoles: false, ancestors: nextAnc });
        continue;
      }
      if (cur.kind === "mapping") {
        total += NODE_OVERHEAD + BigInt(cur.entries.length) * ENTRY_OVERHEAD;
        if (total > maxKey) {
          throw notSourceError(
            "limit_exceeded",
            { limit: "max_key_bytes", configured: limits.maxKeyBytes },
            `mapping key exceeds max_key_bytes (${limits.maxKeyBytes})`,
          );
        }
        if (ancestors.has(cur) || ancestors.has(cur.entries)) {
          throw notSourceError(
            "limit_exceeded",
            { limit: "max_depth", configured: limits.maxDepth },
            `cyclic mapping key exceeds max_depth (${limits.maxDepth})`,
          );
        }
        const nextAnc = new Set(ancestors);
        nextAnc.add(cur);
        nextAnc.add(cur.entries);
        stack.push({ op: "mapPairsStep", pairs: cur.entries, idx: 0, ancestors: nextAnc });
        continue;
      }
    }
    if (typeof cur === "string") {
      const sBytes = validateUtf8Str(cur);
      total += NODE_OVERHEAD + BigInt(sBytes.byteLength);
      if (total > maxKey) {
        throw notSourceError(
          "limit_exceeded",
          { limit: "max_key_bytes", configured: limits.maxKeyBytes },
          `mapping key exceeds max_key_bytes (${limits.maxKeyBytes})`,
        );
      }
      continue;
    }
    if (cur === null || typeof cur === "boolean" || typeof cur === "bigint" || typeof cur === "number") {
      total += NODE_OVERHEAD;
      if (total > maxKey) {
        throw notSourceError(
          "limit_exceeded",
          { limit: "max_key_bytes", configured: limits.maxKeyBytes },
          `mapping key exceeds max_key_bytes (${limits.maxKeyBytes})`,
        );
      }
      continue;
    }
    if (Array.isArray(cur)) {
      total += NODE_OVERHEAD;
      if (total > maxKey) {
        throw notSourceError(
          "limit_exceeded",
          { limit: "max_key_bytes", configured: limits.maxKeyBytes },
          `mapping key exceeds max_key_bytes (${limits.maxKeyBytes})`,
        );
      }
      if (ancestors.has(cur)) {
        throw notSourceError(
          "limit_exceeded",
          { limit: "max_depth", configured: limits.maxDepth },
          `cyclic mapping key exceeds max_depth (${limits.maxDepth})`,
        );
      }
      const nextAnc = new Set(ancestors);
      nextAnc.add(cur);
      stack.push({ op: "seqStep", items: cur, idx: 0, checkHoles: true, ancestors: nextAnc });
      continue;
    }
    if (cur instanceof Mapping) {
      total += NODE_OVERHEAD + BigInt(cur.entries.length) * ENTRY_OVERHEAD;
      if (total > maxKey) {
        throw notSourceError(
          "limit_exceeded",
          { limit: "max_key_bytes", configured: limits.maxKeyBytes },
          `mapping key exceeds max_key_bytes (${limits.maxKeyBytes})`,
        );
      }
      if (ancestors.has(cur)) {
        throw notSourceError(
          "limit_exceeded",
          { limit: "max_depth", configured: limits.maxDepth },
          `cyclic mapping key exceeds max_depth (${limits.maxDepth})`,
        );
      }
      const nextAnc = new Set(ancestors);
      nextAnc.add(cur);
      stack.push({ op: "mapPairsStep", pairs: cur.entries, idx: 0, ancestors: nextAnc });
      continue;
    }
    if (cur instanceof Map) {
      total += NODE_OVERHEAD + BigInt(cur.size) * ENTRY_OVERHEAD;
      if (total > maxKey) {
        throw notSourceError(
          "limit_exceeded",
          { limit: "max_key_bytes", configured: limits.maxKeyBytes },
          `mapping key exceeds max_key_bytes (${limits.maxKeyBytes})`,
        );
      }
      if (ancestors.has(cur)) {
        throw notSourceError(
          "limit_exceeded",
          { limit: "max_depth", configured: limits.maxDepth },
          `cyclic mapping key exceeds max_depth (${limits.maxDepth})`,
        );
      }
      const nextAnc = new Set(ancestors);
      nextAnc.add(cur);
      stack.push({ op: "mapIterStep", iter: cur.entries(), ancestors: nextAnc });
      continue;
    }
    if (isPlainObject(cur)) {
      const keys = Object.keys(cur);
      total += NODE_OVERHEAD + BigInt(keys.length) * ENTRY_OVERHEAD;
      if (total > maxKey) {
        throw notSourceError(
          "limit_exceeded",
          { limit: "max_key_bytes", configured: limits.maxKeyBytes },
          `mapping key exceeds max_key_bytes (${limits.maxKeyBytes})`,
        );
      }
      if (ancestors.has(cur)) {
        throw notSourceError(
          "limit_exceeded",
          { limit: "max_depth", configured: limits.maxDepth },
          `cyclic mapping key exceeds max_depth (${limits.maxDepth})`,
        );
      }
      const nextAnc = new Set(ancestors);
      nextAnc.add(cur);
      stack.push({ op: "objKeysStep", obj: cur, keys, idx: 0, ancestors: nextAnc });
      continue;
    }
    throw notSourceError(
      "type",
      { expected: "YAML-serializable value", found: typeof cur },
      `unsupported type for YAML emission: ${typeof cur}`,
    );
  }
}

interface EmitPreflightState {
  opts: EmitOptions;
  nodesVisited: number;
  minOutputBytes: bigint;
  ownedBytes: bigint;
}

type PreflightWorkFrame =
  | { op: "eval"; raw: unknown; depth: number; inKey: boolean; isKeyRoot: boolean }
  | { op: "finishTagged"; uri: string; owner: object }
  | {
      op: "seqStep";
      rawItems: readonly unknown[];
      idx: number;
      nextDepth: number;
      inKey: boolean;
      checkHoles: boolean;
      owner1: object | null;
      owner2: object;
      items: Node[];
    }
  | {
      op: "mapIter";
      rawPairs: ReadonlyArray<readonly [unknown, unknown]>;
      idx: number;
      nextDepth: number;
      inKey: boolean;
      owner1: object | null;
      owner2: object;
      entries: Array<[Node, Node]>;
      keyIndex: KeyIndex;
    }
  | {
      op: "mapEntryDone";
      rawPairs: ReadonlyArray<readonly [unknown, unknown]>;
      nextIdx: number;
      nextDepth: number;
      inKey: boolean;
      owner1: object | null;
      owner2: object;
      entries: Array<[Node, Node]>;
      keyIndex: KeyIndex;
    };

function convertAndPreflightNode(
  rootRaw: unknown,
  opts: EmitOptions,
  state: EmitPreflightState,
): Node {
  const limits = opts.limits;
  const active = new Set<object>();
  const work: PreflightWorkFrame[] = [
    { op: "eval", raw: rootRaw, depth: 0, inKey: false, isKeyRoot: false },
  ];
  const results: Node[] = [];

  while (work.length > 0) {
    const frame = work.pop()!;
    switch (frame.op) {
      case "finishTagged": {
        active.delete(frame.owner);
        const cleanInner = results.pop()!;
        results.push({
          kind: "tagged",
          tag: { uri: frame.uri, handle: "", suffix: frame.uri },
          value: cleanInner,
          span: null,
          via: null,
          tagStart: null,
        });
        break;
      }
      case "seqStep": {
        if (frame.idx > 0) {
          frame.items.push(results.pop()!);
        }
        if (frame.idx >= frame.rawItems.length) {
          active.delete(frame.owner2);
          if (frame.owner1 !== null) active.delete(frame.owner1);
          results.push({ kind: "sequence", items: frame.items, span: null, via: null, tagStart: null });
        } else {
          if (frame.checkHoles && !(frame.idx in frame.rawItems)) {
            throw notSourceError(
              "type",
              { expected: "YAML-serializable value", found: "undefined" },
              "sparse array holes (undefined) are not YAML-serializable",
            );
          }
          work.push({
            op: "seqStep",
            rawItems: frame.rawItems,
            idx: frame.idx + 1,
            nextDepth: frame.nextDepth,
            inKey: frame.inKey,
            checkHoles: frame.checkHoles,
            owner1: frame.owner1,
            owner2: frame.owner2,
            items: frame.items,
          });
          work.push({
            op: "eval",
            raw: frame.rawItems[frame.idx],
            depth: frame.nextDepth,
            inKey: frame.inKey,
            isKeyRoot: false,
          });
        }
        break;
      }
      case "mapIter": {
        if (frame.idx >= frame.rawPairs.length) {
          active.delete(frame.owner2);
          if (frame.owner1 !== null) active.delete(frame.owner1);
          results.push({ kind: "mapping", entries: frame.entries, span: null, via: null, tagStart: null });
        } else {
          const [rawK, rawV] = frame.rawPairs[frame.idx]!;
          work.push({
            op: "mapEntryDone",
            rawPairs: frame.rawPairs,
            nextIdx: frame.idx + 1,
            nextDepth: frame.nextDepth,
            inKey: frame.inKey,
            owner1: frame.owner1,
            owner2: frame.owner2,
            entries: frame.entries,
            keyIndex: frame.keyIndex,
          });
          work.push({ op: "eval", raw: rawV, depth: frame.nextDepth, inKey: frame.inKey, isKeyRoot: false });
          work.push({ op: "eval", raw: rawK, depth: frame.nextDepth, inKey: true, isKeyRoot: true });
        }
        break;
      }
      case "mapEntryDone": {
        const vNode = results.pop()!;
        const kNode = results.pop()!;
        if (!frame.keyIndex.checkAndInsert(kNode)) {
          throw notSourceError("duplicate_key", {}, "duplicate mapping key");
        }
        frame.entries.push([kNode, vNode]);
        work.push({
          op: "mapIter",
          rawPairs: frame.rawPairs,
          idx: frame.nextIdx,
          nextDepth: frame.nextDepth,
          inKey: frame.inKey,
          owner1: frame.owner1,
          owner2: frame.owner2,
          entries: frame.entries,
          keyIndex: frame.keyIndex,
        });
        break;
      }
      case "eval": {
        const { raw, depth, inKey, isKeyRoot } = frame;
        state.nodesVisited += 1;
        if (state.nodesVisited > limits.maxNodes) {
          throw notSourceError(
            "limit_exceeded",
            { limit: "max_nodes", configured: limits.maxNodes },
            `node count exceeds max_nodes (${limits.maxNodes})`,
          );
        }
        state.ownedBytes += NODE_OVERHEAD;

        if (isKeyRoot) {
          checkEmitKeyWeight(raw, limits);
        }

        if (isNodeObject(raw)) {
          if (active.has(raw)) {
            throw notSourceError(
              "limit_exceeded",
              { limit: "max_depth", configured: limits.maxDepth },
              `cyclic node graph exceeds max_depth (${limits.maxDepth})`,
            );
          }
          switch (raw.kind) {
            case "null":
              results.push({ kind: "null", span: null, via: null, tagStart: null });
              break;
            case "bool":
              if (typeof raw.value !== "boolean") {
                throw notSourceError("type", { expected: "boolean", found: typeof raw.value }, "expected boolean");
              }
              results.push({ kind: "bool", value: raw.value, span: null, via: null, tagStart: null });
              break;
            case "int":
              if (typeof raw.value !== "bigint") {
                throw notSourceError("type", { expected: "bigint", found: typeof raw.value }, "expected bigint");
              }
              if (raw.value < I128_MIN || raw.value > U128_MAX) {
                throw notSourceError("integer_out_of_range", {}, "integer outside 128-bit range");
              }
              results.push({ kind: "int", value: raw.value, span: null, via: null, tagStart: null });
              break;
            case "float": {
              if (typeof raw.value !== "number") {
                throw notSourceError("type", { expected: "number", found: typeof raw.value }, "expected number");
              }
              const rawBits = raw.bits !== undefined ? raw.bits : floatToBits(raw.value);
              if (typeof rawBits !== "string" || !/^[0-9a-f]{16}$/.test(rawBits)) {
                throw notSourceError("internal", {}, "invalid float bits on Node");
              }
              const { value, canonicalBits } = bitsToFloat(rawBits);
              if (Number.isNaN(value) || Number.isNaN(raw.value)) {
                if (inKey) {
                  throw notSourceError("invalid_key", {}, "NaN float is not valid inside a mapping key");
                }
                results.push({
                  kind: "float",
                  value: Number.NaN,
                  bits: CANONICAL_NAN_BITS,
                  span: null,
                  via: null,
                  tagStart: null,
                });
              } else {
                results.push({ kind: "float", value, bits: canonicalBits, span: null, via: null, tagStart: null });
              }
              break;
            }
            case "string": {
              const sBytes = validateUtf8Str(raw.value);
              if (sBytes.byteLength > limits.maxScalarBytes) {
                throw notSourceError(
                  "limit_exceeded",
                  { limit: "max_scalar_bytes", configured: limits.maxScalarBytes },
                  `scalar byte length exceeds max_scalar_bytes (${limits.maxScalarBytes})`,
                );
              }
              state.minOutputBytes += BigInt(sBytes.byteLength);
              state.ownedBytes += BigInt(sBytes.byteLength);
              results.push({ kind: "string", value: raw.value, span: null, via: null, tagStart: null });
              break;
            }
            case "tagged": {
              if (!raw.tag || typeof raw.tag !== "object") {
                throw invalidOptionsError("Tagged node must hold a Tag object");
              }
              const uriBytes = validateUtf8Str(raw.tag.uri);
              validateUtf8Str(raw.tag.handle ?? "");
              validateUtf8Str(raw.tag.suffix ?? raw.tag.uri);
              if (uriBytes.byteLength > limits.maxTagBytes || !isValidNonCoreTagUri(raw.tag.uri)) {
                throw invalidOptionsError("Tagged node contains invalid or oversized Tag.uri");
              }
              const innerRaw = raw.value;
              const innerKind = isNodeObject(innerRaw) ? innerRaw.kind : null;
              if (raw.tag.uri === "tag:yaml.org,2002:merge") {
                if (inKey || innerKind !== "string") {
                  throw notSourceError(
                    "tag_kind_mismatch",
                    {},
                    "tag:yaml.org,2002:merge may only wrap a non-key String node",
                  );
                }
              } else if (innerKind !== "string" && innerKind !== "sequence" && innerKind !== "mapping") {
                throw notSourceError(
                  "tag_kind_mismatch",
                  {},
                  "Tagged node must wrap String, Sequence, or Mapping",
                );
              }
              state.minOutputBytes += renderedTagMinBytes(uriBytes, raw.tag.uri);
              state.ownedBytes += 2n * BigInt(uriBytes.byteLength);
              active.add(raw);
              work.push({ op: "finishTagged", uri: raw.tag.uri, owner: raw });
              work.push({ op: "eval", raw: innerRaw, depth, inKey, isKeyRoot: false });
              break;
            }
            case "sequence": {
              const nextDepth = depth + 1;
              if (nextDepth > limits.maxDepth) {
                throw notSourceError(
                  "limit_exceeded",
                  { limit: "max_depth", configured: limits.maxDepth },
                  `collection nesting exceeds max_depth (${limits.maxDepth})`,
                );
              }
              if (active.has(raw.items)) {
                throw notSourceError(
                  "limit_exceeded",
                  { limit: "max_depth", configured: limits.maxDepth },
                  `cyclic sequence exceeds max_depth (${limits.maxDepth})`,
                );
              }
              active.add(raw);
              active.add(raw.items);
              work.push({
                op: "seqStep",
                rawItems: raw.items,
                idx: 0,
                nextDepth,
                inKey,
                checkHoles: false,
                owner1: raw,
                owner2: raw.items,
                items: [],
              });
              break;
            }
            case "mapping": {
              const nextDepth = depth + 1;
              if (nextDepth > limits.maxDepth) {
                throw notSourceError(
                  "limit_exceeded",
                  { limit: "max_depth", configured: limits.maxDepth },
                  `collection nesting exceeds max_depth (${limits.maxDepth})`,
                );
              }
              if (raw.entries.length > limits.maxMappingEntries) {
                throw notSourceError(
                  "limit_exceeded",
                  { limit: "max_mapping_entries", configured: limits.maxMappingEntries },
                  `mapping entry count exceeds max_mapping_entries (${limits.maxMappingEntries})`,
                );
              }
              if (active.has(raw.entries)) {
                throw notSourceError(
                  "limit_exceeded",
                  { limit: "max_depth", configured: limits.maxDepth },
                  `cyclic mapping exceeds max_depth (${limits.maxDepth})`,
                );
              }
              state.ownedBytes += BigInt(raw.entries.length) * ENTRY_OVERHEAD;
              active.add(raw);
              active.add(raw.entries);
              work.push({
                op: "mapIter",
                rawPairs: raw.entries,
                idx: 0,
                nextDepth,
                inKey,
                owner1: raw,
                owner2: raw.entries,
                entries: [],
                keyIndex: new KeyIndex(),
              });
              break;
            }
          }
          break;
        }

        if (raw === null) {
          results.push({ kind: "null", span: null, via: null, tagStart: null });
          break;
        }
        if (typeof raw === "boolean") {
          results.push({ kind: "bool", value: raw, span: null, via: null, tagStart: null });
          break;
        }
        if (typeof raw === "bigint") {
          if (raw < I128_MIN || raw > U128_MAX) {
            throw notSourceError("integer_out_of_range", {}, "bigint outside 128-bit range");
          }
          results.push({ kind: "int", value: raw, span: null, via: null, tagStart: null });
          break;
        }
        if (typeof raw === "number") {
          if (Number.isSafeInteger(raw) && !Object.is(raw, -0)) {
            results.push({ kind: "int", value: BigInt(raw), span: null, via: null, tagStart: null });
          } else if (Number.isNaN(raw)) {
            if (inKey) {
              throw notSourceError("invalid_key", {}, "NaN float is not valid inside a mapping key");
            }
            results.push({
              kind: "float",
              value: Number.NaN,
              bits: CANONICAL_NAN_BITS,
              span: null,
              via: null,
              tagStart: null,
            });
          } else {
            results.push({ kind: "float", value: raw, bits: floatToBits(raw), span: null, via: null, tagStart: null });
          }
          break;
        }
        if (typeof raw === "string") {
          const sBytes = validateUtf8Str(raw);
          if (sBytes.byteLength > limits.maxScalarBytes) {
            throw notSourceError(
              "limit_exceeded",
              { limit: "max_scalar_bytes", configured: limits.maxScalarBytes },
              `scalar byte length exceeds max_scalar_bytes (${limits.maxScalarBytes})`,
            );
          }
          state.minOutputBytes += BigInt(sBytes.byteLength);
          state.ownedBytes += BigInt(sBytes.byteLength);
          results.push({ kind: "string", value: raw, span: null, via: null, tagStart: null });
          break;
        }
        if (Array.isArray(raw)) {
          const nextDepth = depth + 1;
          if (nextDepth > limits.maxDepth) {
            throw notSourceError(
              "limit_exceeded",
              { limit: "max_depth", configured: limits.maxDepth },
              `collection nesting exceeds max_depth (${limits.maxDepth})`,
            );
          }
          if (active.has(raw)) {
            throw notSourceError(
              "limit_exceeded",
              { limit: "max_depth", configured: limits.maxDepth },
              `cyclic sequence exceeds max_depth (${limits.maxDepth})`,
            );
          }
          active.add(raw);
          work.push({
            op: "seqStep",
            rawItems: raw,
            idx: 0,
            nextDepth,
            inKey,
            checkHoles: true,
            owner1: null,
            owner2: raw,
            items: [],
          });
          break;
        }
        if (raw instanceof Mapping || raw instanceof Map || isPlainObject(raw)) {
          const nextDepth = depth + 1;
          if (nextDepth > limits.maxDepth) {
            throw notSourceError(
              "limit_exceeded",
              { limit: "max_depth", configured: limits.maxDepth },
              `collection nesting exceeds max_depth (${limits.maxDepth})`,
            );
          }
          if (active.has(raw)) {
            throw notSourceError(
              "limit_exceeded",
              { limit: "max_depth", configured: limits.maxDepth },
              `cyclic mapping exceeds max_depth (${limits.maxDepth})`,
            );
          }
          let rawPairs: Array<[unknown, unknown]>;
          if (raw instanceof Mapping) {
            if (raw.entries.length > limits.maxMappingEntries) {
              throw notSourceError(
                "limit_exceeded",
                { limit: "max_mapping_entries", configured: limits.maxMappingEntries },
                `mapping entry count exceeds max_mapping_entries (${limits.maxMappingEntries})`,
              );
            }
            rawPairs = raw.entries;
          } else if (raw instanceof Map) {
            if (raw.size > limits.maxMappingEntries) {
              throw notSourceError(
                "limit_exceeded",
                { limit: "max_mapping_entries", configured: limits.maxMappingEntries },
                `mapping entry count exceeds max_mapping_entries (${limits.maxMappingEntries})`,
              );
            }
            rawPairs = Array.from(raw.entries());
          } else {
            const keys = Object.keys(raw);
            if (keys.length > limits.maxMappingEntries) {
              throw notSourceError(
                "limit_exceeded",
                { limit: "max_mapping_entries", configured: limits.maxMappingEntries },
                `mapping entry count exceeds max_mapping_entries (${limits.maxMappingEntries})`,
              );
            }
            rawPairs = keys.map((k) => [k, curVal(raw, k)]);
          }
          state.ownedBytes += BigInt(rawPairs.length) * ENTRY_OVERHEAD;
          active.add(raw);
          work.push({
            op: "mapIter",
            rawPairs,
            idx: 0,
            nextDepth,
            inKey,
            owner1: null,
            owner2: raw,
            entries: [],
            keyIndex: new KeyIndex(),
          });
          break;
        }

        throw notSourceError(
          "type",
          {
            expected: "YAML-serializable value",
            found:
              raw === undefined
                ? "undefined"
                : typeof raw === "object"
                  ? (raw.constructor?.name ?? "object")
                  : typeof raw,
          },
          `unsupported type for YAML emission: ${raw === undefined ? "undefined" : typeof raw}`,
        );
      }
    }
  }

  return results.pop()!;
}

function curVal(obj: Record<string, unknown>, k: string): unknown {
  return obj[k];
}

export function fromPlain(value: unknown, options?: EmitOptionsInput): Node {
  const opts = validateEmitOptions(options);
  const state: EmitPreflightState = {
    opts,
    nodesVisited: 0,
    minOutputBytes: 0n,
    ownedBytes: 0n,
  };
  return convertAndPreflightNode(value, opts, state);
}

function isDocumentObject(val: unknown): val is Document {
  if (!val || typeof val !== "object" || Array.isArray(val) || val instanceof Mapping || val instanceof Map) {
    return false;
  }
  const obj = val as Record<string, unknown>;
  const keys = Object.keys(obj);
  if (keys.length !== 3) return false;
  for (const k of keys) {
    if (k !== "root" && k !== "explicitStart" && k !== "yamlVersion") {
      return false;
    }
  }
  return (
    hasOwn(obj, "root") &&
    hasOwn(obj, "explicitStart") &&
    hasOwn(obj, "yamlVersion") &&
    isNodeObject(obj.root)
  );
}

export function dumpAll(documents: readonly unknown[], options?: EmitOptionsInput): string {
  const opts = validateEmitOptions(options);
  if (!Array.isArray(documents)) {
    throw notSourceError(
      "type",
      { expected: "array of documents", found: typeof documents },
      "dumpAll expects an array of documents",
    );
  }
  if (documents.length > opts.limits.maxDocuments) {
    throw notSourceError(
      "limit_exceeded",
      { limit: "max_documents", configured: opts.limits.maxDocuments },
      `document count exceeds max_documents (${opts.limits.maxDocuments})`,
    );
  }
  if (documents.length === 0) {
    return "";
  }

  const state: EmitPreflightState = {
    opts,
    nodesVisited: 0,
    minOutputBytes: 0n,
    ownedBytes: 0n,
  };
  const cleanDocs: Document[] = [];
  for (const item of documents) {
    if (isDocumentObject(item)) {
      const checkedDoc = createDocument(
        { kind: "null", span: null, via: null, tagStart: null },
        item.yamlVersion,
        item.explicitStart,
      );
      const root = convertAndPreflightNode(item.root, opts, state);
      cleanDocs.push({
        yamlVersion: checkedDoc.yamlVersion,
        explicitStart: checkedDoc.explicitStart,
        root,
      });
    } else {
      const root = convertAndPreflightNode(item, opts, state);
      cleanDocs.push({ yamlVersion: null, explicitStart: false, root });
    }
  }

  if (state.minOutputBytes > BigInt(opts.limits.maxOutputBytes)) {
    throw notSourceError(
      "output_limit_exceeded",
      { configured: opts.limits.maxOutputBytes },
      `encoded output exceeds max_output_bytes (${opts.limits.maxOutputBytes})`,
    );
  }
  if (state.ownedBytes > BigInt(MAX_WASM_INPUT_CEILING)) {
    throw notSourceError(
      "limit_exceeded",
      { limit: "max_owned_bytes", configured: MAX_WASM_INPUT_CEILING },
      `WASM emission logical owned weight exceeds ${MAX_WASM_INPUT_CEILING} bytes`,
    );
  }

  const w = new BoundedWriter(MAX_WASM_INPUT_CEILING);
  const encSt: EncodeState = {
    opts: {
      mergeKeys: "reject",
      unknownTags: "preserve",
      limits: {
        ...defaultLimits(),
        maxDepth: opts.limits.maxDepth,
        maxNodes: opts.limits.maxNodes,
        maxMappingEntries: opts.limits.maxMappingEntries,
        maxDocuments: opts.limits.maxDocuments,
        maxOutputBytes: MAX_WASM_INPUT_CEILING,
      },
    },
    nodesRemaining: opts.limits.maxNodes,
    activeAncestors: new Set<object>(),
  };
  let wireJson: string;
  try {
    w.pushRaw('{"format":"yamlprims-value","version":1,"documents":[');
    for (let i = 0; i < cleanDocs.length; i++) {
      if (i > 0) w.pushRaw(",");
      const d = cleanDocs[i]!;
      w.pushRaw('{"yaml_version":');
      if (d.yamlVersion !== null) {
        w.pushRaw(`[${d.yamlVersion[0]},${d.yamlVersion[1]}]`);
      } else {
        w.pushRaw("null");
      }
      w.pushRaw(d.explicitStart ? ',"explicit_start":true,"root":' : ',"explicit_start":false,"root":');
      writeNode(w, d.root, false, encSt, 0);
      w.pushRaw("}");
    }
    w.pushRaw("]}");
    wireJson = w.finish();
  } catch (err) {
    if (err instanceof YamlprimsError && err.kind === "output_limit_exceeded") {
      throw notSourceError(
        "limit_exceeded",
        { limit: "max_input_bytes", configured: MAX_WASM_INPUT_CEILING },
        `WASM emission wire envelope exceeds ${MAX_WASM_INPUT_CEILING} bytes`,
      );
    }
    throw err;
  }

  const wireBytes = encoder.encode(wireJson);
  if (wireBytes.byteLength > MAX_WASM_INPUT_CEILING) {
    throw notSourceError(
      "limit_exceeded",
      { limit: "max_input_bytes", configured: MAX_WASM_INPUT_CEILING },
      `WASM emission wire envelope exceeds ${MAX_WASM_INPUT_CEILING} bytes`,
    );
  }

  const { status, output } = runWasmEmit(wireBytes, opts);
  if (status === 2) {
    throw decodeFallbackStatus2(output, opts.limits.maxOutputBytes);
  }
  const text = decoder.decode(output);
  if (status === 1) {
    const errOpts = defaultOptions();
    const decoded = decodeWire(text, widenBridgeOptions(errOpts, output.byteLength));
    if (decoded.kind === "error") {
      throw decoded.error;
    }
    throw notSourceError("internal", {}, "expected error envelope from emit_wire status 1");
  }
  return text;
}

export function dump(value: unknown, options?: EmitOptionsInput): string {
  return dumpAll([value], options);
}

// Internal-only hook for in-repo linear-memory and trap tests (not part of the
// public TypeScript declarations or supported package surface).
Object.defineProperty(load, Symbol.for("yamlprims.internal.wasmTestHarness"), {
  value: createWasmTestHarness,
  enumerable: false,
  configurable: false,
  writable: false,
});
