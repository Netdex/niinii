use serde::{
    de::{self, IgnoredAny, SeqAccess, Visitor},
    ser::SerializeSeq,
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::{fmt, marker::PhantomData};

/// Special deserializer deriving an `Option` from a seq of length 0 or 1.
pub(crate) fn deserialize<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct OptionVisitor<T> {
        marker: PhantomData<T>,
    }
    impl<'de, T: Deserialize<'de>> Visitor<'de> for OptionVisitor<T> {
        type Value = Option<T>;
        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("seq of len 0 or 1")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let val: T = seq
                .next_element()?
                .ok_or_else(|| de::Error::invalid_length(0, &self))?;
            match seq.next_element()? {
                Some(IgnoredAny) => Err(de::Error::invalid_length(1, &self)),
                None => Ok(Some(val)),
            }
        }
    }
    deserializer.deserialize_seq(OptionVisitor {
        marker: PhantomData,
    })
}

/// Special serializer deriving an `Option` from a seq of length 0 or 1.
pub(crate) fn serialize<S, T>(value: &Option<T>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: Serialize,
{
    let mut seq = serializer.serialize_seq(None)?;
    if let Some(some) = value {
        seq.serialize_element(some)?;
    }
    seq.end()
}
