use lasso::{Spur, ThreadedRodeo};
use once_cell::sync::Lazy;
use std::{borrow::Borrow, cmp::Ordering, ffi::OsStr, fmt, hash, ops::Deref, sync::Arc};

pub(crate) static STRING_INTERNER: Lazy<Arc<ThreadedRodeo>> =
  Lazy::new(|| Arc::new(Default::default()));

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct InternedString(Spur);

impl InternedString {
  pub fn new<T>(text: T) -> InternedString
  where
    T: AsRef<str>,
  {
    InternedString(STRING_INTERNER.get_or_intern(text))
  }

  pub fn new_static(text: &'static str) -> InternedString {
    InternedString(STRING_INTERNER.get_or_intern_static(text))
  }

  #[inline(always)]
  pub fn as_str(&self) -> &str {
    &*self
  }

  #[inline(always)]
  #[allow(clippy::wrong_self_convention)]
  #[allow(clippy::inherent_to_string_shadow_display)]
  pub fn to_string(&self) -> String {
    self.as_str().to_string()
  }

  #[inline(always)]
  pub fn len(&self) -> usize {
    self.as_str().len()
  }

  #[inline(always)]
  pub fn is_empty(&self) -> bool {
    self.as_str().is_empty()
  }
}

impl Default for InternedString {
  fn default() -> Self {
    InternedString::new_static("")
  }
}

impl Deref for InternedString {
  type Target = str;

  fn deref(&self) -> &str {
    STRING_INTERNER.resolve(&self.0)
  }
}

impl PartialEq<InternedString> for InternedString {
  fn eq(&self, other: &InternedString) -> bool {
    self.0 == other.0 || self.as_str() == other.as_str()
  }
}

impl Eq for InternedString {}

impl PartialEq<str> for InternedString {
  fn eq(&self, other: &str) -> bool {
    self.as_str() == other
  }
}

impl PartialEq<InternedString> for str {
  fn eq(&self, other: &InternedString) -> bool {
    other == self
  }
}

impl<'a> PartialEq<&'a str> for InternedString {
  fn eq(&self, other: &&'a str) -> bool {
    self == *other
  }
}

impl<'a> PartialEq<InternedString> for &'a str {
  fn eq(&self, other: &InternedString) -> bool {
    *self == other
  }
}

impl PartialEq<String> for InternedString {
  fn eq(&self, other: &String) -> bool {
    self.as_str() == other
  }
}

impl PartialEq<InternedString> for String {
  fn eq(&self, other: &InternedString) -> bool {
    other == self
  }
}

impl<'a> PartialEq<&'a String> for InternedString {
  fn eq(&self, other: &&'a String) -> bool {
    self == *other
  }
}

impl<'a> PartialEq<InternedString> for &'a String {
  fn eq(&self, other: &InternedString) -> bool {
    *self == other
  }
}

impl Ord for InternedString {
  fn cmp(&self, other: &InternedString) -> Ordering {
    self.as_str().cmp(other.as_str())
  }
}

impl PartialOrd for InternedString {
  fn partial_cmp(&self, other: &InternedString) -> Option<Ordering> {
    Some(self.cmp(other))
  }
}

impl hash::Hash for InternedString {
  fn hash<H: hash::Hasher>(&self, hasher: &mut H) {
    self.as_str().hash(hasher)
  }
}

impl fmt::Debug for InternedString {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    fmt::Debug::fmt(self.as_str(), f)
  }
}

impl fmt::Display for InternedString {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    fmt::Display::fmt(self.as_str(), f)
  }
}

impl<T> From<T> for InternedString
where
  T: Into<String> + AsRef<str>,
{
  fn from(text: T) -> Self {
    Self::new(text)
  }
}

impl From<InternedString> for String {
  fn from(text: InternedString) -> Self {
    text.as_str().into()
  }
}

impl Borrow<str> for InternedString {
  fn borrow(&self) -> &str {
    self.as_str()
  }
}

impl AsRef<OsStr> for InternedString {
  #[inline]
  fn as_ref(&self) -> &OsStr {
    (&*self).as_ref()
  }
}

mod serde {
  use super::InternedString;
  use serde::{
    de::Error,
    de::{Unexpected, Visitor},
    Deserialize, Deserializer, Serialize, Serializer,
  };

  impl Serialize for InternedString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
      S: Serializer,
    {
      self.as_str().serialize(serializer)
    }
  }

  struct InternedStringVisitor;
  impl<'de> Visitor<'de> for InternedStringVisitor {
    type Value = InternedString;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
      formatter.write_str("a string")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
      E: Error,
    {
      Ok(InternedString::from(v))
    }

    fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
    where
      E: Error,
    {
      match std::str::from_utf8(v) {
        Ok(s) => Ok(InternedString::from(s)),
        Err(_) => Err(Error::invalid_value(Unexpected::Bytes(v), &self)),
      }
    }
  }

  impl<'de> Deserialize<'de> for InternedString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
      D: Deserializer<'de>,
    {
      deserializer.deserialize_str(InternedStringVisitor)
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_test::{assert_tokens, Token};

  #[test]
  fn interned_str_serde() {
    assert_tokens(&InternedString::new_static("foo"), &[Token::Str("foo")]);
  }
}
