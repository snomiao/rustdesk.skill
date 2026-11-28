//! What JavaScript does with text, numbers and paths, where the TypeScript
//! relies on it: `trim`, `toLowerCase`, `String(number)`, `toFixed(2)`,
//! `Math.round`, `JSON.stringify` of a string, and node's `path.posix`.
//!
//! Where an exact copy would be long and the case is rare, a function
//! answers `None` and the caller hands the invocation back.

/// JavaScript's WhiteSpace and LineTerminator: what `trim` takes off and
/// what `\s` matches (held against bun by rs/src/compare.rs).
pub fn is_space(c: char) -> bool {
    matches!(
        c,
        '\u{9}'..='\u{d}' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
    )
}

/// `text.trim()`
pub fn trim(text: &str) -> &str {
    text.trim_matches(is_space)
}

/// The members every plain JavaScript object has from Object.prototype: a
/// lookup `obj[name]` by one of these names answers with that member.
pub const OBJECT_MEMBERS: [&str; 12] = [
    "constructor",
    "toString",
    "toLocaleString",
    "valueOf",
    "hasOwnProperty",
    "isPrototypeOf",
    "propertyIsEnumerable",
    "__proto__",
    "__defineGetter__",
    "__defineSetter__",
    "__lookupGetter__",
    "__lookupSetter__",
];

pub fn is_object_member(name: &str) -> bool {
    OBJECT_MEMBERS.contains(&name)
}

/// Characters whose `toLowerCase` in the runtime is not what Rust's
/// `to_lowercase` gives, or may not be in another runtime's release: the
/// capital sigma, whose lower case depends on what is around it, and the
/// blocks whose case pairs came with Unicode 14 or later. rs/src/compare.rs
/// compares every other code point with bun's `toLowerCase`.
pub fn lowercase_unsure(c: char) -> bool {
    matches!(c as u32,
        0x3a3 // Σ: σ or ς by its neighbours
        | 0x2c00..=0x2c5f // Glagolitic
        | 0xa720..=0xa7ff // Latin Extended-D
        | 0x10570..=0x105bf // Vithkuqi
        | 0x10d40..=0x10d8f // Garay
        | 0x16ea0..=0x16edf // Beria Erfe
        | 0x1e900..=0x1e95f // Adlam
    )
}

/// `text.toLowerCase()`, or `None` where the runtime's answer is not certain.
pub fn to_lower(text: &str) -> Option<String> {
    if text.chars().any(lowercase_unsure) {
        return None;
    }
    Some(text.to_lowercase())
}

/// `Math.round(x)`: the nearest integer, halves upwards.
pub fn math_round(x: f64) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let floor = x.floor();
    let rounded = if x - floor >= 0.5 { floor + 1.0 } else { floor };
    // Math.round(-0.4) is -0.
    if rounded == 0.0 && x.is_sign_negative() {
        -0.0
    } else {
        rounded
    }
}

/// `String(x)` for a number, or `None` when Rust's shortest digits might
/// not be the runtime's (16 or 17 significant digits, where two spellings
/// can be equally near and JavaScript takes the even one).
pub fn number(x: f64) -> Option<String> {
    if x.is_nan() {
        return Some("NaN".into());
    }
    if x.is_infinite() {
        return Some(if x > 0.0 { "Infinity" } else { "-Infinity" }.into());
    }
    if x == 0.0 {
        return Some("0".into());
    }
    let sign = if x < 0.0 { "-" } else { "" };
    // d.ddde±N, the shortest digits that read back as x.
    let exp_form = format!("{:e}", x.abs());
    let (mantissa, exponent) = exp_form.split_once('e')?;
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let exponent: i32 = exponent.parse().ok()?;
    let k = digits.len() as i32;
    if k > 15 {
        return None;
    }
    let n = exponent + 1;
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let e = n - 1;
        let e = if e < 0 { format!("-{}", -e) } else { format!("+{e}") };
        if k == 1 {
            format!("{digits}e{e}")
        } else {
            format!("{}.{}e{e}", &digits[..1], &digits[1..])
        }
    };
    Some(format!("{sign}{body}"))
}

/// `x.toFixed(2)`, or `None` for a number of 1e21 or more (String(x) then).
pub fn to_fixed_2(x: f64) -> Option<String> {
    if x.is_nan() {
        return Some("NaN".into());
    }
    if x.abs() >= 1e21 || x.is_infinite() {
        return None;
    }
    let sign = if x < 0.0 { "-" } else { "" };
    // The exact decimal value of the double: no f64 has more than 1074
    // digits after the point.
    let exact = format!("{:.1100}", x.abs());
    let (whole, fraction) = exact.split_once('.')?;
    let kept = &fraction[..2];
    let rest = &fraction[2..];
    // The nearer of the two, and of two equally near the larger.
    let up = rest.as_bytes()[0] >= b'5';
    let mut digits: Vec<u8> = format!("{whole}{kept}").into_bytes();
    if up {
        let mut at = digits.len();
        loop {
            if at == 0 {
                digits.insert(0, b'1');
                break;
            }
            at -= 1;
            if digits[at] == b'9' {
                digits[at] = b'0';
            } else {
                digits[at] += 1;
                break;
            }
        }
    }
    let text = String::from_utf8(digits).ok()?;
    let (int_part, frac_part) = text.split_at(text.len() - 2);
    let int_part = int_part.trim_start_matches('0');
    let int_part = if int_part.is_empty() { "0" } else { int_part };
    Some(format!("{sign}{int_part}.{frac_part}"))
}

/// `JSON.stringify(text)` for a string without lone surrogates (a Rust
/// string has none).
pub fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `text.length`: UTF-16 code units.
pub fn utf16_len(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

/// `text.padStart(width)`
pub fn pad_start(text: &str, width: usize) -> String {
    let len = utf16_len(text);
    if len >= width {
        text.to_string()
    } else {
        format!("{}{text}", " ".repeat(width - len))
    }
}

/// `text.slice(0, units)`, or `None` when that would cut a character in two.
pub fn slice_utf16(text: &str, units: usize) -> Option<&str> {
    let mut count = 0;
    for (at, c) in text.char_indices() {
        if count == units {
            return Some(&text[..at]);
        }
        count += c.len_utf16();
        if count > units {
            return None;
        }
    }
    Some(text)
}

// ── node's path.posix ────────────────────────────────────────────────────────

/// `path.posix.normalize(p)`
pub fn normalize(p: &str) -> String {
    if p.is_empty() {
        return ".".into();
    }
    let absolute = p.starts_with('/');
    let trailing = p.ends_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for segment in p.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if parts.last().is_some_and(|last| *last != "..") {
                    parts.pop();
                } else if !absolute {
                    parts.push("..");
                }
            }
            segment => parts.push(segment),
        }
    }
    let mut body = parts.join("/");
    if body.is_empty() && !absolute {
        body = ".".into();
    }
    if !body.is_empty() && trailing {
        body.push('/');
    }
    if absolute {
        format!("/{body}")
    } else {
        body
    }
}

/// `path.posix.join(...parts)`
pub fn join(parts: &[&str]) -> String {
    let joined: Vec<&str> = parts.iter().copied().filter(|part| !part.is_empty()).collect();
    if joined.is_empty() {
        return ".".into();
    }
    normalize(&joined.join("/"))
}

/// `path.posix.basename(p)`
pub fn basename(p: &str) -> &str {
    let trimmed = p.trim_end_matches('/');
    if trimmed.is_empty() {
        return "";
    }
    match trimmed.rfind('/') {
        Some(at) => &trimmed[at + 1..],
        None => trimmed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_as_javascript_writes_them() {
        for (x, text) in [
            (0.0, "0"),
            (-0.0, "0"),
            (1.0, "1"),
            (-5.0, "-5"),
            (0.5, "0.5"),
            (0.9, "0.9"),
            (0.9876, "0.9876"),
            (1280.0, "1280"),
            (1e21, "1e+21"),
            (1.5e21, "1.5e+21"),
            (123456789012345.0, "123456789012345"),
            (1e-7, "1e-7"),
            (1.5e-7, "1.5e-7"),
            (0.000001, "0.000001"),
            (0.0000015, "0.0000015"),
            (100.0, "100"),
            (1e20, "100000000000000000000"),
            (2.5, "2.5"),
            (-0.5, "-0.5"),
            (f64::NAN, "NaN"),
            (f64::INFINITY, "Infinity"),
        ] {
            assert_eq!(number(x).as_deref(), Some(text), "{x}");
        }
        assert_eq!(number(0.1 + 0.2), None);
        assert_eq!(number(1459077941253781.2), None);
    }

    #[test]
    fn to_fixed_takes_the_exact_value() {
        for (x, text) in [
            (0.0, "0.00"),
            (-0.0, "0.00"),
            (1.0, "1.00"),
            (0.125, "0.13"),
            (0.375, "0.38"),
            (1.005, "1.00"),
            (0.995, "0.99"),
            (0.9999, "1.00"),
            (9.999, "10.00"),
            (0.5, "0.50"),
            (-0.001, "-0.00"),
            (-1.5, "-1.50"),
            (-0.125, "-0.13"),
            (0.9876, "0.99"),
            (123.456, "123.46"),
            (1e20, "100000000000000000000.00"),
        ] {
            assert_eq!(to_fixed_2(x).as_deref(), Some(text), "{x}");
        }
        assert_eq!(to_fixed_2(1e21), None);
    }

    #[test]
    fn math_round_takes_halves_up() {
        for (x, r) in [(0.5, 1.0), (1.5, 2.0), (2.5, 3.0), (-0.5, -0.0), (-1.5, -1.0), (-2.6, -3.0), (0.49999999999999994, 0.0), (4503599627370495.5, 4503599627370496.0)] {
            assert_eq!(math_round(x), r, "{x}");
            assert_eq!(math_round(x).is_sign_negative(), r.is_sign_negative(), "{x}");
        }
        assert!(math_round(-0.4).is_sign_negative());
    }

    #[test]
    fn posix_paths_as_node_writes_them() {
        for (parts, joined) in [
            (&["/tmp", "rustdesk-skill"][..], "/tmp/rustdesk-skill"),
            (&["/tmp/", "rustdesk-skill"], "/tmp/rustdesk-skill"),
            (&["/a//b/../c", "x"], "/a/c/x"),
            (&["rel", "x"], "rel/x"),
            (&["./rel/", "x"], "rel/x"),
            (&["../a", "x"], "../a/x"),
            (&["a/../..", "x"], "../x"),
            (&["/", "x"], "/x"),
            (&["/..", "x"], "/x"),
            (&["", "x"], "x"),
            (&["a", ""], "a"),
            (&["a/", ""], "a/"),
            (&["", ""], "."),
            (&["//x//", ".config", "rustdesk-skill"], "/x/.config/rustdesk-skill"),
        ] {
            assert_eq!(join(parts), joined, "{parts:?}");
        }
        assert_eq!(normalize("a/.."), ".");
        assert_eq!(normalize("a/../"), "./");
        assert_eq!(normalize("/a/.."), "/");
        for (p, base) in [("/a/b", "b"), ("/a/b/", "b"), ("/", ""), ("", ""), ("b", "b"), ("//", ""), ("a//", "a")] {
            assert_eq!(basename(p), base, "{p}");
        }
    }

    #[test]
    fn strings_as_json_stringify_writes_them() {
        assert_eq!(json_string("a\"b\\c/d"), r#""a\"b\\c/d""#);
        assert_eq!(json_string("\u{8}\u{c}\n\r\t\u{1}\u{1f}\u{7f}\u{2028}é😀"), "\"\\b\\f\\n\\r\\t\\u0001\\u001f\u{7f}\u{2028}é😀\"");
    }

    #[test]
    fn utf16_measures() {
        assert_eq!(utf16_len("a😀é"), 4);
        assert_eq!(pad_start("5", 4), "   5");
        assert_eq!(slice_utf16("ab😀c", 2), Some("ab"));
        assert_eq!(slice_utf16("ab😀c", 3), None);
        assert_eq!(slice_utf16("ab😀c", 4), Some("ab😀"));
        assert_eq!(slice_utf16("ab", 200), Some("ab"));
        assert_eq!(trim("\u{feff} a \u{3000}\n"), "a");
        assert_eq!(trim("\u{180e}a"), "\u{180e}a");
    }
}
