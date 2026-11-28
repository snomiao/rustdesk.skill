//! `JSON.parse`, for the files and the program output the TypeScript reads:
//! the grammar of ECMA-404 and nothing more (no comments, no trailing
//! commas, no byte order mark), whitespace being space, tab, CR and LF.
//!
//! Two things valid JSON can hold are not taken: a `\u` escape that leaves a
//! lone surrogate (JavaScript keeps it, a Rust string cannot), and nesting
//! deeper than `MAX_DEPTH`. Both are `Parse::Unsure`, and the caller hands
//! back.

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    /// The members as written, a name given twice included.
    Obj(Vec<(String, Json)>),
}

#[derive(Debug, PartialEq)]
pub enum Parse {
    /// `JSON.parse` throws.
    Invalid,
    /// Valid, or perhaps valid, but not taken here.
    Unsure,
}

pub const MAX_DEPTH: usize = 200;

impl Json {
    /// `value[name]` for an object: the last member of that name.
    pub fn get(&self, name: &str) -> Option<&Json> {
        match self {
            Json::Obj(members) => members.iter().rev().find(|(key, _)| key == name).map(|(_, value)| value),
            _ => None,
        }
    }
}

pub fn parse(text: &str) -> Result<Json, Parse> {
    let mut parser = Parser { bytes: text.as_bytes(), at: 0, lone: false };
    parser.blank();
    let value = parser.value(0)?;
    parser.blank();
    if parser.at != parser.bytes.len() {
        return Err(Parse::Invalid);
    }
    // Only now is it known that the text is valid.
    if parser.lone {
        return Err(Parse::Unsure);
    }
    Ok(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    at: usize,
    /// A `\u` escape left a lone surrogate somewhere.
    lone: bool,
}

impl Parser<'_> {
    fn blank(&mut self) {
        while matches!(self.bytes.get(self.at), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn eat(&mut self, byte: u8) -> bool {
        if self.bytes.get(self.at) == Some(&byte) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn word(&mut self, word: &[u8]) -> Result<(), Parse> {
        if self.bytes[self.at..].starts_with(word) {
            self.at += word.len();
            Ok(())
        } else {
            Err(Parse::Invalid)
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json, Parse> {
        match self.bytes.get(self.at) {
            Some(b'{') => {
                if depth >= MAX_DEPTH {
                    return Err(Parse::Unsure);
                }
                self.at += 1;
                let mut members = Vec::new();
                self.blank();
                if self.eat(b'}') {
                    return Ok(Json::Obj(members));
                }
                loop {
                    self.blank();
                    if self.bytes.get(self.at) != Some(&b'"') {
                        return Err(Parse::Invalid);
                    }
                    let key = self.string()?;
                    self.blank();
                    if !self.eat(b':') {
                        return Err(Parse::Invalid);
                    }
                    self.blank();
                    let value = self.value(depth + 1)?;
                    members.push((key, value));
                    self.blank();
                    if self.eat(b',') {
                        continue;
                    }
                    if self.eat(b'}') {
                        return Ok(Json::Obj(members));
                    }
                    return Err(Parse::Invalid);
                }
            }
            Some(b'[') => {
                if depth >= MAX_DEPTH {
                    return Err(Parse::Unsure);
                }
                self.at += 1;
                let mut items = Vec::new();
                self.blank();
                if self.eat(b']') {
                    return Ok(Json::Arr(items));
                }
                loop {
                    self.blank();
                    items.push(self.value(depth + 1)?);
                    self.blank();
                    if self.eat(b',') {
                        continue;
                    }
                    if self.eat(b']') {
                        return Ok(Json::Arr(items));
                    }
                    return Err(Parse::Invalid);
                }
            }
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(b't') => self.word(b"true").map(|()| Json::Bool(true)),
            Some(b'f') => self.word(b"false").map(|()| Json::Bool(false)),
            Some(b'n') => self.word(b"null").map(|()| Json::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(Parse::Invalid),
        }
    }

    fn digits(&mut self) -> usize {
        let start = self.at;
        while matches!(self.bytes.get(self.at), Some(b'0'..=b'9')) {
            self.at += 1;
        }
        self.at - start
    }

    fn number(&mut self) -> Result<Json, Parse> {
        let start = self.at;
        self.eat(b'-');
        match self.bytes.get(self.at) {
            Some(b'0') => self.at += 1,
            Some(b'1'..=b'9') => {
                self.digits();
            }
            _ => return Err(Parse::Invalid),
        }
        if self.eat(b'.') && self.digits() == 0 {
            return Err(Parse::Invalid);
        }
        if matches!(self.bytes.get(self.at), Some(b'e' | b'E')) {
            self.at += 1;
            if !self.eat(b'+') {
                self.eat(b'-');
            }
            if self.digits() == 0 {
                return Err(Parse::Invalid);
            }
        }
        let text = std::str::from_utf8(&self.bytes[start..self.at]).map_err(|_| Parse::Invalid)?;
        // Correctly rounded, as JavaScript reads it; too large is Infinity.
        text.parse::<f64>().map(Json::Num).map_err(|_| Parse::Unsure)
    }

    fn hex4(&mut self) -> Result<u32, Parse> {
        let hex = self.bytes.get(self.at..self.at + 4).ok_or(Parse::Invalid)?;
        let text = std::str::from_utf8(hex).map_err(|_| Parse::Invalid)?;
        if !text.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Parse::Invalid);
        }
        self.at += 4;
        u32::from_str_radix(text, 16).map_err(|_| Parse::Invalid)
    }

    fn string(&mut self) -> Result<String, Parse> {
        self.at += 1;
        let mut out = String::new();
        loop {
            let start = self.at;
            while let Some(&b) = self.bytes.get(self.at) {
                if b == b'"' || b == b'\\' || b < 0x20 {
                    break;
                }
                self.at += 1;
            }
            out.push_str(std::str::from_utf8(&self.bytes[start..self.at]).map_err(|_| Parse::Invalid)?);
            match self.bytes.get(self.at) {
                Some(b'"') => {
                    self.at += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.at += 1;
                    let escape = *self.bytes.get(self.at).ok_or(Parse::Invalid)?;
                    self.at += 1;
                    match escape {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let unit = self.hex4()?;
                            if (0xd800..0xdc00).contains(&unit) && self.bytes[self.at..].starts_with(b"\\u") {
                                let save = self.at;
                                self.at += 2;
                                let low = self.hex4()?;
                                if (0xdc00..0xe000).contains(&low) {
                                    let c = 0x10000 + ((unit - 0xd800) << 10) + (low - 0xdc00);
                                    out.push(char::from_u32(c).ok_or(Parse::Invalid)?);
                                    continue;
                                }
                                self.at = save;
                                self.lone = true;
                            } else if (0xd800..0xe000).contains(&unit) {
                                self.lone = true;
                            } else {
                                out.push(char::from_u32(unit).ok_or(Parse::Invalid)?);
                            }
                        }
                        _ => return Err(Parse::Invalid),
                    }
                }
                // A control character, or the end of the text.
                _ => return Err(Parse::Invalid),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_parse_takes_what_it_takes() {
        assert_eq!(parse(" [\"a\", 1.5e2, -0, true, false, null, {\"k\": []}] \r\n"), Ok(Json::Arr(vec![
            Json::Str("a".into()),
            Json::Num(150.0),
            Json::Num(-0.0),
            Json::Bool(true),
            Json::Bool(false),
            Json::Null,
            Json::Obj(vec![("k".into(), Json::Arr(vec![]))]),
        ])));
        assert_eq!(parse(r#""\u00e9\ud83d\ude00\/\b""#), Ok(Json::Str("é😀/\u{8}".into())));
        assert_eq!(parse("1e400"), Ok(Json::Num(f64::INFINITY)));
        let object = parse(r#"{"w": 1, "w": 2}"#).unwrap();
        assert_eq!(object.get("w"), Some(&Json::Num(2.0)));
        for text in ["", " ", "[1,]", "{\"a\":1,}", "// c\n1", "\u{feff}1", "01", "1.", ".5", "+1", "-", "1e", "'a'", "\"a\tb\"", "\"\\x41\"", "[1] x", "nul", "\"\\u12\"", "{a: 1}", "NaN", "[\"\\ud800\" x]"] {
            assert_eq!(parse(text), Err(Parse::Invalid), "{text:?}");
        }
        for text in ["\"\\ud800\"", "\"\\udc00\"", "\"\\ud800\\u0041\""] {
            assert_eq!(parse(text), Err(Parse::Unsure), "{text:?}");
        }
        assert_eq!(parse(&format!("{}{}", "[".repeat(MAX_DEPTH + 1), "]".repeat(MAX_DEPTH + 1))), Err(Parse::Unsure));
        assert!(parse(&format!("{}{}", "[".repeat(MAX_DEPTH), "]".repeat(MAX_DEPTH))).is_ok());
    }
}
