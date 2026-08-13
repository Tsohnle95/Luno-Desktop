use serde_json::Value;

const REDACTED: &str = "[REDACTED]";

/// Redacts secrets in arbitrary log values before they can reach storage or the UI.
pub fn redact(value: &str) -> String {
    let mut output = value.to_owned();
    for key in ["api_key", "apikey", "api-key", "access_token", "auth_token", "token", "password", "secret", "key"] {
        output = redact_key_value(&output, key);
    }
    redact_urls(&output)
}

fn redact_key_value(input: &str, key: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        let boundary = i == 0 || !bytes[i - 1].is_ascii_alphanumeric() && bytes[i - 1] != b'_';
        let key_end = i + key.len();
        if boundary && key_end <= bytes.len() && input[i..key_end].eq_ignore_ascii_case(key) {
            let mut j = key_end;
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') { j += 1; }
            if j < bytes.len() && (bytes[j] == b'=' || bytes[j] == b':') {
                out.push_str(&input[..i]);
                out.push_str(&input[i..key_end]);
                out.push_str(&input[key_end..j]);
                out.push_str(&input[j..=j]);
                out.push_str(REDACTED);
                i = j + 1;
                while i < bytes.len() && !matches!(bytes[i], b' ' | b'\t' | b'&' | b',' | b';' | b'\n' | b'"' | b'\'') { i += 1; }
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn redact_urls(input: &str) -> String {
    input.split_whitespace().map(|part| {
        if let Some(scheme) = part.find("://") {
            let start = part[..scheme].rfind(|c: char| c.is_whitespace()).map_or(0, |i| i + 1);
            let url = &part[start..];
            let mut clean = url.to_owned();
            if let Some(at) = clean.find('@') {
                if clean[..at].contains(':') { clean.replace_range(..at, REDACTED); }
            }
            if let Some(q) = clean.find('?') {
                let (base, query) = clean.split_at(q);
                let query = query.split('&').map(|item| {
                    let name = item.split('=').next().unwrap_or(item);
                    if ["key", "api_key", "apikey", "token", "access_token", "password", "secret"].iter().any(|x| name.trim_start_matches('?').eq_ignore_ascii_case(x)) {
                        format!("{name}={REDACTED}")
                    } else { item.to_owned() }
                }).collect::<Vec<_>>().join("&");
                clean = format!("{base}{query}");
            }
            clean
        } else { part.to_owned() }
    }).collect::<Vec<_>>().join(" ")
}

pub fn redact_json(value: &Value) -> Value {
    match value {
        Value::String(s) => Value::String(redact(s)),
        Value::Array(items) => Value::Array(items.iter().map(redact_json).collect()),
        Value::Object(map) => Value::Object(map.iter().map(|(key, value)| {
            let sensitive = ["api_key", "apikey", "api-key", "access_token", "auth_token", "token", "password", "secret"].iter().any(|x| key.eq_ignore_ascii_case(x));
            (key.clone(), if sensitive { Value::String(REDACTED.to_owned()) } else { redact_json(value) })
        }).collect()),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn redacts_keys_and_urls() {
        let value = redact("api_key=abc https://u:pw@example.test/x?token=xyz&ok=1");
        assert!(!value.contains("abc") && !value.contains("pw") && !value.contains("xyz"));
        assert!(value.contains(REDACTED));
    }
}
