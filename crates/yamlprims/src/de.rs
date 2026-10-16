//! Strict typed deserialization over the owned tree (spec §8).

use crate::error::{Error, ErrorKind, LimitKind, PathSegment, RelatedRole, Site};
use crate::options::Options;
use crate::value::{Location, Node, NodeSpan, Span, Value, Via};
use serde::de::{self, DeserializeSeed, IntoDeserializer, Visitor};

#[derive(Clone, Copy)]
pub(crate) struct NodeDe<'a> {
    pub(crate) node: &'a Node,
    pub(crate) opts: &'a Options,
    pub(crate) depth: usize,
    pub(crate) last_known: Option<Location>,
    /// Nearest alias use site or merge directive above (or at) this node;
    /// errors report it in `related` (§7.2).
    pub(crate) origin: Option<(RelatedRole, Span)>,
}

impl<'a> NodeDe<'a> {
    pub(crate) fn root(node: &'a Node, opts: &'a Options) -> Self {
        NodeDe {
            node,
            opts,
            depth: 0,
            last_known: None,
            origin: origin_of(node, None),
        }
    }

    fn child(&self, node: &'a Node) -> NodeDe<'a> {
        let last_known = match node.span {
            NodeSpan::Source(s) => Some(s.start),
            NodeSpan::Synthetic => self.last_known,
        };
        NodeDe {
            node,
            opts: self.opts,
            depth: self.depth + 1,
            last_known,
            origin: origin_of(node, self.origin),
        }
    }

    fn site(&self) -> Site {
        match self.node.span {
            NodeSpan::Source(s) => Site::Source(s.start),
            NodeSpan::Synthetic => Site::NotSource {
                last_known: self.last_known,
            },
        }
    }

    /// Attach this node's site to errors raised through serde's constructors.
    fn fix(&self, mut e: Error) -> Error {
        if e.unsited {
            e.site = self.site();
            e.unsited = false;
            e = self.with_origin(e);
        }
        e
    }

    fn with_origin(&self, e: Error) -> Error {
        match self.origin {
            Some((role, span)) => e.with_related(role, span),
            None => e,
        }
    }

    fn err(&self, kind: ErrorKind, msg: impl std::fmt::Display) -> Error {
        self.with_origin(Error::new(kind, self.site(), msg))
    }

    fn type_err(&self, expected: &str) -> Error {
        let found = match &self.node.value {
            Value::Tagged(t) => format!("tagged {}", crate::error::bounded(&t.tag.uri, 256)),
            v => v.kind_name().to_owned(),
        };
        self.err(
            ErrorKind::Type {
                expected: expected.to_owned(),
                found: found.clone(),
            },
            format!("expected {expected}, found {found}"),
        )
    }

    fn depth_guard(&self) -> Result<(), Error> {
        if self.depth > self.opts.limits.max_depth {
            let configured = self.opts.limits.max_depth as u64;
            return Err(self.err(
                ErrorKind::LimitExceeded {
                    limit: LimitKind::MaxDepth,
                    configured,
                },
                "max_depth exceeded during conversion",
            ));
        }
        Ok(())
    }

    fn integer(&self, expected: &str) -> Result<&'a crate::value::Integer, Error> {
        match &self.node.value {
            Value::Int(i) => Ok(i),
            _ => Err(self.type_err(expected)),
        }
    }

    fn out_of_range(&self, target: &str) -> Error {
        self.err(
            ErrorKind::IntegerOutOfRange,
            format!("integer out of range for {target}"),
        )
    }
}

fn origin_of(node: &Node, inherited: Option<(RelatedRole, Span)>) -> Option<(RelatedRole, Span)> {
    match node.via() {
        Some(Via::Alias { use_site }) => Some((RelatedRole::AliasUse, *use_site)),
        Some(Via::Merge { directive }) => Some((RelatedRole::MergeDirective, *directive)),
        None => inherited,
    }
}

macro_rules! de_int {
    ($method:ident, $visit:ident, $t:ty) => {
        fn $method<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
            let i = self.integer(stringify!($t))?;
            let v: Option<$t> = if i.is_negative() {
                i.to_i128().and_then(|x| <$t>::try_from(x).ok())
            } else {
                i.to_u128().and_then(|x| <$t>::try_from(x).ok())
            };
            let v = v.ok_or_else(|| self.out_of_range(stringify!($t)))?;
            visitor.$visit(v).map_err(|e| self.fix(e))
        }
    };
}

impl<'de, 'a> de::Deserializer<'de> for NodeDe<'a> {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.depth_guard()?;
        let r = match &self.node.value {
            Value::Null => visitor.visit_unit(),
            Value::Bool(b) => visitor.visit_bool(*b),
            Value::Int(i) => {
                if let Some(u) = i.to_u64() {
                    visitor.visit_u64(u)
                } else if let Some(s) = i.to_i64() {
                    visitor.visit_i64(s)
                } else if let Some(u) = i.to_u128() {
                    visitor.visit_u128(u)
                } else {
                    visitor.visit_i128(i.to_i128().unwrap_or_default())
                }
            }
            Value::Float(f) => visitor.visit_f64(*f),
            Value::String(s) => visitor.visit_str(s),
            Value::Sequence(items) => visitor.visit_seq(SeqDe {
                de: self,
                items,
                i: 0,
            }),
            Value::Mapping(m) => visitor.visit_map(MapDe {
                de: self,
                entries: m.iter().collect(),
                i: 0,
            }),
            Value::Tagged(_) => return Err(self.type_err("untagged value")),
        };
        r.map_err(|e| self.fix(e))
    }

    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.node.value {
            Value::Bool(b) => visitor.visit_bool(b).map_err(|e| self.fix(e)),
            _ => Err(self.type_err("bool")),
        }
    }

    de_int!(deserialize_u8, visit_u8, u8);
    de_int!(deserialize_u16, visit_u16, u16);
    de_int!(deserialize_u32, visit_u32, u32);
    de_int!(deserialize_u64, visit_u64, u64);
    de_int!(deserialize_u128, visit_u128, u128);
    de_int!(deserialize_i8, visit_i8, i8);
    de_int!(deserialize_i16, visit_i16, i16);
    de_int!(deserialize_i32, visit_i32, i32);
    de_int!(deserialize_i64, visit_i64, i64);
    de_int!(deserialize_i128, visit_i128, i128);

    fn deserialize_f64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        let f = match &self.node.value {
            Value::Float(f) => *f,
            Value::Int(i) => i
                .to_f64_exact()
                .ok_or_else(|| self.type_err("f64 (integer not exactly representable)"))?,
            _ => return Err(self.type_err("f64")),
        };
        visitor.visit_f64(f).map_err(|e| self.fix(e))
    }

    fn deserialize_f32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        let r = match &self.node.value {
            // round to nearest (ties to even), reject only an infinite result.
            Value::Float(f) => {
                let r = *f as f32;
                if r.is_infinite() && f.is_finite() {
                    return Err(self.err(ErrorKind::FloatOutOfRange, "float out of range for f32"));
                }
                r
            }
            Value::Int(i) => {
                let f = i
                    .to_f64_exact()
                    .ok_or_else(|| self.type_err("f32 (integer not exactly representable)"))?;
                let r = f as f32;
                if !r.is_finite() || r as f64 != f {
                    return Err(self.type_err("f32 (integer not exactly representable)"));
                }
                r
            }
            _ => return Err(self.type_err("f32")),
        };
        visitor.visit_f32(r).map_err(|e| self.fix(e))
    }

    fn deserialize_char<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        if let Value::String(s) = &self.node.value {
            let mut it = s.chars();
            if let (Some(c), None) = (it.next(), it.next()) {
                return visitor.visit_char(c).map_err(|e| self.fix(e));
            }
        }
        Err(self.type_err("char"))
    }

    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match &self.node.value {
            Value::String(s) => visitor.visit_str(s).map_err(|e| self.fix(e)),
            _ => Err(self.type_err("string")),
        }
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_str(visitor)
    }

    fn deserialize_bytes<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value, Error> {
        Err(self.type_err("bytes (unsupported)"))
    }

    fn deserialize_byte_buf<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value, Error> {
        Err(self.type_err("bytes (unsupported)"))
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.node.value {
            Value::Null => visitor.visit_none().map_err(|e| self.fix(e)),
            _ => visitor.visit_some(self).map_err(|e| self.fix(e)),
        }
    }

    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.node.value {
            Value::Null => visitor.visit_unit().map_err(|e| self.fix(e)),
            _ => Err(self.type_err("null")),
        }
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        use crate::protocol::{NODE, NODE_SLOT, SPANNED, SPAN_SLOT, TAGGED_VALUE, TAG_SLOT};
        match name {
            // §8 private protocols: the visitor takes the slot first thing.
            NODE => {
                self.depth_guard()?;
                NODE_SLOT.with(|s| *s.borrow_mut() = Some(self.node.clone()));
            }
            SPANNED => SPAN_SLOT.with(|s| *s.borrow_mut() = Some(self.node.source_span())),
            TAGGED_VALUE => {
                let Value::Tagged(t) = &self.node.value else {
                    return Err(self.type_err("tagged value"));
                };
                TAG_SLOT.with(|s| *s.borrow_mut() = Some(t.tag.clone()));
                // The tag wrapper is not a depth level; the inner node keeps its own site.
                let inner = NodeDe {
                    depth: self.depth,
                    ..self.child(&t.value)
                };
                let r = visitor
                    .visit_newtype_struct(inner)
                    .map_err(|e| inner.fix(e));
                TAG_SLOT.with(|s| s.borrow_mut().take());
                return r;
            }
            _ => {}
        }
        let r = visitor.visit_newtype_struct(self).map_err(|e| self.fix(e));
        // A visitor that ignored its slot must not leak it into a later call.
        NODE_SLOT.with(|s| s.borrow_mut().take());
        SPAN_SLOT.with(|s| s.borrow_mut().take());
        r
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.depth_guard()?;
        match &self.node.value {
            Value::Sequence(items) => visitor
                .visit_seq(SeqDe {
                    de: self,
                    items,
                    i: 0,
                })
                .map_err(|e| self.fix(e)),
            _ => Err(self.type_err("sequence")),
        }
    }

    fn deserialize_tuple<V: Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value, Error> {
        match &self.node.value {
            Value::Sequence(items) if items.len() == len => self.deserialize_seq(visitor),
            _ => Err(self.type_err(&format!("sequence of length {len}"))),
        }
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.deserialize_tuple(len, visitor)
    }

    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.depth_guard()?;
        match &self.node.value {
            Value::Mapping(m) => visitor
                .visit_map(MapDe {
                    de: self,
                    entries: m.iter().collect(),
                    i: 0,
                })
                .map_err(|e| self.fix(e)),
            _ => Err(self.type_err("mapping")),
        }
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.deserialize_map(visitor)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.depth_guard()?;
        match &self.node.value {
            Value::String(s) => visitor
                .visit_enum(EnumDe {
                    de: self,
                    variant: s,
                    value: None,
                })
                .map_err(|e| self.fix(e)),
            Value::Mapping(m) if m.len() == 1 => {
                let (k, v) = m.iter().next().ok_or_else(|| self.type_err("enum"))?;
                match &k.value {
                    Value::String(s) => visitor
                        .visit_enum(EnumDe {
                            de: self,
                            variant: s,
                            value: Some(v),
                        })
                        .map_err(|e| self.fix(e)),
                    _ => Err(self.child(k).type_err("enum variant name")),
                }
            }
            // Local tags only: `!Variant value`.
            Value::Tagged(t) if t.tag.uri.starts_with('!') && !t.tag.uri.starts_with("!!") => {
                // a plain scalar payload of a variant-selecting local tag is
                // resolved with the Core schema before conversion.
                if let (true, Value::String(text)) = (t.inner_plain, &t.value.value) {
                    let v = crate::resolve::core_plain(text)
                        .map_err(|k| self.err(k, "cannot resolve variant payload"))?;
                    let resolved = Node {
                        value: v,
                        ..t.value.clone()
                    };
                    let d = NodeDe {
                        node: &resolved,
                        ..self.child(&t.value)
                    };
                    return visitor
                        .visit_enum(OwnedEnumDe {
                            variant: &t.tag.uri[1..],
                            value: d,
                        })
                        .map_err(|e| self.fix(e));
                }
                visitor
                    .visit_enum(EnumDe {
                        de: self,
                        variant: &t.tag.uri[1..],
                        value: Some(&t.value),
                    })
                    .map_err(|e| self.fix(e))
            }
            _ => Err(self.type_err("enum (string, single-key mapping or local tag)")),
        }
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_str(visitor)
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_unit()
    }
}

struct SeqDe<'a> {
    de: NodeDe<'a>,
    items: &'a [Node],
    i: usize,
}

impl<'de, 'a> de::SeqAccess<'de> for SeqDe<'a> {
    type Error = Error;
    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Error> {
        let Some(n) = self.items.get(self.i) else {
            return Ok(None);
        };
        let idx = self.i;
        self.i += 1;
        let child = self.de.child(n);
        seed.deserialize(child).map(Some).map_err(|e| {
            let mut e = child.fix(e);
            e.path.insert(0, PathSegment::Index(idx));
            e
        })
    }
    fn size_hint(&self) -> Option<usize> {
        Some(self.items.len() - self.i)
    }
}

struct MapDe<'a> {
    de: NodeDe<'a>,
    entries: Vec<(&'a Node, &'a Node)>,
    i: usize,
}

impl<'de, 'a> de::MapAccess<'de> for MapDe<'a> {
    type Error = Error;
    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Error> {
        let Some((k, _)) = self.entries.get(self.i) else {
            return Ok(None);
        };
        let child = self.de.child(k);
        seed.deserialize(child).map(Some).map_err(|e| {
            let mut e = child.fix(e);
            e.path
                .insert(0, PathSegment::Key(Box::new(k.value.clone())));
            e
        })
    }
    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, Error> {
        let (k, v) = self.entries[self.i];
        self.i += 1;
        let child = self.de.child(v);
        seed.deserialize(child).map_err(|e| {
            let mut e = child.fix(e);
            e.path
                .insert(0, PathSegment::Key(Box::new(k.value.clone())));
            e
        })
    }
    fn size_hint(&self) -> Option<usize> {
        Some(self.entries.len() - self.i)
    }
}

struct EnumDe<'a> {
    de: NodeDe<'a>,
    variant: &'a str,
    value: Option<&'a Node>,
}

impl<'de, 'a> de::EnumAccess<'de> for EnumDe<'a> {
    type Error = Error;
    type Variant = VariantDe<'a>;
    fn variant_seed<V: DeserializeSeed<'de>>(
        self,
        seed: V,
    ) -> Result<(V::Value, Self::Variant), Error> {
        let name: de::value::StrDeserializer<'_, Error> = self.variant.into_deserializer();
        let v = seed.deserialize(name).map_err(|e| self.de.fix(e))?;
        Ok((
            v,
            VariantDe {
                de: self.de,
                value: self.value,
                direct: false,
            },
        ))
    }
}

struct VariantDe<'a> {
    de: NodeDe<'a>,
    value: Option<&'a Node>,
    /// `de` already points at the payload node.
    direct: bool,
}

impl<'a> VariantDe<'a> {
    fn value_de(&self) -> Result<NodeDe<'a>, Error> {
        match self.value {
            Some(_) if self.direct => Ok(self.de),
            Some(v) => Ok(self.de.child(v)),
            None => Err(self.de.type_err("enum variant with a value")),
        }
    }
}

impl<'de, 'a> de::VariantAccess<'de> for VariantDe<'a> {
    type Error = Error;
    fn unit_variant(self) -> Result<(), Error> {
        match self.value {
            None => Ok(()),
            Some(v) if matches!(v.value, Value::Null) => Ok(()),
            Some(v) => Err(self.de.child(v).type_err("unit variant")),
        }
    }
    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value, Error> {
        let d = self.value_de()?;
        seed.deserialize(d).map_err(|e| d.fix(e))
    }
    fn tuple_variant<V: Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value, Error> {
        de::Deserializer::deserialize_tuple(self.value_de()?, len, visitor)
    }
    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        de::Deserializer::deserialize_struct(self.value_de()?, "", fields, visitor)
    }
}

/// Enum access over a node that lives only for this call ( resolved payload).
struct OwnedEnumDe<'v, 'n> {
    variant: &'v str,
    value: NodeDe<'n>,
}

impl<'de, 'v, 'n> de::EnumAccess<'de> for OwnedEnumDe<'v, 'n> {
    type Error = Error;
    type Variant = VariantDe<'n>;
    fn variant_seed<V: DeserializeSeed<'de>>(
        self,
        seed: V,
    ) -> Result<(V::Value, Self::Variant), Error> {
        let name: de::value::StrDeserializer<'_, Error> = self.variant.into_deserializer();
        let v = seed.deserialize(name).map_err(|e| self.value.fix(e))?;
        Ok((
            v,
            VariantDe {
                de: self.value,
                value: Some(self.value.node),
                direct: true,
            },
        ))
    }
}
