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

/// `String(x)` for every number: the fewest digits that read back as `x`,
/// of those the nearest to `x`, and of two as near the one whose last digit
/// is even (ECMAScript's Number::toString, as JavaScriptCore does it; held
/// against bun by rs/src/compare.rs). For what the binary has to write
/// after it has acted, where it cannot hand back.
pub fn number_exact(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x == 0.0 {
        return "0".into();
    }
    if x < 0.0 {
        return format!("-{}", number_exact(-x));
    }
    if x.is_infinite() {
        return "Infinity".into();
    }
    // Rust's shortest digits have the fewest digits, but where two are as
    // near it may take the other one; its neighbours decide.
    let shortest = format!("{x:e}");
    let (mantissa, exponent) = shortest.split_once('e').expect("{:e} writes an exponent");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let exponent: i32 = exponent.parse().expect("{:e} writes an integer exponent");
    let k = digits.len() as u32;
    let s: u64 = digits.parse().expect("at most 17 digits");
    let p = exponent - k as i32 + 1;
    let lowest = 10u64.pow(k - 1);
    let reads_back = |c: u64| (lowest..lowest * 10).contains(&c) && format!("{c}e{p}").parse::<f64>().ok() == Some(x);
    // The exact value of x, all its digits, and the power of ten of the first.
    let exact = format!("{x:.1100e}");
    let (exact_mantissa, exact_exponent) = exact.split_once('e').expect("{:e} writes an exponent");
    let exact_digits: String = exact_mantissa.chars().filter(|c| *c != '.').collect();
    let exact_exponent: i32 = exact_exponent.parse().expect("an integer exponent");
    // How x compares with (c + 0.5) × 10^p, half way between c and c + 1.
    let against_half = |c: u64| -> std::cmp::Ordering {
        let half = format!("{}5", c);
        // (c·10 + 5) × 10^(p−1) has its first digit at 10^exponent.
        if exact_exponent != exponent {
            return exact_exponent.cmp(&exponent);
        }
        let half = format!("{half:0<width$}", width = exact_digits.len());
        exact_digits.as_str().cmp(half.as_str())
    };
    let mut best = None;
    for c in [s.wrapping_sub(1), s, s + 1] {
        if !reads_back(c) {
            continue;
        }
        best = Some(match best {
            None => c,
            Some(b) => match against_half(b) {
                std::cmp::Ordering::Greater => c,
                std::cmp::Ordering::Less => b,
                std::cmp::Ordering::Equal if b % 2 == 0 => b,
                std::cmp::Ordering::Equal => c,
            },
        });
    }
    let digits = best.unwrap_or(s).to_string();
    let k = digits.len() as i32;
    let n = exponent + 1;
    if k <= n && n <= 21 {
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
    }
}

/// `Number(text)` for a string: JavaScript's StringToNumber (white space
/// around, empty is 0, `Infinity`, `0x`/`0o`/`0b` without a sign, decimal
/// literals with an exponent; anything else is NaN).
pub fn string_to_number(text: &str) -> f64 {
    let t = trim(text);
    if t.is_empty() {
        return 0.0;
    }
    let bytes = t.as_bytes();
    if bytes.len() > 2 && bytes[0] == b'0' {
        let radix = match bytes[1] {
            b'x' | b'X' => 16,
            b'o' | b'O' => 8,
            b'b' | b'B' => 2,
            _ => 0,
        };
        if radix != 0 {
            return integer_in_radix(&t[2..], radix);
        }
    }
    let (sign, rest) = match bytes[0] {
        b'+' => (1.0, &t[1..]),
        b'-' => (-1.0, &t[1..]),
        _ => (1.0, t),
    };
    if rest == "Infinity" {
        return sign * f64::INFINITY;
    }
    // StrUnsignedDecimalLiteral: digits [. digits] | . digits, then [eE][+-]digits.
    let b = rest.as_bytes();
    let mut at = 0;
    let whole = b.iter().take_while(|c| c.is_ascii_digit()).count();
    at += whole;
    let mut fraction = 0;
    if b.get(at) == Some(&b'.') {
        at += 1;
        fraction = b[at..].iter().take_while(|c| c.is_ascii_digit()).count();
        at += fraction;
    }
    if whole + fraction == 0 {
        return f64::NAN;
    }
    if matches!(b.get(at), Some(b'e' | b'E')) {
        at += 1;
        if matches!(b.get(at), Some(b'+' | b'-')) {
            at += 1;
        }
        let exponent = b[at..].iter().take_while(|c| c.is_ascii_digit()).count();
        if exponent == 0 {
            return f64::NAN;
        }
        at += exponent;
    }
    if at != b.len() {
        return f64::NAN;
    }
    // Rust reads a decimal literal correctly rounded, as JavaScript does.
    match rest.parse::<f64>() {
        Ok(value) => sign * value,
        Err(_) => f64::NAN,
    }
}

/// The value of `digits` in base 2, 8 or 16, rounded to the nearest double
/// (half to even), or NaN when one is not a digit there.
fn integer_in_radix(digits: &str, radix: u32) -> f64 {
    let width = radix.trailing_zeros();
    let mut bits: Vec<bool> = Vec::with_capacity(digits.len() * width as usize);
    for c in digits.chars() {
        let Some(value) = c.to_digit(radix) else { return f64::NAN };
        for at in (0..width).rev() {
            bits.push(value >> at & 1 == 1);
        }
    }
    let Some(first) = bits.iter().position(|b| *b) else { return 0.0 };
    let bits = &bits[first..];
    if bits.len() <= 53 {
        return bits.iter().fold(0u64, |n, b| n << 1 | u64::from(*b)) as f64;
    }
    // 53 bits kept, the next one and whether any after it is set decide.
    let mut kept = bits[..53].iter().fold(0u64, |n, b| n << 1 | u64::from(*b));
    let half = bits[53];
    let beyond = bits[54..].iter().any(|b| *b);
    if half && (beyond || kept & 1 == 1) {
        kept += 1;
    }
    let scale = (bits.len() - 53) as i32;
    if scale > 1100 {
        return f64::INFINITY;
    }
    // kept < 2^54, times a power of two: exact until it is too large.
    (kept as f64) * 2f64.powi(scale)
}

/// `Math.max(a, b)`: NaN if either is.
pub fn math_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == b {
        // Math.max(-0, 0) is 0.
        if a.is_sign_negative() {
            b
        } else {
            a
        }
    } else {
        a.max(b)
    }
}

/// `text.toLowerCase()` for looking up one of the fixed texts the
/// TypeScript searches the screen for, which are all ASCII, kana and CJK:
/// a character whose lower case is not certain here (`lowercase_unsure`)
/// becomes U+FFFF, which none of those texts holds. Whatever the runtime
/// makes of such a character (σ or ς, or a letter of its own script), it is
/// none of them either, and no other character's lower case depends on
/// what is around it, so whether one of them is found does not change.
pub fn lower_for_fixed(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if lowercase_unsure(c) {
            out.push('\u{ffff}');
        } else {
            out.extend(c.to_lowercase());
        }
    }
    out
}

/// Whether `text` is made of what `lower_for_fixed` relies on: ASCII, kana
/// and CJK ideographs.
pub fn fixed_alphabet(text: &str) -> bool {
    text.chars().all(|c| c.is_ascii() || matches!(c as u32, 0x3000..=0x30ff | 0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xf900..=0xfaff))
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

/// `text.slice(0, units)` as bun prints it: half of a character cut in two
/// is a lone surrogate, which it writes as U+FFFD.
pub fn slice_utf16_lossy(text: &str, units: usize) -> String {
    let mut out = String::new();
    let mut count = 0;
    for c in text.chars() {
        if count >= units {
            break;
        }
        if count + c.len_utf16() > units {
            out.push('\u{fffd}');
            break;
        }
        out.push(c);
        count += c.len_utf16();
    }
    out
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
    fn every_number_as_javascript_writes_it() {
        for (x, text) in [
            (0.0, "0"),
            (-0.0, "0"),
            (1459077941253781.2, "1459077941253781.2"),
            (0.1 + 0.2, "0.30000000000000004"),
            (123456789012345680000.0, "123456789012345680000"),
            (1e21, "1e+21"),
            (-1e-7, "-1e-7"),
            (5e-324, "5e-324"),
            (1.7976931348623157e308, "1.7976931348623157e+308"),
            (f64::INFINITY, "Infinity"),
            (f64::NEG_INFINITY, "-Infinity"),
            (9007199254740993.0, "9007199254740992"),
            (2f64.powi(60), "1152921504606847000"),
        ] {
            assert_eq!(number_exact(x), text, "{x:e}");
        }
        for x in [0.5, 1280.0, 1e20, 123456789012345.0, 0.000001, 1.5e-7, 2.5, -0.5] {
            assert_eq!(Some(number_exact(x)), number(x), "{x}");
        }
    }

    #[test]
    fn strings_read_as_numbers() {
        for (text, x) in [
            ("", 0.0),
            ("  \n", 0.0),
            (" 12 ", 12.0),
            ("1e3", 1000.0),
            ("0x10", 16.0),
            ("0X1f", 31.0),
            ("0o17", 15.0),
            ("0b101", 5.0),
            ("-0x10", f64::NAN),
            ("Infinity", f64::INFINITY),
            ("-Infinity", f64::NEG_INFINITY),
            ("infinity", f64::NAN),
            ("1.", 1.0),
            (".5", 0.5),
            (".", f64::NAN),
            ("1_0", f64::NAN),
            ("+5", 5.0),
            ("--5", f64::NAN),
            ("1e", f64::NAN),
            ("0x", f64::NAN),
            ("\u{feff}7\u{3000}", 7.0),
            ("0x20000000000001", 9007199254740992.0),
            ("0x20000000000003", 9007199254740996.0),
        ] {
            let got = string_to_number(text);
            assert!(got == x || (got.is_nan() && x.is_nan()), "{text:?}: {got}");
        }
        assert!(string_to_number("-0").is_sign_negative());
    }

    #[test]
    fn lower_case_for_the_fixed_texts() {
        assert_eq!(lower_for_fixed("ΣΥΝΔΕΣΗ OK"), "\u{ffff}υνδε\u{ffff}η ok");
        assert_eq!(lower_for_fixed("İD"), "i\u{307}d");
        assert!(fixed_alphabet("リモートID パスワードが必要です 記憶する 确定 確定 Remote ID"));
        assert!(!fixed_alphabet("σ"));
        assert_eq!(slice_utf16_lossy("ab😀c", 3), "ab\u{fffd}");
        assert_eq!(slice_utf16_lossy("ab😀c", 4), "ab😀");
        assert_eq!(slice_utf16_lossy("ab", 200), "ab");
        assert!(math_max(40.0, f64::NAN).is_nan());
        assert_eq!(math_max(40.0, 12.0), 40.0);
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
