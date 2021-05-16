use super::InternedString;
use serde::{
  de::{Error, MapAccess, Visitor},
  ser::SerializeStruct,
  Deserialize, Deserializer, Serialize, Serializer,
};
use smallvec::SmallVec;
use std::{collections::BTreeMap, fmt, marker::PhantomData};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "operator", content = "values")]
pub enum SelectorValueRequirement {
  /// Require that the value is one of a set of values
  In(SmallVec<[InternedString; 2]>),

  /// Require that the value is not one of a set of values
  NotIn(SmallVec<[InternedString; 2]>),

  /// Require that a value exists
  Exists,

  /// Require that a value does not exist
  DoesNotExist,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SelectorRequirement {
  /// The attribute key that the selector applies to.
  pub key: InternedString,

  /// Represents a key's relationship to a set of values.
  #[serde(flatten)]
  pub value_requirement: SelectorValueRequirement,
}

pub trait SelectorType {
  const FLAT_KEYS_NAME: Option<&'static str>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct Selector<T: SelectorType> {
  flat: Option<BTreeMap<InternedString, InternedString>>,
  expressions: Option<Vec<SelectorRequirement>>,
  marker: PhantomData<T>,
}

mod ser_de {
  use std::{
    collections::{hash_map::Entry, HashMap},
    sync::Mutex,
  };

  use once_cell::sync::Lazy;
  use serde::de::{IgnoredAny, SeqAccess};

  use super::*;

  const MATCH_EXPRESSIONS_KEY: &str = "matchExpressions";

  impl<T: SelectorType> Serialize for Selector<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
      S: Serializer,
    {
      let has_flat_keys = T::FLAT_KEYS_NAME.is_some();
      let num_keys = if has_flat_keys { 2usize } else { 1usize };

      let mut map = serializer.serialize_struct(stringify!(Selector), num_keys)?;
      if let Some(flat_keys_name) = T::FLAT_KEYS_NAME {
        match self.flat.as_ref() {
          None => map.skip_field(flat_keys_name)?,
          Some(v) => map.serialize_field(flat_keys_name, v)?,
        }
      }

      match self.expressions.as_deref() {
        None => map.skip_field(MATCH_EXPRESSIONS_KEY)?,
        Some(v) => map.serialize_field(MATCH_EXPRESSIONS_KEY, v)?,
      }

      map.end()
    }
  }

  enum Field<T: SelectorType> {
    Flat,
    Expressions,
    Ignore(PhantomData<T>),
  }

  struct FieldVisitor<T: SelectorType>(PhantomData<T>);
  impl<'de, T: SelectorType> Visitor<'de> for FieldVisitor<T> {
    type Value = Field<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
      formatter.write_str("field identifier")
    }

    fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
    where
      E: Error,
    {
      if T::FLAT_KEYS_NAME.is_some() {
        match v {
          0u64 => Ok(Field::Flat),
          1u64 => Ok(Field::Expressions),
          _ => Ok(Field::Ignore(PhantomData)),
        }
      } else {
        match v {
          0u64 => Ok(Field::Expressions),
          _ => Ok(Field::Ignore(PhantomData)),
        }
      }
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
      E: Error,
    {
      if v == MATCH_EXPRESSIONS_KEY {
        Ok(Field::Expressions)
      } else if Some(v) == T::FLAT_KEYS_NAME {
        Ok(Field::Flat)
      } else {
        Ok(Field::Ignore(PhantomData))
      }
    }

    fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
    where
      E: Error,
    {
      if v == MATCH_EXPRESSIONS_KEY.as_bytes() {
        Ok(Field::Expressions)
      } else if Some(v) == T::FLAT_KEYS_NAME.map(|v| v.as_bytes()) {
        Ok(Field::Flat)
      } else {
        Ok(Field::Ignore(PhantomData))
      }
    }
  }
  impl<'de, T: SelectorType> Deserialize<'de> for Field<T> {
    #[inline]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
      D: Deserializer<'de>,
    {
      deserializer.deserialize_identifier(FieldVisitor::<T>(PhantomData))
    }
  }

  struct SelectorVisitor<'de, T: SelectorType> {
    marker: PhantomData<Selector<T>>,
    lifetime: PhantomData<&'de ()>,
  }
  impl<'de, T: SelectorType> Visitor<'de> for SelectorVisitor<'de, T> {
    type Value = Selector<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
      formatter.write_str("struct Selector")
    }

    #[inline]
    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
      A: SeqAccess<'de>,
    {
      if T::FLAT_KEYS_NAME.is_some() {
        let flat = match seq.next_element()? {
          Some(v) => v,
          None => None,
        };

        let expressions = match seq.next_element()? {
          Some(v) => v,
          None => None,
        };

        Ok(Selector {
          flat,
          expressions,
          marker: PhantomData,
        })
      } else {
        let expressions = match seq.next_element()? {
          Some(v) => v,
          None => None,
        };

        Ok(Selector {
          flat: None,
          expressions,
          marker: PhantomData,
        })
      }
    }

    #[inline]
    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
      A: MapAccess<'de>,
    {
      let mut flat: Option<Option<BTreeMap<InternedString, InternedString>>> = None;
      let mut expressions: Option<Option<Vec<SelectorRequirement>>> = None;

      while let Some(key) = map.next_key::<Field<T>>()? {
        match key {
          Field::Flat => {
            if flat.is_some() {
              // SAFETY: we know that FLAT_KEYS_NAME is not none here, cause we got a Field::Flat
              return Err(<A::Error as Error>::duplicate_field(
                T::FLAT_KEYS_NAME.unwrap(),
              ));
            }

            flat = Some(map.next_value()?);
          }
          Field::Expressions => {
            if expressions.is_some() {
              // SAFETY: we know that FLAT_KEYS_NAME is not none here, cause we got a Field::Flat
              return Err(<A::Error as Error>::duplicate_field(
                T::FLAT_KEYS_NAME.unwrap(),
              ));
            }

            expressions = Some(map.next_value()?);
          }

          _ => {
            let _: Option<IgnoredAny> = map.next_value()?;
          }
        }
      }

      let flat = flat.unwrap_or_default();
      let expressions = expressions.unwrap_or_default();
      Ok(Selector {
        flat,
        expressions,
        marker: PhantomData,
      })
    }
  }

  static FIELD_CACHES: Lazy<Mutex<HashMap<&'static str, &'static [&'static str]>>> =
    Lazy::new(Default::default);

  fn get_field_names(flat_field_name: &'static str) -> &'static [&'static str] {
    let mut lock = FIELD_CACHES.lock().unwrap();
    match lock.entry(flat_field_name) {
      Entry::Occupied(v) => *v.get(),
      Entry::Vacant(entry) => {
        let arr = [flat_field_name, MATCH_EXPRESSIONS_KEY];
        let boxed: Box<[&'static str]> = Box::new(arr);
        let leaked: &'static [&'static str] = Box::leak(boxed);
        entry.insert(leaked);
        leaked
      }
    }
  }

  impl<'de, T: SelectorType> Deserialize<'de> for Selector<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
      D: Deserializer<'de>,
    {
      let visitor = SelectorVisitor::<'de, T> {
        marker: PhantomData,
        lifetime: PhantomData,
      };
      if let Some(flat_field_name) = T::FLAT_KEYS_NAME {
        let fields = get_field_names(flat_field_name);
        deserializer.deserialize_struct("Selector", fields, visitor)
      } else {
        deserializer.deserialize_struct("Selector", &[MATCH_EXPRESSIONS_KEY], visitor)
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_test::{assert_tokens, Token};
  use smallvec::smallvec;

  #[derive(Debug, Serialize, Deserialize, PartialEq)]
  struct LabelsSelector {
    #[serde(flatten)]
    selector: Selector<Self>,
    extra: InternedString,
  }

  impl SelectorType for LabelsSelector {
    const FLAT_KEYS_NAME: Option<&'static str> = Some("labels");
  }

  #[test]
  fn labels_selector_serde() {
    assert_tokens(
      &LabelsSelector {
        selector: Selector {
          flat: Some(
            std::array::IntoIter::new([("type", "radio")])
              .map(|(k, v)| (InternedString::new_static(k), InternedString::new_static(v)))
              .collect(),
          ),
          expressions: Some(vec![
            SelectorRequirement {
              key: InternedString::new_static("idVendor"),
              value_requirement: SelectorValueRequirement::Exists,
            },
            SelectorRequirement {
              key: InternedString::new_static("idProduct"),
              value_requirement: SelectorValueRequirement::NotIn(smallvec![
                InternedString::new_static("0030"),
                InternedString::new_static("DE2422340"),
              ]),
            },
          ]),
          marker: PhantomData,
        },
        extra: InternedString::new_static("foo"),
      },
      &[
        Token::Map { len: None },
        // labels field
        Token::Str("labels"),
        Token::Map { len: Some(1) },
        Token::Str("type"),
        Token::Str("radio"),
        Token::MapEnd,
        // expressions field
        Token::Str("matchExpressions"),
        Token::Seq { len: Some(2) },
        Token::Map { len: None }, // start SelectorValueRequirement
        Token::Str("key"),
        Token::Str("idVendor"),
        Token::Str("operator"),
        Token::Str("Exists"),
        Token::MapEnd,            // end SelectorValueRequirement
        Token::Map { len: None }, // start SelectorValueRequirement
        Token::Str("key"),
        Token::Str("idProduct"),
        Token::Str("operator"),
        Token::Str("NotIn"),
        Token::Str("values"),
        Token::Seq { len: Some(2) },
        Token::Str("0030"),
        Token::Str("DE2422340"),
        Token::SeqEnd,
        Token::MapEnd, // end SelectorValueRequirement
        Token::SeqEnd,
        // extra field
        Token::Str("extra"),
        Token::Str("foo"),
        Token::MapEnd,
      ],
    )
  }
}
