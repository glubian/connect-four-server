use std::time::Duration;

use serde::de::{Deserialize, Deserializer};
use serde::ser::Serializer;

pub mod as_secs {
    use super::*;

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
    use super::*;

    pub fn serialize<S>(value: &Option<Duration>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_f64(value.unwrap().as_secs_f64())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = f64::deserialize(deserializer)?;
        Ok(Some(Duration::from_secs_f64(secs)))
    }
}

pub mod as_millis_optional_tuple {
    use super::*;

    const MILLIS: f64 = 1000.0;

    pub fn serialize<S>(value: &Option<[Duration; 2]>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_seq(value.unwrap().iter().map(|d| d.as_secs_f64() * MILLIS))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<[Duration; 2]>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v: [f64; 2] = Deserialize::deserialize(deserializer)?;
        Ok(Some(v.map(|v| Duration::from_secs_f64(v / MILLIS))))
    }
}
