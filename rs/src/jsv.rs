//! What JavaScript does with a value `JSON.parse` made, where the TypeScript
//! does arithmetic on it or calls a method of it: for what the binary has to
//! answer after it has acted on the page, where it can no longer hand back
//! (rs/src/compare.rs holds these against bun).
//!
//! A JSON value is never a function, so turning an object into a primitive
//! (`ToPrimitive`) finds no `valueOf` or `toString` of its own that it could
//! call: an object with a member named `toString` has none left and throws
//! TypeError "No default value"; any other object is "[object Object]". A
//! list is its elements joined with commas, each as `String` makes it.

use crate::js;
use crate::json::Json;

/// Why a value has no number.
#[derive(Debug, PartialEq)]
pub enum Coerce {
    /// TypeError "No default value".
    NoDefault,
    /// Something nested deeper than the parser keeps (`Json::Deep`):
    /// JavaScript would answer, or run out of stack; not imitated.
    Deep,
}

/// `!!value`
pub fn truthy(value: &Json) -> bool {
    match value {
        Json::Null => false,
        Json::Bool(b) => *b,
        Json::Num(n) => *n != 0.0 && !n.is_nan(),
        Json::Str(s) => !s.is_empty(),
        Json::Arr(_) | Json::Obj(_) | Json::Deep(_) => true,
    }
}

/// `Number(value)`
pub fn to_number(value: &Json) -> Result<f64, Coerce> {
    match value {
        Json::Null => Ok(0.0),
        Json::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
        Json::Num(n) => Ok(*n),
        Json::Str(s) => Ok(js::string_to_number(s)),
        Json::Arr(_) | Json::Deep(true) => Ok(js::string_to_number(&joined(value)?)),
        Json::Obj(_) => {
            object_string(value)?;
            Ok(f64::NAN)
        }
        Json::Deep(false) => Err(Coerce::Deep),
    }
}

/// `String(object)` of a plain object.
fn object_string(value: &Json) -> Result<&'static str, Coerce> {
    if value.get("toString").is_some() {
        Err(Coerce::NoDefault)
    } else {
        Ok("[object Object]")
    }
}

/// `String(list)`: `list.join(",")`.
fn joined(value: &Json) -> Result<String, Coerce> {
    let Json::Arr(items) = value else { return Err(Coerce::Deep) };
    let mut parts = Vec::with_capacity(items.len());
    for item in items {
        parts.push(match item {
            Json::Null => String::new(),
            Json::Bool(b) => b.to_string(),
            Json::Num(n) => js::number_exact(*n),
            Json::Str(s) => s.clone(),
            Json::Arr(_) | Json::Deep(true) => joined(item)?,
            Json::Obj(_) => object_string(item)?.to_string(),
            Json::Deep(false) => return Err(Coerce::Deep),
        });
    }
    Ok(parts.join(","))
}

/// How JavaScriptCore names a value in "… is not a function. (In '…', '…'
/// is <value>)".
pub fn shown(value: Option<&Json>) -> String {
    match value {
        None => "undefined".into(),
        Some(Json::Null) => "null".into(),
        Some(Json::Bool(b)) => b.to_string(),
        Some(Json::Num(n)) => js::number_exact(*n),
        Some(Json::Str(s)) => format!("\"{s}\""),
        Some(Json::Arr(_) | Json::Deep(true)) => "an instance of Array".into(),
        Some(Json::Obj(_) | Json::Deep(false)) => "an instance of Object".into(),
    }
}

/// The TypeError `b.text.trim()` throws for a box whose text is not a
/// string (`None` for a string): src/ocr.ts matchText.
pub fn text_trim_error(text: Option<&Json>) -> Option<String> {
    match text {
        Some(Json::Str(_)) => None,
        None => Some("undefined is not an object (evaluating 'b.text.trim')".into()),
        Some(Json::Null) => Some("null is not an object (evaluating 'b.text.trim')".into()),
        // A number, true or false, a list: no `trim` there.
        Some(Json::Num(_) | Json::Bool(_) | Json::Arr(_) | Json::Deep(true)) => Some(not_a_function(None)),
        Some(object @ Json::Obj(_)) => Some(not_a_function(object.get("trim"))),
        // An object nested too deep to be a box's text: not reached.
        Some(Json::Deep(false)) => Some(not_a_function(None)),
    }
}

fn not_a_function(trim: Option<&Json>) -> String {
    format!("b.text.trim is not a function. (In 'b.text.trim()', 'b.text.trim' is {})", shown(trim))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::parse_js;

    fn number(text: &str) -> Result<f64, Coerce> {
        to_number(&parse_js(text).unwrap())
    }

    #[test]
    fn values_as_numbers() {
        for (text, x) in [
            ("5", 5.0),
            ("\" 7 \"", 7.0),
            ("\"0x10\"", 16.0),
            ("true", 1.0),
            ("false", 0.0),
            ("null", 0.0),
            ("[]", 0.0),
            ("[5]", 5.0),
            ("[[\"8\"]]", 8.0),
            ("[null]", 0.0),
            ("[[]]", 0.0),
            ("[-0]", 0.0),
            ("[1e21]", 1e21),
        ] {
            assert_eq!(number(text), Ok(x), "{text}");
        }
        assert!(number("-0").unwrap().is_sign_negative());
        assert!(!number("[-0]").unwrap().is_sign_negative());
        for text in ["[1,2]", "{}", "{\"valueOf\": 1}", "\"abc\"", "[true]", "[{}]"] {
            assert!(number(text).unwrap().is_nan(), "{text}");
        }
        for text in ["{\"toString\": 1}", "[{\"toString\": 1}]", "[1, [{\"toString\": null}]]", "{\"toString\": 1, \"valueOf\": 2}"] {
            assert_eq!(number(text), Err(Coerce::NoDefault), "{text}");
        }
    }

    #[test]
    fn what_b_text_trim_throws() {
        let error = |text: &str| text_trim_error(Some(&parse_js(text).unwrap()));
        assert_eq!(error("\"ok\""), None);
        assert_eq!(text_trim_error(None).unwrap(), "undefined is not an object (evaluating 'b.text.trim')");
        assert_eq!(error("null").unwrap(), "null is not an object (evaluating 'b.text.trim')");
        assert_eq!(error("5").unwrap(), "b.text.trim is not a function. (In 'b.text.trim()', 'b.text.trim' is undefined)");
        assert_eq!(error("{\"trim\": \"x\"}").unwrap(), "b.text.trim is not a function. (In 'b.text.trim()', 'b.text.trim' is \"x\")");
        assert_eq!(error("{\"trim\": -0}").unwrap(), "b.text.trim is not a function. (In 'b.text.trim()', 'b.text.trim' is 0)");
        assert_eq!(error("{\"trim\": [1]}").unwrap(), "b.text.trim is not a function. (In 'b.text.trim()', 'b.text.trim' is an instance of Array)");
    }
}
