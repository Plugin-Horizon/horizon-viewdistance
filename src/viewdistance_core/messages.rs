//! `messages.yml`: every answer of `/vdt` can be reworded. A missing key keeps the built-in English text, as in the
//! original; `prefix` is put in front of every line. The file is the original's (nested keys, `&` colour codes,
//! `{placeholders}`); only the keys this port answers with are read.

use std::collections::HashMap;
use std::sync::Mutex;

/// The file written when `messages.yml` is missing (the original's complete default).
pub const DEFAULT_YAML: &str = include_str!("../../data/viewdistance/messages.yml");

static MESSAGES: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

fn unquote(v: &str) -> String {
    let v = v.trim();
    let bytes = v.as_bytes();
    if v.len() >= 2 && ((bytes[0] == b'"' && bytes[v.len() - 1] == b'"') || (bytes[0] == b'\'' && bytes[v.len() - 1] == b'\'')) {
        let inner = &v[1..v.len() - 1];
        if bytes[0] == b'"' {
            return inner.replace("\\\"", "\"").replace("\\\\", "\\");
        }
        return inner.replace("''", "'");
    }
    v.to_owned()
}

/// Reads the nested `key: value` lines into flat dotted keys.
pub fn parse(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut stack: Vec<(usize, String)> = Vec::new();
    for raw in text.lines() {
        let trimmed = raw.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = raw.len() - trimmed.len();
        let Some((key, value)) = trimmed.split_once(':') else { continue };
        let key = key.trim().trim_matches('"').to_owned();
        while stack.last().is_some_and(|(i, _)| *i >= indent) {
            stack.pop();
        }
        // A value may carry a trailing comment only when it is not quoted; the original's file has none.
        let value = value.trim();
        if value.is_empty() {
            stack.push((indent, key));
        } else {
            let path: Vec<&str> = stack.iter().map(|(_, k)| k.as_str()).chain(std::iter::once(key.as_str())).collect();
            out.insert(path.join("."), unquote(value));
        }
    }
    out
}

/// Takes the file's text as the messages in force (an empty or unreadable file leaves only the built-in texts).
pub fn load(text: &str) {
    *MESSAGES.lock().unwrap_or_else(|e| e.into_inner()) = Some(parse(text));
}

/// The text of `key`: the file's when it has one, else `default`, with every `{name}` filled from `vars`.
pub fn t(key: &str, default: &str, vars: &[(&str, String)]) -> String {
    let guard = MESSAGES.lock().unwrap_or_else(|e| e.into_inner());
    let mut text = guard.as_ref().and_then(|m| m.get(key)).cloned().unwrap_or_else(|| default.to_owned());
    for (name, value) in vars {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    text
}

/// The `prefix` put in front of every message.
pub fn prefix() -> String {
    MESSAGES.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|m| m.get("prefix")).cloned().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_keys_and_quotes() {
        let m = parse("prefix: \"&8[VDT] \"\nstatus:\n  header: \"&6Hi\"\n  fixed:\n    none: '&7none'\nreloaded: \"&aDone \\\"x\\\"\"\n");
        assert_eq!(m["prefix"], "&8[VDT] ");
        assert_eq!(m["status.header"], "&6Hi");
        assert_eq!(m["status.fixed.none"], "&7none");
        assert_eq!(m["reloaded"], "&aDone \"x\"");
    }

    #[test]
    fn the_shipped_file_has_the_keys_the_port_uses() {
        let m = parse(DEFAULT_YAML);
        assert_eq!(m["no-permission"], "&cNo permission.");
        assert_eq!(m["history.empty"], "&7No distance changes recorded yet since the last restart.");
        assert_eq!(m["preset.applied"], "&aApplied preset &f{preset}&a ({count} values changed). Config reloaded - fine-tune further as needed.");
        assert!(m.contains_key("doctor.header") && m.contains_key("analyze.footer"));
    }

    #[test]
    fn placeholders_are_filled() {
        assert_eq!(t("nokey.at.all", "Hello {who}, {who}!", &[("who", "you".into())]), "Hello you, you!");
    }
}
