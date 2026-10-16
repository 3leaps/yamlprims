//! Private serde protocols for `Node`, `Value`, `Spanned<T>` and `TaggedValue<T>` (spec §8).
//!
//! Each type asks its deserializer for a newtype struct with a reserved name. This crate's
//! deserializer recognizes the name, parks the owned data in a thread-local slot and calls
//! `visit_newtype_struct`; the visitor takes the slot before doing anything else, so nested
//! uses cannot see each other's data. Through a foreign deserializer the slot stays empty and
//! the types degrade: `Node`/`Value` are rebuilt from the data model as synthetic nodes,
//! `Spanned` gets `span: None`, and `TaggedValue` is an error (there is no tag to report).

use crate::value::Integer;
use crate::value::{Mapping, Node, Span, Tag, Value};
use serde::de::{self, Deserialize, Deserializer, IntoDeserializer, MapAccess, SeqAccess, Visitor};
use std::cell::RefCell;
use std::fmt;

pub(crate) const NODE: &str = "$yamlprims::Node";
pub(crate) const SPANNED: &str = "$yamlprims::Spanned";
pub(crate) const TAGGED_VALUE: &str = "$yamlprims::TaggedValue";

thread_local! {
    pub(crate) static NODE_SLOT: RefCell<Option<Node>> = const { RefCell::new(None) };
    pub(crate) static SPAN_SLOT: RefCell<Option<Option<Span>>> = const { RefCell::new(None) };
    pub(crate) static TAG_SLOT: RefCell<Option<Tag>> = const { RefCell::new(None) };
}

/// A value together with the source span of the node it came from (`None` for synthetic nodes
/// or foreign deserializers). Transparent: the node, tagged or not, goes to `T`.
#[derive(Clone, Debug, PartialEq)]
pub struct Spanned<T> {
    pub value: T,
    pub span: Option<Span>,
}

/// A tagged node's tag and its inner value converted by `T`'s rules. Untagged nodes are a
/// `Type` error. The inner value of a non-core tag is the raw String (§4.4), so
/// `TaggedValue<u32>` rejects `!x 7`; only enum variant selection re-resolves.
#[derive(Clone, Debug, PartialEq)]
pub struct TaggedValue<T> {
    pub tag: Tag,
    pub value: T,
}

impl<'de> Deserialize<'de> for Node {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Node, D::Error> {
        d.deserialize_newtype_struct(NODE, NodeVisitor)
    }
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Value, D::Error> {
        Node::deserialize(d).map(|n| n.value)
    }
}

struct NodeVisitor;

impl<'de> Visitor<'de> for NodeVisitor {
    type Value = Node;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("any YAML value")
    }
    fn visit_newtype_struct<D: Deserializer<'de>>(self, d: D) -> Result<Node, D::Error> {
        match NODE_SLOT.with(|s| s.borrow_mut().take()) {
            Some(n) => Ok(n),
            None => d.deserialize_any(ForeignVisitor).map(Node::synthetic),
        }
    }
    // Foreign deserializers that do not wrap newtypes call these directly; every
    // data-model visit ForeignVisitor accepts is accepted here too.
    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Node, E> {
        ForeignVisitor.visit_bool(v).map(Node::synthetic)
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Node, E> {
        ForeignVisitor.visit_i64(v).map(Node::synthetic)
    }
    fn visit_i128<E: de::Error>(self, v: i128) -> Result<Node, E> {
        ForeignVisitor.visit_i128(v).map(Node::synthetic)
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Node, E> {
        ForeignVisitor.visit_u64(v).map(Node::synthetic)
    }
    fn visit_u128<E: de::Error>(self, v: u128) -> Result<Node, E> {
        ForeignVisitor.visit_u128(v).map(Node::synthetic)
    }
    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Node, E> {
        ForeignVisitor.visit_f64(v).map(Node::synthetic)
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Node, E> {
        ForeignVisitor.visit_str(v).map(Node::synthetic)
    }
    fn visit_string<E: de::Error>(self, v: String) -> Result<Node, E> {
        ForeignVisitor.visit_string(v).map(Node::synthetic)
    }
    fn visit_unit<E: de::Error>(self) -> Result<Node, E> {
        Ok(Node::synthetic(Value::Null))
    }
    fn visit_none<E: de::Error>(self) -> Result<Node, E> {
        Ok(Node::synthetic(Value::Null))
    }
    fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<Node, D::Error> {
        d.deserialize_any(ForeignVisitor).map(Node::synthetic)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, a: A) -> Result<Node, A::Error> {
        ForeignVisitor.visit_seq(a).map(Node::synthetic)
    }
    fn visit_map<A: MapAccess<'de>>(self, a: A) -> Result<Node, A::Error> {
        ForeignVisitor.visit_map(a).map(Node::synthetic)
    }
}

/// Builds a synthetic value from any self-describing data model.
struct ForeignVisitor;

impl<'de> Visitor<'de> for ForeignVisitor {
    type Value = Value;
    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("any value")
    }
    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Value, E> {
        Ok(Value::Int(Integer::from_i128(v.into())))
    }
    fn visit_i128<E: de::Error>(self, v: i128) -> Result<Value, E> {
        Ok(Value::Int(Integer::from_i128(v)))
    }
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Value, E> {
        Ok(Value::Int(Integer::from_u128(v.into())))
    }
    fn visit_u128<E: de::Error>(self, v: u128) -> Result<Value, E> {
        Ok(Value::Int(Integer::from_u128(v)))
    }
    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Value, E> {
        Ok(Value::Float(v))
    }
    fn visit_str<E: de::Error>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(v.to_owned()))
    }
    fn visit_string<E: de::Error>(self, v: String) -> Result<Value, E> {
        Ok(Value::String(v))
    }
    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_none<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<Value, D::Error> {
        d.deserialize_any(self)
    }
    fn visit_newtype_struct<D: Deserializer<'de>>(self, d: D) -> Result<Value, D::Error> {
        d.deserialize_any(self)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Value, A::Error> {
        let mut items = Vec::new();
        while let Some(n) = a.next_element::<Node>()? {
            items.push(n);
        }
        Ok(Value::Sequence(items))
    }
    fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Value, A::Error> {
        let mut m = Mapping::new();
        while let Some((k, v)) = a.next_entry::<Node, Node>()? {
            m.insert_new(k, v)
                .map_err(|e| de::Error::custom(format!("invalid mapping key: {e:?}")))?;
        }
        Ok(Value::Mapping(m))
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Spanned<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V<T>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for V<T> {
            type Value = Spanned<T>;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a value")
            }
            fn visit_newtype_struct<D: Deserializer<'de>>(
                self,
                d: D,
            ) -> Result<Spanned<T>, D::Error> {
                // Take the span before T runs, so a nested Spanned sees only its own.
                let span = SPAN_SLOT.with(|s| s.borrow_mut().take()).flatten();
                Ok(Spanned {
                    value: T::deserialize(d)?,
                    span,
                })
            }
            // Foreign deserializers may forward the value itself instead of a newtype.
            // These never touch the slot: only this crate's deserializer sets it, and it
            // always calls `visit_newtype_struct`.
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Spanned<T>, E> {
                foreign(v.into_deserializer())
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Spanned<T>, E> {
                foreign(v.into_deserializer())
            }
            fn visit_i128<E: de::Error>(self, v: i128) -> Result<Spanned<T>, E> {
                foreign(v.into_deserializer())
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Spanned<T>, E> {
                foreign(v.into_deserializer())
            }
            fn visit_u128<E: de::Error>(self, v: u128) -> Result<Spanned<T>, E> {
                foreign(v.into_deserializer())
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Spanned<T>, E> {
                foreign(v.into_deserializer())
            }
            fn visit_char<E: de::Error>(self, v: char) -> Result<Spanned<T>, E> {
                foreign(v.into_deserializer())
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Spanned<T>, E> {
                foreign(v.into_deserializer())
            }
            fn visit_borrowed_str<E: de::Error>(self, v: &'de str) -> Result<Spanned<T>, E> {
                foreign(de::value::BorrowedStrDeserializer::new(v))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Spanned<T>, E> {
                foreign(v.into_deserializer())
            }
            fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Spanned<T>, E> {
                foreign(de::value::BytesDeserializer::new(v))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Spanned<T>, E> {
                foreign(().into_deserializer())
            }
            fn visit_none<E: de::Error>(self) -> Result<Spanned<T>, E> {
                foreign(NoneDeserializer(std::marker::PhantomData))
            }
            fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<Spanned<T>, D::Error> {
                foreign(SomeDeserializer(d))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, a: A) -> Result<Spanned<T>, A::Error> {
                foreign(de::value::SeqAccessDeserializer::new(a))
            }
            fn visit_map<A: MapAccess<'de>>(self, a: A) -> Result<Spanned<T>, A::Error> {
                foreign(de::value::MapAccessDeserializer::new(a))
            }
            fn visit_enum<A: de::EnumAccess<'de>>(self, a: A) -> Result<Spanned<T>, A::Error> {
                foreign(de::value::EnumAccessDeserializer::new(a))
            }
        }
        fn foreign<'de, T: Deserialize<'de>, D: Deserializer<'de>>(
            d: D,
        ) -> Result<Spanned<T>, D::Error> {
            Ok(Spanned {
                value: T::deserialize(d)?,
                span: None,
            })
        }
        d.deserialize_newtype_struct(SPANNED, V(std::marker::PhantomData))
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for TaggedValue<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V<T>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for V<T> {
            type Value = TaggedValue<T>;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a tagged value")
            }
            fn visit_newtype_struct<D: Deserializer<'de>>(
                self,
                d: D,
            ) -> Result<TaggedValue<T>, D::Error> {
                let tag = TAG_SLOT.with(|s| s.borrow_mut().take());
                let Some(tag) = tag else {
                    return Err(de::Error::custom(
                        "TaggedValue needs the yamlprims deserializer",
                    ));
                };
                Ok(TaggedValue {
                    tag,
                    value: T::deserialize(d)?,
                })
            }
        }
        d.deserialize_newtype_struct(TAGGED_VALUE, V(std::marker::PhantomData))
    }
}

/// Replays a foreign `visit_none` to the wrapped type.
struct NoneDeserializer<E>(std::marker::PhantomData<E>);

impl<'de, E: de::Error> Deserializer<'de> for NoneDeserializer<E> {
    type Error = E;
    fn deserialize_any<V: Visitor<'de>>(self, v: V) -> Result<V::Value, E> {
        v.visit_none()
    }
    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf option
        unit unit_struct newtype_struct seq tuple tuple_struct map struct enum identifier ignored_any
    }
}

/// Replays a foreign `visit_some`: an `Option` target sees `Some`, any other target sees
/// the inner value (through `deserialize_any`, so only self-describing formats apply).
struct SomeDeserializer<D>(D);

impl<'de, D: Deserializer<'de>> Deserializer<'de> for SomeDeserializer<D> {
    type Error = D::Error;
    fn deserialize_any<V: Visitor<'de>>(self, v: V) -> Result<V::Value, D::Error> {
        self.0.deserialize_any(v)
    }
    fn deserialize_option<V: Visitor<'de>>(self, v: V) -> Result<V::Value, D::Error> {
        v.visit_some(self.0)
    }
    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf
        unit unit_struct newtype_struct seq tuple tuple_struct map struct enum identifier ignored_any
    }
}
