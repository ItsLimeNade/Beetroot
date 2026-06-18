# cinnamon: `StatusSettings` alarm minutes fields

## Problem

Some Nightscout instances return alarm-minute values as JSON strings (`"15"`) instead of integers (`15`). The `StatusSettings` struct in `cinnamon` types these as `Vec<i64>`, causing serde to fail with:

```
invalid type: string "15", expected i64
```

Affected fields in `StatusSettings`:

- `alarm_urgent_high_mins: Option<Vec<i64>>`
- `alarm_high_mins: Option<Vec<i64>>`
- `alarm_low_mins: Option<Vec<i64>>`
- `alarm_urgent_low_mins: Option<Vec<i64>>`
- `alarm_urgent_mins: Option<Vec<i64>>`
- `alarm_warn_mins: Option<Vec<i64>>`

## Fix

Add a helper deserializer that accepts both forms and annotate each field with it:

```rust
fn deserialize_string_or_i64_vec<'de, D>(deserializer: D) -> Result<Option<Vec<i64>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{SeqAccess, Visitor};

    struct Visitor;
    impl<'de> serde::de::Visitor<'de> for Visitor {
        type Value = Option<Vec<i64>>;

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(f, "null, or a sequence of integers or integer-strings")
        }

        fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_some<D: serde::Deserializer<'de>>(
            self,
            d: D,
        ) -> Result<Self::Value, D::Error> {
            d.deserialize_seq(SeqItemVisitor).map(Some)
        }
    }

    struct SeqItemVisitor;
    impl<'de> serde::de::Visitor<'de> for SeqItemVisitor {
        type Value = Vec<i64>;

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(f, "a sequence of integers or integer-strings")
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut out = Vec::new();
            while let Some(v) = seq.next_element::<serde_json::Value>()? {
                let n = match &v {
                    serde_json::Value::Number(n) => n
                        .as_i64()
                        .ok_or_else(|| serde::de::Error::custom("expected i64"))?,
                    serde_json::Value::String(s) => s
                        .parse::<i64>()
                        .map_err(serde::de::Error::custom)?,
                    _ => return Err(serde::de::Error::custom("expected integer or string")),
                };
                out.push(n);
            }
            Ok(out)
        }
    }

    deserializer.deserialize_option(Visitor)
}
```

Then on each affected field:

```rust
#[serde(default, deserialize_with = "deserialize_string_or_i64_vec")]
pub alarm_urgent_high_mins: Option<Vec<i64>>,
```
