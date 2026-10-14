---
id: DDR-0001
title: Rich values and a versioned lossless wire boundary
status: proposed
date: 2026-10-14
---

# DDR-0001: Rich values and a versioned lossless wire boundary

## Decision

Use an owned tree with private invariant-bearing fields, exact signed/unsigned
128-bit integer range, arbitrary mapping keys in ordered entries, non-core tags,
source/synthetic spans and explicit alias/merge provenance.

The wire representation encodes YAML integers as decimal strings and floats as
binary64 bit strings. Mapping keys are nodes, not JSON object member names.
Version the closed grammar independently from crates and the C ABI. Go and
TypeScript retain rich models and verify canonical round trips independently.

Plain conversion is a documented projection with explicit refusals. It must not
be presented as a lossless substitute for the wire contract.

## Locations and accounting

Offsets refer to original input bytes, including a BOM; columns count Unicode
scalar values. Ordinary and indentless block sequences have distinct start
rules. Bind every expectation to this explicit convention.

Revision 1.1 proposes exact block-scalar ends and omitted-child placement, with
parent containment except at alias/merge insertion boundaries. Scalar values
remain unchanged even when their spans exclude trailing breaks retained by
chomping. Explicit empty documents have boundary-positioned Null roots;
zero-document streams stay empty in multi-document APIs.

These changes retain wire grammar version 1 but change profile-bound locations
and document cardinality. Preserve revision 1.0 profiles and goldens; qualify a
new specification/profile pin rather than updating historical expectations.

Tags add one wrapper node, whose weight includes its three tag-string lengths
once. Logical accounting, actual allocation and process RSS are distinct values.
Resolve the remaining [freeze items](../design-review.md) before conformance claims.

## Acceptance

Test wide integers, signed zero, nonfinite floats, composite/duplicate keys,
recursive NaN-key rejection, tags, source positions and synthetic nodes. Compare
error categories and complete paths, not just a generic failure outcome.
