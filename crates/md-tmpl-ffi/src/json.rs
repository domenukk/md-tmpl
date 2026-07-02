use std::sync::Arc;

use md_tmpl::Value;

/// Convert a JSON string to a template `Value`.
pub(crate) fn json_to_value(json: &str) -> Result<Value, String> {
    // Simple recursive JSON parser — avoids pulling in serde_json as a dependency.
    // We leverage the serde feature to deserialize.
    let trimmed = json.trim();
    if trimmed.is_empty() {
        return Err("empty JSON string".to_string());
    }
    parse_json_value(trimmed).map(|(val, _)| val)
}

/// Recursive JSON parser that returns (Value, `remaining_str`).
pub(crate) fn parse_json_value(s: &str) -> Result<(Value, &str), String> {
    let s = s.trim_start();
    if s.is_empty() {
        return Err("unexpected end of JSON".to_string());
    }

    match s.as_bytes()[0] {
        b'"' => parse_json_string(s),
        b'{' => parse_json_object(s),
        b'[' => parse_json_array(s),
        b't' | b'f' => parse_json_bool(s),
        b'n' => parse_json_null(s),
        _ => parse_json_number(s),
    }
}

pub(crate) fn parse_json_string(s: &str) -> Result<(Value, &str), String> {
    debug_assert!(s.starts_with('"'));
    let s = &s[1..]; // skip opening quote
    let mut result = String::new();
    let mut chars = s.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => {
                return Ok((Value::Str(result), &s[i + 1..]));
            }
            '\\' => {
                if let Some((_, escaped)) = chars.next() {
                    match escaped {
                        '"' | '\\' | '/' => result.push(escaped),
                        'n' => result.push('\n'),
                        'r' => result.push('\r'),
                        't' => result.push('\t'),
                        'b' => result.push('\u{08}'),
                        'f' => result.push('\u{0C}'),
                        'u' => {
                            // Parse 4 hex digits
                            let mut hex = String::with_capacity(4);
                            for _ in 0..4 {
                                if let Some((_, h)) = chars.next() {
                                    hex.push(h);
                                } else {
                                    return Err("incomplete unicode escape".to_string());
                                }
                            }
                            let code = u32::from_str_radix(&hex, 16)
                                .map_err(|e| format!("invalid unicode escape: {e}"))?;
                            let ch = char::from_u32(code)
                                .ok_or_else(|| format!("invalid unicode code point: {code}"))?;
                            result.push(ch);
                        }
                        _ => {
                            result.push('\\');
                            result.push(escaped);
                        }
                    }
                }
            }
            _ => result.push(c),
        }
    }
    Err("unterminated string".to_string())
}

pub(crate) fn parse_json_object(s: &str) -> Result<(Value, &str), String> {
    debug_assert!(s.starts_with('{'));
    let mut s = s[1..].trim_start();
    let mut map = std::collections::HashMap::new();

    if let Some(rest) = s.strip_prefix('}') {
        return Ok((Value::Struct(Arc::new(map.into_iter().collect())), rest));
    }

    loop {
        // Parse key
        if !s.starts_with('"') {
            return Err(format!(
                "expected string key, got: {}",
                &s[..s.len().min(20)]
            ));
        }
        let (key_val, rest) = parse_json_string(s)?;
        let Value::Str(key) = key_val else {
            unreachable!()
        };
        s = rest.trim_start();

        // Expect colon
        if !s.starts_with(':') {
            return Err("expected ':' after key".to_string());
        }
        s = s[1..].trim_start();

        // Parse value
        let (val, rest) = parse_json_value(s)?;
        map.insert(key, val);
        s = rest.trim_start();

        if s.starts_with('}') {
            s = &s[1..];
            break;
        }
        if s.starts_with(',') {
            s = s[1..].trim_start();
        } else {
            return Err("expected ',' or '}' in object".to_string());
        }
    }

    Ok((Value::Struct(Arc::new(map.into_iter().collect())), s))
}

pub(crate) fn parse_json_array(s: &str) -> Result<(Value, &str), String> {
    debug_assert!(s.starts_with('['));
    let mut s = s[1..].trim_start();
    let mut items = Vec::new();

    if let Some(rest) = s.strip_prefix(']') {
        return Ok((Value::List(Arc::new(items)), rest));
    }

    loop {
        let (val, rest) = parse_json_value(s)?;
        items.push(val);
        s = rest.trim_start();

        if s.starts_with(']') {
            s = &s[1..];
            break;
        }
        if s.starts_with(',') {
            s = s[1..].trim_start();
        } else {
            return Err("expected ',' or ']' in array".to_string());
        }
    }

    Ok((Value::List(Arc::new(items)), s))
}

pub(crate) fn parse_json_bool(s: &str) -> Result<(Value, &str), String> {
    if let Some(rest) = s.strip_prefix("true") {
        Ok((Value::Bool(true), rest))
    } else if let Some(rest) = s.strip_prefix("false") {
        Ok((Value::Bool(false), rest))
    } else {
        Err(format!("unexpected token: {}", &s[..s.len().min(10)]))
    }
}

pub(crate) fn parse_json_null(s: &str) -> Result<(Value, &str), String> {
    if let Some(rest) = s.strip_prefix("null") {
        // Map JSON null to the template engine's `Value::None`, used by
        // `option(T)` types.
        Ok((Value::None, rest))
    } else {
        Err(format!("unexpected token: {}", &s[..s.len().min(10)]))
    }
}

pub(crate) fn parse_json_number(s: &str) -> Result<(Value, &str), String> {
    let end = s
        .find(|c: char| {
            !c.is_ascii_digit() && c != '-' && c != '+' && c != '.' && c != 'e' && c != 'E'
        })
        .unwrap_or(s.len());
    let num_str = &s[..end];

    if num_str.contains('.') || num_str.contains('e') || num_str.contains('E') {
        let f: f64 = num_str
            .parse()
            .map_err(|e| format!("invalid float '{num_str}': {e}"))?;
        Ok((Value::Float(f), &s[end..]))
    } else {
        let i: i64 = num_str
            .parse()
            .map_err(|e| format!("invalid integer '{num_str}': {e}"))?;
        Ok((Value::Int(i), &s[end..]))
    }
}

/// Parse a JSON string of `[["name", "type"], ...]` pairs.
pub(crate) fn parse_json_string_pairs(json: &str) -> Result<Vec<Vec<String>>, String> {
    let trimmed = json.trim();
    if !trimmed.starts_with('[') {
        return Err("expected JSON array".to_string());
    }

    let (val, _) = parse_json_array(trimmed)?;
    let Value::List(items) = val else {
        return Err("expected JSON array".to_string());
    };

    let mut result = Vec::new();
    for item in items.iter() {
        let Value::List(pair) = item else {
            return Err("expected [name, type] pair".to_string());
        };
        let mut strings = Vec::new();
        for elem in pair.iter() {
            let Value::Str(s) = elem else {
                return Err("expected string in pair".to_string());
            };
            strings.push(s.clone());
        }
        result.push(strings);
    }
    Ok(result)
}
