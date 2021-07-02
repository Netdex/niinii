
use serde::{
    de::{self, IgnoredAny, SeqAccess, Visitor},
    Deserializer,
};
use std::fmt;

/// Special deserializer to handle `Counter::ordinal` which is either `[]` (false) or `bool`.
pub(crate) fn deserialize<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    struct BoolVisitor;
    impl<'de> Visitor<'de> for BoolVisitor {
        type Value = bool;
        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("[] or bool")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            match seq.next_element()? {
                Some(IgnoredAny) => Err(de::Error::invalid_length(0, &self)),
                None => Ok(false),
            }
        }
        fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(v)
        }
    }
    deserializer.deserialize_any(BoolVisitor)
}
