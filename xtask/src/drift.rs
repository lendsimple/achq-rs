//! `cargo xtask fetch` and `cargo xtask drift`: compare the vendored files with
//! the pages the documentation serves.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde_json::Value;

use crate::sources::Kind;

/// What changed in one vendored file, or `None` if nothing did.
///
/// A fragment's request-body properties, query parameters and component schemas
/// are compared one by one; anything else that differs is reported as other
/// changes.
pub fn report(kind: Kind, vendored: &str, live: &str) -> Result<Option<String>, String> {
    if vendored == live {
        return Ok(None);
    }
    if kind == Kind::Markdown {
        return Ok(Some("  changed\n".into()));
    }
    let parse = |text: &str| serde_json::from_str::<Value>(text).map_err(|e| e.to_string());
    let (old, new) = (parse(vendored)?, parse(live)?);
    if old == new {
        // Only the formatting differs.
        return Ok(Some("  reformatted\n".into()));
    }
    let (old_items, new_items) = (items(&old), items(&new));
    let mut out = String::new();
    for (label, names) in [
        ("added", keys_only_in(&new_items, &old_items)),
        ("removed", keys_only_in(&old_items, &new_items)),
        ("changed", changed(&old_items, &new_items)),
    ] {
        for name in names {
            writeln!(out, "  {label}: {name}").unwrap();
        }
    }
    if out.is_empty() {
        out.push_str("  other changes\n");
    }
    Ok(Some(out))
}

/// The parts of a fragment worth naming in a report.
fn items(spec: &Value) -> BTreeMap<String, &Value> {
    let mut items = BTreeMap::new();
    for methods in spec["paths"]
        .as_object()
        .into_iter()
        .flat_map(|p| p.values())
    {
        for op in methods.as_object().into_iter().flat_map(|m| m.values()) {
            for param in op["parameters"].as_array().into_iter().flatten() {
                if let Some(name) = param["name"].as_str() {
                    items.insert(format!("query parameter {name}"), param);
                }
            }
            let contents = op["requestBody"]["content"].as_object();
            for content in contents.into_iter().flat_map(|c| c.values()) {
                let properties = content["schema"]["properties"].as_object();
                for (name, property) in properties.into_iter().flatten() {
                    items.insert(format!("property {name}"), property);
                }
            }
        }
    }
    let schemas = spec["components"]["schemas"].as_object();
    for (name, schema) in schemas.into_iter().flatten() {
        items.insert(format!("schema {name}"), schema);
    }
    items
}

fn keys_only_in<'a>(a: &'a BTreeMap<String, &Value>, b: &BTreeMap<String, &Value>) -> Vec<&'a str> {
    a.keys()
        .filter(|k| !b.contains_key(*k))
        .map(String::as_str)
        .collect()
}

fn changed<'a>(old: &'a BTreeMap<String, &Value>, new: &BTreeMap<String, &Value>) -> Vec<&'a str> {
    old.iter()
        .filter(|(k, v)| new.get(*k).is_some_and(|n| n != *v))
        .map(|(k, _)| k.as_str())
        .collect()
}

/// Today's date in UTC, as `YYYY-MM-DD`.
pub fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the clock is after 1970")
        .as_secs();
    civil_date(i64::try_from(secs / 86_400).expect("in range"))
}

/// The date `days` after 1970-01-01 (Howard Hinnant's `civil_from_days`).
fn civil_date(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Rewrites the date and checksum in the `schema/README.md` row for `file`.
pub fn update_readme(readme: &str, file: &str, date: &str, sha256: &str) -> Result<String, String> {
    let prefix = format!("| `{file}` |");
    let mut found = false;
    let lines: Vec<String> = readme
        .lines()
        .map(|line| {
            if !line.starts_with(&prefix) {
                return line.to_owned();
            }
            found = true;
            let cells: Vec<&str> = line.split(" | ").collect();
            // | `file` | <url> | date | `sha` |
            format!("{} | {} | {date} | `{sha256}` |", cells[0], cells[1])
        })
        .collect();
    if !found {
        return Err(format!("schema/README.md has no row for {file}"));
    }
    Ok(lines.join("\n") + "\n")
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut hex, b| {
            write!(hex, "{b:02x}").unwrap();
            hex
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fragment(properties: Value, params: Value) -> String {
        serde_json::json!({"paths": {"/?cancel": {"post": {
            "parameters": params,
            "requestBody": {"content": {"application/x-www-form-urlencoded": {
                "schema": {"properties": properties}
            }}}
        }}}})
        .to_string()
    }

    #[test]
    fn reports_properties_and_parameters() {
        use serde_json::json;
        let old = fragment(
            json!({"A": {"type": "string"}, "B": {"type": "string"}}),
            json!([{"name": "Command"}]),
        );
        let new = fragment(
            json!({"A": {"type": "integer"}, "C": {"type": "string"}}),
            json!([{"name": "Command"}, {"name": "OwnerAuth"}]),
        );
        assert_eq!(
            report(Kind::Json, &old, &new).unwrap().unwrap(),
            "  added: property C\n  added: query parameter OwnerAuth\n  \
             removed: property B\n  changed: property A\n"
        );
        assert_eq!(report(Kind::Json, &old, &old).unwrap(), None);
    }

    #[test]
    fn reports_schemas_and_other_changes() {
        let old = r#"{"info": {"version": "1.0"}, "components": {"schemas": {"Fee": {}}}}"#;
        let renamed = r#"{"info": {"version": "1.1"}, "components": {"schemas": {"Fee": {}}}}"#;
        assert_eq!(
            report(Kind::Json, old, renamed).unwrap().unwrap(),
            "  other changes\n"
        );
        let changed = r#"{"info": {"version": "1.0"}, "components": {"schemas": {"Fee": {"type": "object"}}}}"#;
        assert_eq!(
            report(Kind::Json, old, changed).unwrap().unwrap(),
            "  changed: schema Fee\n"
        );
        assert_eq!(
            report(Kind::Json, "{\"a\": 1}", "{\n  \"a\": 1\n}")
                .unwrap()
                .unwrap(),
            "  reformatted\n"
        );
    }

    #[test]
    fn markdown_is_compared_as_text() {
        assert_eq!(report(Kind::Markdown, "a\n", "a\n").unwrap(), None);
        assert_eq!(
            report(Kind::Markdown, "a\n", "b\n").unwrap().unwrap(),
            "  changed\n"
        );
    }

    #[test]
    fn dates() {
        assert_eq!(civil_date(0), "1970-01-01");
        assert_eq!(civil_date(20_761), "2026-11-04");
        assert_eq!(civil_date(11_016), "2000-02-29");
    }

    #[test]
    fn updates_the_readme_row() {
        let readme = "| File | Source | Fetched | SHA-256 |\n\
                      | `a.json` | <https://x/a> | 2026-01-01 | `00` |\n\
                      | `b.json` | <https://x/b> | 2026-01-01 | `11` |\n";
        assert_eq!(
            update_readme(readme, "b.json", "2026-11-04", "ff").unwrap(),
            "| File | Source | Fetched | SHA-256 |\n\
             | `a.json` | <https://x/a> | 2026-01-01 | `00` |\n\
             | `b.json` | <https://x/b> | 2026-11-04 | `ff` |\n"
        );
        assert!(update_readme(readme, "c.json", "", "").is_err());
    }

    #[test]
    fn hashes() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
