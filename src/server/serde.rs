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
