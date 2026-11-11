//! Typed `serde::Serialize` conversion to `Node` and YAML text (spec-v1.5 §8.2).

use crate::budget::{ENTRY_OVERHEAD, NODE_OVERHEAD};
use crate::emit::{emit_with, EmitOptions};
use crate::error::{Error, ErrorKind, LimitKind, Site};
use crate::protocol::{
    SerActiveState, Spanned, TaggedValue, NODE, SER_ACTIVE_STATE, SER_TAG_SLOT, SER_VALUE_SLOT,
    SPANNED, TAGGED_VALUE,
};
use crate::value::{
    is_valid_non_core_tag_uri, Document, InsertError, Integer, Mapping, Node, Tag, Tagged, Value,
};
use serde::ser::{
    self, Serialize, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant,
    SerializeTuple, SerializeTupleStruct, SerializeTupleVariant, Serializer,
};

const CANONICAL_NAN_BITS: u64 = 0x7ff8_0000_0000_0000;

fn ser_err(kind: ErrorKind, msg: impl std::fmt::Display) -> Error {
    Error::new(kind, Site::NotSource { last_known: None }, msg)
}

fn ser_limit_err(limit: LimitKind, configured: usize, msg: &str) -> Error {
    ser_err(
        ErrorKind::LimitExceeded {
            limit,
            configured: configured as u64,
        },
        format!("{msg} ({configured})"),
    )
}

fn canonicalize_float(f: f64) -> f64 {
    if f.is_nan() {
        f64::from_bits(CANONICAL_NAN_BITS)
    } else {
        f
    }
}

enum SerKeyWeightFrame<'a> {
    Val(&'a Value),
    Seq {
        items: &'a [Node],
        idx: usize,
    },
    Map {
        entries: &'a [(Node, Node)],
        idx: usize,
        next_val: Option<&'a Value>,
    },
}

fn check_key_weight_borrowed(key_root: &Value, max_key_bytes: usize) -> Result<(), Error> {
    let mut total: usize = 0;
    let mut stack: Vec<SerKeyWeightFrame<'_>> = vec![SerKeyWeightFrame::Val(key_root)];
    while let Some(frame) = stack.pop() {
        let v = match frame {
            SerKeyWeightFrame::Val(v) => v,
            SerKeyWeightFrame::Seq { items, idx } => {
                let Some(item) = items.get(idx) else {
                    continue;
                };
                if idx + 1 < items.len() {
                    stack.push(SerKeyWeightFrame::Seq {
                        items,
                        idx: idx + 1,
                    });
                }
                &item.value
            }
            SerKeyWeightFrame::Map {
                entries,
                idx,
                next_val: Some(val),
            } => {
                if idx + 1 < entries.len() {
                    stack.push(SerKeyWeightFrame::Map {
                        entries,
                        idx: idx + 1,
                        next_val: None,
                    });
                }
                val
            }
            SerKeyWeightFrame::Map {
                entries,
                idx,
                next_val: None,
            } => {
                let Some((k, val)) = entries.get(idx) else {
                    continue;
                };
                stack.push(SerKeyWeightFrame::Map {
                    entries,
                    idx,
                    next_val: Some(&val.value),
                });
                &k.value
            }
        };
        total = total.saturating_add(NODE_OVERHEAD as usize);
        if total > max_key_bytes {
            return Err(ser_limit_err(
                LimitKind::MaxKeyBytes,
                max_key_bytes,
                "mapping key exceeds max_key_bytes",
            ));
        }
        match v {
            Value::Null | Value::Bool(_) | Value::Int(_) | Value::Float(_) => {}
            Value::String(s) => {
                total = total.saturating_add(s.len());
                if total > max_key_bytes {
                    return Err(ser_limit_err(
                        LimitKind::MaxKeyBytes,
                        max_key_bytes,
                        "mapping key exceeds max_key_bytes",
                    ));
                }
            }
            Value::Sequence(items) => {
                if !items.is_empty() {
                    stack.push(SerKeyWeightFrame::Seq { items, idx: 0 });
                }
            }
            Value::Mapping(map) => {
                total = total.saturating_add(map.len().saturating_mul(ENTRY_OVERHEAD as usize));
                if total > max_key_bytes {
                    return Err(ser_limit_err(
                        LimitKind::MaxKeyBytes,
                        max_key_bytes,
                        "mapping key exceeds max_key_bytes",
                    ));
                }
                if !map.entries.is_empty() {
                    stack.push(SerKeyWeightFrame::Map {
                        entries: &map.entries,
                        idx: 0,
                        next_val: None,
                    });
                }
            }
            Value::Tagged(t) => {
                total = total.saturating_add(t.tag.uri.len().saturating_mul(2));
                if total > max_key_bytes {
                    return Err(ser_limit_err(
                        LimitKind::MaxKeyBytes,
                        max_key_bytes,
                        "mapping key exceeds max_key_bytes",
                    ));
                }
                stack.push(SerKeyWeightFrame::Val(&t.value.value));
            }
        }
    }
    Ok(())
}

fn check_key_weight_for_ser(key_root: &Node, max_key_bytes: usize) -> Result<(), Error> {
    check_key_weight_borrowed(&key_root.value, max_key_bytes)
}

enum BorrowedPreflightFrame<'a> {
    Val {
        val: &'a Value,
        depth: usize,
        in_key: bool,
        is_key_root: bool,
    },
    Seq {
        items: &'a [Node],
        idx: usize,
        depth: usize,
        in_key: bool,
    },
    Map {
        entries: &'a [(Node, Node)],
        idx: usize,
        next_val: Option<&'a Value>,
        depth: usize,
        in_key: bool,
    },
}

fn preflight_borrowed_value(root: &Value, st: &SerActiveState) -> Result<(), Error> {
    let mut nodes_used = st.nodes_used;
    let mut stack: Vec<BorrowedPreflightFrame<'_>> = vec![BorrowedPreflightFrame::Val {
        val: root,
        depth: st.depth,
        in_key: st.in_key,
        is_key_root: false,
    }];
    while let Some(step) = stack.pop() {
        let (val, depth, in_key, is_key_root) = match step {
            BorrowedPreflightFrame::Val {
                val,
                depth,
                in_key,
                is_key_root,
            } => (val, depth, in_key, is_key_root),
            BorrowedPreflightFrame::Seq {
                items,
                idx,
                depth,
                in_key,
            } => {
                let Some(item) = items.get(idx) else {
                    continue;
                };
                if idx + 1 < items.len() {
                    stack.push(BorrowedPreflightFrame::Seq {
                        items,
                        idx: idx + 1,
                        depth,
                        in_key,
                    });
                }
                (&item.value, depth, in_key, false)
            }
            BorrowedPreflightFrame::Map {
                entries,
                idx,
                next_val: Some(v),
                depth,
                in_key,
            } => {
                if idx + 1 < entries.len() {
                    stack.push(BorrowedPreflightFrame::Map {
                        entries,
                        idx: idx + 1,
                        next_val: None,
                        depth,
                        in_key,
                    });
                }
                (v, depth, in_key, false)
            }
            BorrowedPreflightFrame::Map {
                entries,
                idx,
                next_val: None,
                depth,
                in_key,
            } => {
                let Some((k, v)) = entries.get(idx) else {
                    continue;
                };
                stack.push(BorrowedPreflightFrame::Map {
                    entries,
                    idx,
                    next_val: Some(&v.value),
                    depth,
                    in_key,
                });
                (&k.value, depth, true, true)
            }
        };
        nodes_used += 1;
        if nodes_used > st.limits.max_nodes {
            return Err(ser_limit_err(
                LimitKind::MaxNodes,
                st.limits.max_nodes,
                "node count exceeds max_nodes",
            ));
        }
        if is_key_root {
            check_key_weight_borrowed(val, st.limits.max_key_bytes)?;
        }
        match val {
            Value::Null | Value::Bool(_) | Value::Int(_) => {}
            Value::Float(f) => {
                if in_key && f.is_nan() {
                    return Err(ser_err(
                        ErrorKind::InvalidKey,
                        "NaN float is not valid inside a mapping key",
                    ));
                }
            }
            Value::String(s) => {
                if s.len() > st.limits.max_scalar_bytes {
                    return Err(ser_limit_err(
                        LimitKind::MaxScalarBytes,
                        st.limits.max_scalar_bytes,
                        "scalar byte length exceeds max_scalar_bytes",
                    ));
                }
            }
            Value::Sequence(items) => {
                let next_depth = depth + 1;
                if next_depth > st.limits.max_depth {
                    return Err(ser_limit_err(
                        LimitKind::MaxDepth,
                        st.limits.max_depth,
                        "collection nesting exceeds max_depth",
                    ));
                }
                if !items.is_empty() {
                    stack.push(BorrowedPreflightFrame::Seq {
                        items,
                        idx: 0,
                        depth: next_depth,
                        in_key,
                    });
                }
            }
            Value::Mapping(map) => {
                let next_depth = depth + 1;
                if next_depth > st.limits.max_depth {
                    return Err(ser_limit_err(
                        LimitKind::MaxDepth,
                        st.limits.max_depth,
                        "collection nesting exceeds max_depth",
                    ));
                }
                if map.len() > st.limits.max_mapping_entries {
                    return Err(ser_limit_err(
                        LimitKind::MaxMappingEntries,
                        st.limits.max_mapping_entries,
                        "mapping entry count exceeds max_mapping_entries",
                    ));
                }
                if !map.entries.is_empty() {
                    stack.push(BorrowedPreflightFrame::Map {
                        entries: &map.entries,
                        idx: 0,
                        next_val: None,
                        depth: next_depth,
                        in_key,
                    });
                }
            }
            Value::Tagged(t) => {
                if t.tag.uri.len() > st.limits.max_tag_bytes
                    || !is_valid_non_core_tag_uri(&t.tag.uri)
                {
                    return Err(ser_err(
                        ErrorKind::InvalidOptions,
                        "Tagged node contains invalid or oversized Tag::uri",
                    ));
                }
                if in_key && t.tag.uri == "tag:yaml.org,2002:merge" {
                    return Err(ser_err(
                        ErrorKind::TagKindMismatch,
                        "tag:yaml.org,2002:merge is not valid on a mapping key",
                    ));
                }
                stack.push(BorrowedPreflightFrame::Val {
                    val: &t.value.value,
                    depth,
                    in_key,
                    is_key_root: false,
                });
            }
        }
    }
    Ok(())
}

fn arm_active_state(opts: &EmitOptions, nodes_used: usize, depth: usize, in_key: bool) {
    SER_ACTIVE_STATE.with(|s| {
        *s.borrow_mut() = Some(SerActiveState {
            limits: opts.limits.clone(),
            nodes_used,
            depth,
            in_key,
        });
    });
}

/// Convert a `serde::Serialize` value into a synthetic [`Node`] under `opts` (spec-v1.5 §8.2).
pub fn to_node<T: Serialize + ?Sized>(value: &T, opts: &EmitOptions) -> Result<Node, Error> {
    opts.validate()?;
    let mut nodes_used = 0usize;
    let ser = NodeSerializer {
        opts,
        nodes_used: &mut nodes_used,
        depth: 0,
        in_key: false,
        wrapper_hops: 0,
    };
    value.serialize(ser)
}

/// Serialize `value` to canonical YAML 1.2.2 Core text under [`EmitOptions::default()`] (spec-v1.5 §8.2).
pub fn to_string<T: Serialize + ?Sized>(value: &T) -> Result<String, Error> {
    to_string_with(value, &EmitOptions::default())
}

/// Serialize `value` to canonical YAML 1.2.2 Core text under `opts` (spec-v1.5 §8.2).
pub fn to_string_with<T: Serialize + ?Sized>(
    value: &T,
    opts: &EmitOptions,
) -> Result<String, Error> {
    let node = to_node(value, opts)?;
    emit_with(&node, opts)
}

struct NodeSerializer<'a> {
    opts: &'a EmitOptions,
    nodes_used: &'a mut usize,
    depth: usize,
    in_key: bool,
    wrapper_hops: usize,
}

impl<'a> NodeSerializer<'a> {
    fn charge_node(&mut self) -> Result<(), Error> {
        *self.nodes_used += 1;
        if *self.nodes_used > self.opts.limits.max_nodes {
            return Err(ser_limit_err(
                LimitKind::MaxNodes,
                self.opts.limits.max_nodes,
                "node count exceeds max_nodes",
            ));
        }
        Ok(())
    }

    fn next_collection_depth(&self) -> Result<usize, Error> {
        let next = self.depth + 1;
        if next > self.opts.limits.max_depth {
            return Err(ser_limit_err(
                LimitKind::MaxDepth,
                self.opts.limits.max_depth,
                "collection nesting exceeds max_depth",
            ));
        }
        Ok(next)
    }

    fn check_wrapper_hop(&mut self) -> Result<(), Error> {
        self.wrapper_hops += 1;
        if self.wrapper_hops > self.opts.limits.max_depth.saturating_add(16) {
            return Err(ser_limit_err(
                LimitKind::MaxDepth,
                self.opts.limits.max_depth,
                "transparent wrapper nesting exceeds max_depth",
            ));
        }
        Ok(())
    }

    fn string_node(&mut self, s: &str) -> Result<Node, Error> {
        self.charge_node()?;
        if s.len() > self.opts.limits.max_scalar_bytes {
            return Err(ser_limit_err(
                LimitKind::MaxScalarBytes,
                self.opts.limits.max_scalar_bytes,
                "scalar byte length exceeds max_scalar_bytes",
            ));
        }
        Ok(Node::synthetic(Value::String(s.to_owned())))
    }

    fn float_node(&mut self, f: f64) -> Result<Node, Error> {
        self.charge_node()?;
        if self.in_key && f.is_nan() {
            return Err(ser_err(
                ErrorKind::InvalidKey,
                "NaN float is not valid inside a mapping key",
            ));
        }
        Ok(Node::synthetic(Value::Float(canonicalize_float(f))))
    }

    fn copy_existing_value(&mut self, root_val: Value) -> Result<Node, Error> {
        // Iteratively validate and clone `root_val` into a synthetic Node while charging budgets.
        enum Work {
            Enter {
                val: Value,
                depth: usize,
                in_key: bool,
                is_key_root: bool,
            },
            SeqStep {
                iter: std::vec::IntoIter<Node>,
                depth: usize,
                in_key: bool,
                len: usize,
            },
            MapStep {
                iter: std::vec::IntoIter<(Node, Node)>,
                next_val: Option<Value>,
                depth: usize,
                in_key: bool,
                len: usize,
            },
            BuildTagged {
                tag: Tag,
                in_key: bool,
            },
        }

        let mut work: Vec<Work> = vec![Work::Enter {
            val: root_val,
            depth: self.depth,
            in_key: self.in_key,
            is_key_root: false,
        }];
        let mut results: Vec<Node> = Vec::new();

        while let Some(step) = work.pop() {
            match step {
                Work::Enter {
                    val,
                    depth,
                    in_key,
                    is_key_root,
                } => {
                    self.charge_node()?;
                    let temp_node_for_key_check;
                    if is_key_root {
                        temp_node_for_key_check = Node::synthetic(val);
                        check_key_weight_for_ser(
                            &temp_node_for_key_check,
                            self.opts.limits.max_key_bytes,
                        )?;
                    } else {
                        temp_node_for_key_check = Node::synthetic(val);
                    }
                    let val = temp_node_for_key_check.into_value();
                    match val {
                        Value::Null => results.push(Node::synthetic(Value::Null)),
                        Value::Bool(b) => results.push(Node::synthetic(Value::Bool(b))),
                        Value::Int(i) => results.push(Node::synthetic(Value::Int(i))),
                        Value::Float(f) => {
                            if in_key && f.is_nan() {
                                return Err(ser_err(
                                    ErrorKind::InvalidKey,
                                    "NaN float is not valid inside a mapping key",
                                ));
                            }
                            results.push(Node::synthetic(Value::Float(canonicalize_float(f))));
                        }
                        Value::String(s) => {
                            if s.len() > self.opts.limits.max_scalar_bytes {
                                return Err(ser_limit_err(
                                    LimitKind::MaxScalarBytes,
                                    self.opts.limits.max_scalar_bytes,
                                    "scalar byte length exceeds max_scalar_bytes",
                                ));
                            }
                            results.push(Node::synthetic(Value::String(s)));
                        }
                        Value::Sequence(items) => {
                            let next_depth = depth + 1;
                            if next_depth > self.opts.limits.max_depth {
                                return Err(ser_limit_err(
                                    LimitKind::MaxDepth,
                                    self.opts.limits.max_depth,
                                    "collection nesting exceeds max_depth",
                                ));
                            }
                            let len = items.len();
                            work.push(Work::SeqStep {
                                iter: items.into_iter(),
                                depth: next_depth,
                                in_key,
                                len,
                            });
                        }
                        Value::Mapping(map) => {
                            let next_depth = depth + 1;
                            if next_depth > self.opts.limits.max_depth {
                                return Err(ser_limit_err(
                                    LimitKind::MaxDepth,
                                    self.opts.limits.max_depth,
                                    "collection nesting exceeds max_depth",
                                ));
                            }
                            if map.len() > self.opts.limits.max_mapping_entries {
                                return Err(ser_limit_err(
                                    LimitKind::MaxMappingEntries,
                                    self.opts.limits.max_mapping_entries,
                                    "mapping entry count exceeds max_mapping_entries",
                                ));
                            }
                            let entries: Vec<(Node, Node)> = map.entries;
                            let len = entries.len();
                            work.push(Work::MapStep {
                                iter: entries.into_iter(),
                                next_val: None,
                                depth: next_depth,
                                in_key,
                                len,
                            });
                        }
                        Value::Tagged(mut t) => {
                            if t.tag.uri.len() > self.opts.limits.max_tag_bytes
                                || !is_valid_non_core_tag_uri(&t.tag.uri)
                            {
                                return Err(ser_err(
                                    ErrorKind::InvalidOptions,
                                    "Tagged node contains invalid or oversized Tag::uri",
                                ));
                            }
                            let inner_node =
                                std::mem::replace(&mut t.value, Node::synthetic(Value::Null));
                            let tag = t.tag.clone();
                            work.push(Work::BuildTagged { tag, in_key });
                            work.push(Work::Enter {
                                val: inner_node.into_value(),
                                depth,
                                in_key,
                                is_key_root: false,
                            });
                        }
                    }
                }
                Work::SeqStep {
                    mut iter,
                    depth,
                    in_key,
                    len,
                } => {
                    if let Some(item) = iter.next() {
                        work.push(Work::SeqStep {
                            iter,
                            depth,
                            in_key,
                            len,
                        });
                        work.push(Work::Enter {
                            val: item.into_value(),
                            depth,
                            in_key,
                            is_key_root: false,
                        });
                    } else {
                        let split = results.len() - len;
                        let items = results.split_off(split);
                        results.push(Node::synthetic(Value::Sequence(items)));
                    }
                }
                Work::MapStep {
                    mut iter,
                    next_val,
                    depth,
                    in_key,
                    len,
                } => {
                    if let Some(v) = next_val {
                        work.push(Work::MapStep {
                            iter,
                            next_val: None,
                            depth,
                            in_key,
                            len,
                        });
                        work.push(Work::Enter {
                            val: v,
                            depth,
                            in_key,
                            is_key_root: false,
                        });
                    } else if let Some((k, v)) = iter.next() {
                        work.push(Work::MapStep {
                            iter,
                            next_val: Some(v.into_value()),
                            depth,
                            in_key,
                            len,
                        });
                        work.push(Work::Enter {
                            val: k.into_value(),
                            depth,
                            in_key: true,
                            is_key_root: true,
                        });
                    } else {
                        let split = results.len() - len * 2;
                        let flat = results.split_off(split);
                        let mut m = Mapping::new();
                        let mut pair_iter = flat.into_iter();
                        while let (Some(k), Some(v)) = (pair_iter.next(), pair_iter.next()) {
                            m.insert_new(k, v).map_err(|e| match e {
                                InsertError::InvalidKey => ser_err(
                                    ErrorKind::InvalidKey,
                                    "NaN float is not valid inside a mapping key",
                                ),
                                InsertError::Duplicate { .. } => {
                                    ser_err(ErrorKind::DuplicateKey, "duplicate mapping key")
                                }
                            })?;
                        }
                        results.push(Node::synthetic(Value::Mapping(m)));
                    }
                }
                Work::BuildTagged { tag, in_key } => {
                    let inner = results.pop().expect("tagged inner node");
                    if in_key && tag.uri == "tag:yaml.org,2002:merge" {
                        return Err(ser_err(
                            ErrorKind::TagKindMismatch,
                            "tag:yaml.org,2002:merge is not valid on a mapping key",
                        ));
                    }
                    let tagged = Tagged::new(tag, inner)?;
                    results.push(Node::synthetic(Value::Tagged(Box::new(tagged))));
                }
            }
        }
        Ok(results.pop().expect("root node"))
    }
}

impl<'a> Serializer for NodeSerializer<'a> {
    type Ok = Node;
    type Error = Error;
    type SerializeSeq = SeqBuilder<'a>;
    type SerializeTuple = SeqBuilder<'a>;
    type SerializeTupleStruct = SeqBuilder<'a>;
    type SerializeTupleVariant = VariantSeqBuilder<'a>;
    type SerializeMap = MapBuilder<'a>;
    type SerializeStruct = MapBuilder<'a>;
    type SerializeStructVariant = VariantMapBuilder<'a>;

    fn serialize_bool(mut self, v: bool) -> Result<Node, Error> {
        self.charge_node()?;
        Ok(Node::synthetic(Value::Bool(v)))
    }

    fn serialize_i8(self, v: i8) -> Result<Node, Error> {
        self.serialize_i128(i128::from(v))
    }

    fn serialize_i16(self, v: i16) -> Result<Node, Error> {
        self.serialize_i128(i128::from(v))
    }

    fn serialize_i32(self, v: i32) -> Result<Node, Error> {
        self.serialize_i128(i128::from(v))
    }

    fn serialize_i64(self, v: i64) -> Result<Node, Error> {
        self.serialize_i128(i128::from(v))
    }

    fn serialize_i128(mut self, v: i128) -> Result<Node, Error> {
        self.charge_node()?;
        Ok(Node::synthetic(Value::Int(Integer::from_i128(v))))
    }

    fn serialize_u8(self, v: u8) -> Result<Node, Error> {
        self.serialize_u128(u128::from(v))
    }

    fn serialize_u16(self, v: u16) -> Result<Node, Error> {
        self.serialize_u128(u128::from(v))
    }

    fn serialize_u32(self, v: u32) -> Result<Node, Error> {
        self.serialize_u128(u128::from(v))
    }

    fn serialize_u64(self, v: u64) -> Result<Node, Error> {
        self.serialize_u128(u128::from(v))
    }

    fn serialize_u128(mut self, v: u128) -> Result<Node, Error> {
        self.charge_node()?;
        Ok(Node::synthetic(Value::Int(Integer::from_u128(v))))
    }

    fn serialize_f32(mut self, v: f32) -> Result<Node, Error> {
        self.float_node(f64::from(v))
    }

    fn serialize_f64(mut self, v: f64) -> Result<Node, Error> {
        self.float_node(v)
    }

    fn serialize_char(mut self, v: char) -> Result<Node, Error> {
        let mut buf = [0u8; 4];
        self.string_node(v.encode_utf8(&mut buf))
    }

    fn serialize_str(mut self, v: &str) -> Result<Node, Error> {
        self.string_node(v)
    }

    fn serialize_bytes(mut self, v: &[u8]) -> Result<Node, Error> {
        self.charge_node()?;
        let _next_depth = self.next_collection_depth()?;
        let cap = v
            .len()
            .min(self.opts.limits.max_nodes.saturating_sub(*self.nodes_used))
            .min(1024);
        let mut items = Vec::with_capacity(cap);
        for &b in v {
            self.charge_node()?;
            items.push(Node::synthetic(Value::Int(Integer::from_u128(u128::from(
                b,
            )))));
        }
        Ok(Node::synthetic(Value::Sequence(items)))
    }

    fn serialize_none(mut self) -> Result<Node, Error> {
        self.charge_node()?;
        Ok(Node::synthetic(Value::Null))
    }

    fn serialize_some<T: ?Sized + Serialize>(mut self, value: &T) -> Result<Node, Error> {
        self.check_wrapper_hop()?;
        value.serialize(self)
    }

    fn serialize_unit(mut self) -> Result<Node, Error> {
        if SER_VALUE_SLOT.with(|s| s.borrow().is_some()) {
            return Ok(Node::synthetic(Value::Null));
        }
        self.charge_node()?;
        Ok(Node::synthetic(Value::Null))
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<Node, Error> {
        self.serialize_unit()
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<Node, Error> {
        self.serialize_str(variant)
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        mut self,
        name: &'static str,
        value: &T,
    ) -> Result<Node, Error> {
        match name {
            NODE => {
                if let Some(res) = SER_VALUE_SLOT.with(|s| s.borrow_mut().take()) {
                    let val = res?;
                    return self.copy_existing_value(val);
                }
                struct SlotGuard;
                impl Drop for SlotGuard {
                    fn drop(&mut self) {
                        SER_ACTIVE_STATE.with(|s| {
                            let _ = s.borrow_mut().take();
                        });
                        SER_VALUE_SLOT.with(|s| {
                            let _ = s.borrow_mut().take();
                        });
                    }
                }
                let _guard = SlotGuard;
                arm_active_state(self.opts, *self.nodes_used, self.depth, self.in_key);
                let fallback = value.serialize(NodeSerializer {
                    opts: self.opts,
                    nodes_used: self.nodes_used,
                    depth: self.depth,
                    in_key: self.in_key,
                    wrapper_hops: self.wrapper_hops,
                });
                SER_ACTIVE_STATE.with(|s| {
                    let _ = s.borrow_mut().take();
                });
                if let Some(res) = SER_VALUE_SLOT.with(|s| s.borrow_mut().take()) {
                    let val = res?;
                    return self.copy_existing_value(val);
                }
                fallback
            }
            SPANNED => {
                self.check_wrapper_hop()?;
                value.serialize(self)
            }
            TAGGED_VALUE => {
                let Some(tag) = SER_TAG_SLOT.with(|s| s.borrow_mut().take()) else {
                    return Err(ser_err(
                        ErrorKind::Internal,
                        "TaggedValue missing tag in serializer slot",
                    ));
                };
                self.charge_node()?;
                if tag.uri.len() > self.opts.limits.max_tag_bytes
                    || !is_valid_non_core_tag_uri(&tag.uri)
                {
                    return Err(ser_err(
                        ErrorKind::InvalidOptions,
                        "TaggedValue contains invalid or oversized Tag::uri",
                    ));
                }
                let inner = value.serialize(NodeSerializer {
                    opts: self.opts,
                    nodes_used: self.nodes_used,
                    depth: self.depth,
                    in_key: self.in_key,
                    wrapper_hops: self.wrapper_hops,
                })?;
                if self.in_key && tag.uri == "tag:yaml.org,2002:merge" {
                    return Err(ser_err(
                        ErrorKind::TagKindMismatch,
                        "tag:yaml.org,2002:merge is not valid on a mapping key",
                    ));
                }
                let tagged = Tagged::new(tag, inner)?;
                Ok(Node::synthetic(Value::Tagged(Box::new(tagged))))
            }
            _ => {
                self.check_wrapper_hop()?;
                value.serialize(self)
            }
        }
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        mut self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Node, Error> {
        self.charge_node()?;
        let next_depth = self.next_collection_depth()?;
        if self.opts.limits.max_mapping_entries < 1 {
            return Err(ser_limit_err(
                LimitKind::MaxMappingEntries,
                self.opts.limits.max_mapping_entries,
                "mapping entry count exceeds max_mapping_entries",
            ));
        }
        let key_node = (NodeSerializer {
            opts: self.opts,
            nodes_used: self.nodes_used,
            depth: next_depth,
            in_key: true,
            wrapper_hops: 0,
        })
        .string_node(variant)?;
        check_key_weight_for_ser(&key_node, self.opts.limits.max_key_bytes)?;
        let val_node = value.serialize(NodeSerializer {
            opts: self.opts,
            nodes_used: self.nodes_used,
            depth: next_depth,
            in_key: self.in_key,
            wrapper_hops: 0,
        })?;
        let mut m = Mapping::new();
        m.insert_new(key_node, val_node)
            .expect("single string key cannot fail");
        Ok(Node::synthetic(Value::Mapping(m)))
    }

    fn serialize_seq(mut self, len: Option<usize>) -> Result<Self::SerializeSeq, Error> {
        self.charge_node()?;
        let next_depth = self.next_collection_depth()?;
        Ok(SeqBuilder {
            opts: self.opts,
            nodes_used: self.nodes_used,
            depth: next_depth,
            in_key: self.in_key,
            items: Vec::with_capacity(len.unwrap_or(0).min(64)),
        })
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Error> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, Error> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_variant(
        mut self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, Error> {
        self.charge_node()?; // outer Mapping
        let map_depth = self.next_collection_depth()?;
        if self.opts.limits.max_mapping_entries < 1 {
            return Err(ser_limit_err(
                LimitKind::MaxMappingEntries,
                self.opts.limits.max_mapping_entries,
                "mapping entry count exceeds max_mapping_entries",
            ));
        }
        let key_node = (NodeSerializer {
            opts: self.opts,
            nodes_used: self.nodes_used,
            depth: map_depth,
            in_key: true,
            wrapper_hops: 0,
        })
        .string_node(variant)?;
        check_key_weight_for_ser(&key_node, self.opts.limits.max_key_bytes)?;
        self.charge_node()?; // inner Sequence
        let seq_depth = map_depth + 1;
        if seq_depth > self.opts.limits.max_depth {
            return Err(ser_limit_err(
                LimitKind::MaxDepth,
                self.opts.limits.max_depth,
                "collection nesting exceeds max_depth",
            ));
        }
        Ok(VariantSeqBuilder {
            key_node,
            seq: SeqBuilder {
                opts: self.opts,
                nodes_used: self.nodes_used,
                depth: seq_depth,
                in_key: self.in_key,
                items: Vec::with_capacity(len.min(64)),
            },
        })
    }

    fn serialize_map(mut self, _len: Option<usize>) -> Result<Self::SerializeMap, Error> {
        self.charge_node()?;
        let next_depth = self.next_collection_depth()?;
        Ok(MapBuilder {
            opts: self.opts,
            nodes_used: self.nodes_used,
            depth: next_depth,
            in_key: self.in_key,
            map: Mapping::new(),
            pending_key: None,
        })
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct, Error> {
        self.serialize_map(Some(len))
    }

    fn serialize_struct_variant(
        mut self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant, Error> {
        self.charge_node()?; // outer Mapping
        let map_depth = self.next_collection_depth()?;
        if self.opts.limits.max_mapping_entries < 1 {
            return Err(ser_limit_err(
                LimitKind::MaxMappingEntries,
                self.opts.limits.max_mapping_entries,
                "mapping entry count exceeds max_mapping_entries",
            ));
        }
        let key_node = (NodeSerializer {
            opts: self.opts,
            nodes_used: self.nodes_used,
            depth: map_depth,
            in_key: true,
            wrapper_hops: 0,
        })
        .string_node(variant)?;
        check_key_weight_for_ser(&key_node, self.opts.limits.max_key_bytes)?;
        self.charge_node()?; // inner Mapping
        let inner_depth = map_depth + 1;
        if inner_depth > self.opts.limits.max_depth {
            return Err(ser_limit_err(
                LimitKind::MaxDepth,
                self.opts.limits.max_depth,
                "collection nesting exceeds max_depth",
            ));
        }
        Ok(VariantMapBuilder {
            key_node,
            inner: MapBuilder {
                opts: self.opts,
                nodes_used: self.nodes_used,
                depth: inner_depth,
                in_key: self.in_key,
                map: Mapping::new(),
                pending_key: None,
            },
        })
    }
}

struct SeqBuilder<'a> {
    opts: &'a EmitOptions,
    nodes_used: &'a mut usize,
    depth: usize,
    in_key: bool,
    items: Vec<Node>,
}

impl<'a> SerializeSeq for SeqBuilder<'a> {
    type Ok = Node;
    type Error = Error;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Error> {
        let node = value.serialize(NodeSerializer {
            opts: self.opts,
            nodes_used: self.nodes_used,
            depth: self.depth,
            in_key: self.in_key,
            wrapper_hops: 0,
        })?;
        self.items.push(node);
        Ok(())
    }

    fn end(self) -> Result<Node, Error> {
        Ok(Node::synthetic(Value::Sequence(self.items)))
    }
}

impl<'a> SerializeTuple for SeqBuilder<'a> {
    type Ok = Node;
    type Error = Error;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Error> {
        SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Node, Error> {
        SerializeSeq::end(self)
    }
}

impl<'a> SerializeTupleStruct for SeqBuilder<'a> {
    type Ok = Node;
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Error> {
        SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Node, Error> {
        SerializeSeq::end(self)
    }
}

struct VariantSeqBuilder<'a> {
    key_node: Node,
    seq: SeqBuilder<'a>,
}

impl<'a> SerializeTupleVariant for VariantSeqBuilder<'a> {
    type Ok = Node;
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Error> {
        SerializeSeq::serialize_element(&mut self.seq, value)
    }

    fn end(self) -> Result<Node, Error> {
        let seq_node = SerializeSeq::end(self.seq)?;
        let mut m = Mapping::new();
        m.insert_new(self.key_node, seq_node)
            .expect("single string key cannot fail");
        Ok(Node::synthetic(Value::Mapping(m)))
    }
}

struct MapBuilder<'a> {
    opts: &'a EmitOptions,
    nodes_used: &'a mut usize,
    depth: usize,
    in_key: bool,
    map: Mapping,
    pending_key: Option<Node>,
}

impl<'a> SerializeMap for MapBuilder<'a> {
    type Ok = Node;
    type Error = Error;

    fn serialize_key<T: ?Sized + Serialize>(&mut self, key: &T) -> Result<(), Error> {
        let key_node = key.serialize(NodeSerializer {
            opts: self.opts,
            nodes_used: self.nodes_used,
            depth: self.depth,
            in_key: true,
            wrapper_hops: 0,
        })?;
        check_key_weight_for_ser(&key_node, self.opts.limits.max_key_bytes)?;
        self.pending_key = Some(key_node);
        Ok(())
    }

    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Error> {
        let Some(key_node) = self.pending_key.take() else {
            return Err(ser_err(
                ErrorKind::Internal,
                "serialize_value called without serialize_key",
            ));
        };
        let val_node = value.serialize(NodeSerializer {
            opts: self.opts,
            nodes_used: self.nodes_used,
            depth: self.depth,
            in_key: self.in_key,
            wrapper_hops: 0,
        })?;
        if self.map.len() >= self.opts.limits.max_mapping_entries {
            return Err(ser_limit_err(
                LimitKind::MaxMappingEntries,
                self.opts.limits.max_mapping_entries,
                "mapping entry count exceeds max_mapping_entries",
            ));
        }
        self.map
            .insert_new(key_node, val_node)
            .map_err(|e| match e {
                InsertError::InvalidKey => ser_err(
                    ErrorKind::InvalidKey,
                    "NaN float is not valid inside a mapping key",
                ),
                InsertError::Duplicate { .. } => {
                    ser_err(ErrorKind::DuplicateKey, "duplicate mapping key")
                }
            })?;
        Ok(())
    }

    fn end(self) -> Result<Node, Error> {
        Ok(Node::synthetic(Value::Mapping(self.map)))
    }
}

impl<'a> SerializeStruct for MapBuilder<'a> {
    type Ok = Node;
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Error> {
        SerializeMap::serialize_entry(self, key, value)
    }

    fn end(self) -> Result<Node, Error> {
        SerializeMap::end(self)
    }
}

struct VariantMapBuilder<'a> {
    key_node: Node,
    inner: MapBuilder<'a>,
}

impl<'a> SerializeStructVariant for VariantMapBuilder<'a> {
    type Ok = Node;
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Error> {
        SerializeMap::serialize_entry(&mut self.inner, key, value)
    }

    fn end(self) -> Result<Node, Error> {
        let inner_node = SerializeMap::end(self.inner)?;
        let mut m = Mapping::new();
        m.insert_new(self.key_node, inner_node)
            .expect("single string key cannot fail");
        Ok(Node::synthetic(Value::Mapping(m)))
    }
}

impl<T: Serialize> Serialize for Spanned<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_newtype_struct(SPANNED, &self.value)
    }
}

impl<T: Serialize> Serialize for TaggedValue<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        struct ClearGuard;
        impl Drop for ClearGuard {
            fn drop(&mut self) {
                SER_TAG_SLOT.with(|s| {
                    let _ = s.borrow_mut().take();
                });
            }
        }
        let _guard = ClearGuard;
        SER_TAG_SLOT.with(|s| *s.borrow_mut() = Some(self.tag.clone()));
        serializer.serialize_newtype_struct(TAGGED_VALUE, &self.value)
    }
}

impl Serialize for Integer {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if let Some(i) = self.to_i128() {
            serializer.serialize_i128(i)
        } else if let Some(u) = self.to_u128() {
            serializer.serialize_u128(u)
        } else {
            Err(ser::Error::custom("integer out of i128/u128 range"))
        }
    }
}

impl Serialize for Mapping {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.len()))?;
        for (k, v) in &self.entries {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

struct ValueDataModel<'a>(&'a Value);

impl<'a> Serialize for ValueDataModel<'a> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.0 {
            Value::Null => serializer.serialize_unit(),
            Value::Bool(b) => serializer.serialize_bool(*b),
            Value::Int(i) => i.serialize(serializer),
            Value::Float(f) => serializer.serialize_f64(*f),
            Value::String(s) => serializer.serialize_str(s),
            Value::Sequence(items) => items.serialize(serializer),
            Value::Mapping(m) => m.serialize(serializer),
            Value::Tagged(t) => {
                struct ClearGuard;
                impl Drop for ClearGuard {
                    fn drop(&mut self) {
                        SER_TAG_SLOT.with(|s| {
                            let _ = s.borrow_mut().take();
                        });
                    }
                }
                let _guard = ClearGuard;
                SER_TAG_SLOT.with(|s| *s.borrow_mut() = Some(t.tag.clone()));
                serializer.serialize_newtype_struct(TAGGED_VALUE, &t.value)
            }
        }
    }
}

struct NodeBorrowWrapper<'a>(&'a Value);

impl<'a> Serialize for NodeBorrowWrapper<'a> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if let Some(st) = SER_ACTIVE_STATE.with(|s| s.borrow_mut().take()) {
            let slot_val = preflight_borrowed_value(self.0, &st).map(|()| self.0.clone());
            SER_VALUE_SLOT.with(|s| *s.borrow_mut() = Some(slot_val));
            serializer.serialize_unit()
        } else {
            ValueDataModel(self.0).serialize(serializer)
        }
    }
}

impl Serialize for Value {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        SER_ACTIVE_STATE.with(|s| {
            let _ = s.borrow_mut().take();
        });
        serializer.serialize_newtype_struct(NODE, &NodeBorrowWrapper(self))
    }
}

impl Serialize for Node {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.value.serialize(serializer)
    }
}

impl Serialize for Document {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.root.serialize(serializer)
    }
}
