use serde::{
    de::{self, Visitor},
    Deserializer,
};
use std::fmt;

/// Special deserializer which removes zero-width non-joiner characters.
pub(crate) fn deserialize<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    struct StringVisitor;
    impl<'de> Visitor<'de> for StringVisitor {
        type Value = String;
        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("str")
        }
        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(v.replace('\u{200C}', ""))
        }
    }
    deserializer.deserialize_str(StringVisitor)
}
