pub mod as_secs {
  use std::time::Duration;

  use serde::de::{Deserialize, Deserializer};
  use serde::ser::Serializer;

  pub fn serialize<S>(value: &Duration, serializer: S) -> Result<S::Ok, S::Error>
  where
      S: Serializer,
  {
      serializer.serialize_f64(value.as_secs_f64())
  }

  pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
  where
      D: Deserializer<'de>,
  {
      let secs = f64::deserialize(deserializer)?;
      Ok(Duration::from_secs_f64(secs))
  }
}

pub mod as_secs_optional {
  use std::time::Duration;

  use serde::de::{Deserialize, Deserializer};
  use serde::ser::Serializer;

  pub fn serialize<S>(value: &Option<Duration>, serializer: S) -> Result<S::Ok, S::Error>
  where
      S: Serializer,
  {
    match value {
      Some(v) => serializer.serialize_f64(v.as_secs_f64()),
      _ => unreachable!(),
    }
  }

  pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
  where
      D: Deserializer<'de>,
  {
      let secs = f64::deserialize(deserializer)?;
      Ok(Some(Duration::from_secs_f64(secs)))
  }
}
