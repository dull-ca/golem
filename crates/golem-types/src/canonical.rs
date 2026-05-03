//! Canonical JSON for signing.
//!
//! Why this exists: `ed25519` is a deterministic signature scheme over an exact
//! byte string. If `golemctl` and `golemd` disagree by a single byte on what
//! "the bundle" is, every signature fails. The previous scaffold relied on
//! `serde_json::Value`'s map type sorting keys — which it does, *unless* any
//! crate in the dep graph enables `serde_json/preserve_order`. That's a
//! footgun-by-feature: turn it on for unrelated reasons, signatures break.
//!
//! This module avoids that by walking the value ourselves and emitting sorted
//! keys explicitly. The output is independent of which `serde_json` features
//! are active anywhere in the workspace.
//!
//! Format choices:
//!   - Objects emit keys in lexicographic byte order.
//!   - No whitespace anywhere (compact form).
//!   - Strings are JSON-escaped via `serde_json::to_string` (handles unicode,
//!     control chars, quotes, backslashes consistently).
//!   - Numbers preserve `serde_json::Number`'s display form. Since all of
//!     Golem's wire-format numbers come from typed Rust ints (u64 version,
//!     u32 mode, etc.), they have a single canonical decimal form.
//!   - Null / bool emit as `null` / `true` / `false`.

use serde::Serialize;
use serde_json::Value;

/// Serialize any `Serialize` value to canonical JSON bytes.
///
/// The result is deterministic: two equivalent inputs (modulo object key
/// order) produce byte-equal output regardless of crate features.
pub fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>, serde_json::Error> {
    let v = serde_json::to_value(value)?;
    let mut out = String::new();
    write_canonical(&v, &mut out);
    Ok(out.into_bytes())
}

fn write_canonical(v: &Value, out: &mut String) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(n) => out.push_str(&n.to_string()),
        Value::String(s) => {
            // serde_json::to_string handles all JSON string escaping for us.
            // Unwrap is safe: a String can always be serialized as a JSON string.
            let escaped = serde_json::to_string(s).expect("string serialization is infallible");
            out.push_str(&escaped);
        }
        Value::Array(arr) => {
            out.push('[');
            for (i, item) in arr.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            // Sort keys lexicographically. This is the load-bearing line —
            // the whole reason this module exists.
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort_unstable();
            out.push('{');
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                let escaped = serde_json::to_string(k).expect("string serialization is infallible");
                out.push_str(&escaped);
                out.push(':');
                write_canonical(&map[*k], out);
            }
            out.push('}');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn primitives() {
        assert_eq!(canonical_json(&Value::Null).unwrap(), b"null");
        assert_eq!(canonical_json(&true).unwrap(), b"true");
        assert_eq!(canonical_json(&false).unwrap(), b"false");
        assert_eq!(canonical_json(&42u64).unwrap(), b"42");
        assert_eq!(canonical_json(&"hi").unwrap(), br#""hi""#);
    }

    #[test]
    fn empty_collections() {
        assert_eq!(canonical_json(&json!([])).unwrap(), b"[]");
        assert_eq!(canonical_json(&json!({})).unwrap(), b"{}");
    }

    #[test]
    fn keys_are_sorted() {
        let v = json!({ "z": 1, "a": 2, "m": 3 });
        assert_eq!(canonical_json(&v).unwrap(), br#"{"a":2,"m":3,"z":1}"#);
    }

    #[test]
    fn nested_keys_are_sorted() {
        let v = json!({
            "outer_z": { "inner_b": 1, "inner_a": 2 },
            "outer_a": [ { "z": 1, "a": 2 }, { "y": 3, "b": 4 } ]
        });
        assert_eq!(
            canonical_json(&v).unwrap(),
            br#"{"outer_a":[{"a":2,"z":1},{"b":4,"y":3}],"outer_z":{"inner_a":2,"inner_b":1}}"#,
        );
    }

    #[test]
    fn permuted_inputs_produce_equal_bytes() {
        // Build the same logical object two different ways. They must canonicalize
        // to byte-equal output. This is the core guarantee that makes signatures
        // verifiable across implementations.
        let a = json!({
            "version": 17,
            "node": "app-01",
            "claims": [
                { "id": { "kind": "file", "key": "/etc/foo" }, "owners": ["bar"] },
                { "id": { "kind": "apt_package", "key": "caddy" }, "owners": ["caddy"] }
            ]
        });
        let b = json!({
            "node": "app-01",
            "claims": [
                { "owners": ["bar"], "id": { "key": "/etc/foo", "kind": "file" } },
                { "owners": ["caddy"], "id": { "kind": "apt_package", "key": "caddy" } }
            ],
            "version": 17
        });
        assert_eq!(canonical_json(&a).unwrap(), canonical_json(&b).unwrap());
    }

    #[test]
    fn arrays_preserve_order() {
        // Arrays have semantic order, unlike objects.
        let a = json!([3, 1, 2]);
        let b = json!([1, 2, 3]);
        assert_ne!(canonical_json(&a).unwrap(), canonical_json(&b).unwrap());
    }

    #[test]
    fn unicode_strings() {
        let v = json!({ "emoji": "🦀", "cyrillic": "Привет", "with_quote": "say \"hi\"" });
        let bytes = canonical_json(&v).unwrap();
        // Round-tripping must yield equivalent value.
        let parsed: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(parsed, v);
    }

    #[test]
    fn deterministic_under_repeated_calls() {
        let v = json!({ "b": [1, 2, { "y": 1, "x": 2 }], "a": null });
        let first = canonical_json(&v).unwrap();
        for _ in 0..16 {
            assert_eq!(canonical_json(&v).unwrap(), first);
        }
    }
}
