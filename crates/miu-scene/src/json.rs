//! Minimal JSON reader used by scene documents.
//!
//! The parser is strict about syntax (RFC 8259) and bounded in nesting depth so a
//! malformed or hostile document cannot exhaust the stack. It produces a
//! [`JsonValue`] tree that the document loader walks; object member order is
//! preserved, which keeps diagnostics and round-tripping predictable.

/// Parsed JSON value.
#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// Number, stored as `f64`.
    Number(f64),
    /// String with escape sequences already decoded.
    String(String),
    /// Array of values.
    Array(Vec<JsonValue>),
    /// Object with members in document order.
    Object(Vec<(String, JsonValue)>),
}

/// Syntax error with the byte offset where parsing stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JsonError {
    /// Byte offset in the original text.
    pub offset: usize,
    /// Short description of the problem.
    pub message: &'static str,
}

impl std::error::Error for JsonError {}

impl core::fmt::Display for JsonError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "JSON error at byte {}: {}", self.offset, self.message)
    }
}

impl JsonValue {
    /// Parses a complete JSON document.
    pub fn parse(text: &str) -> Result<Self, JsonError> {
        let mut parser = Parser {
            bytes: text.as_bytes(),
            index: 0,
            depth: 0,
        };
        parser.skip_whitespace();
        let value = parser.parse_value()?;
        parser.skip_whitespace();
        if parser.index != parser.bytes.len() {
            return Err(parser.error("trailing characters"));
        }
        Ok(value)
    }

    /// Object member lookup.
    pub fn get(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Object(members) => members
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value),
            _ => None,
        }
    }

    /// Array element lookup.
    pub fn at(&self, index: usize) -> Option<&Self> {
        match self {
            Self::Array(values) => values.get(index),
            _ => None,
        }
    }

    /// String contents, if this value is a string.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value.as_str()),
            _ => None,
        }
    }

    /// Boolean contents.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    /// Number contents as `f64`.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            _ => None,
        }
    }

    /// Number contents as `f32`.
    pub fn as_f32(&self) -> Option<f32> {
        self.as_f64().map(|value| value as f32)
    }

    /// Array contents.
    pub fn as_array(&self) -> Option<&[JsonValue]> {
        match self {
            Self::Array(values) => Some(values.as_slice()),
            _ => None,
        }
    }

    /// Object members.
    pub fn as_object(&self) -> Option<&[(String, JsonValue)]> {
        match self {
            Self::Object(members) => Some(members.as_slice()),
            _ => None,
        }
    }

    /// Whether this value is `null`.
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Reads a `[x, y]` array as a two-dimensional vector.
    pub fn as_vec2(&self) -> Option<miu_core::Vec2> {
        let values = self.as_array()?;
        if values.len() < 2 {
            return None;
        }
        Some(miu_core::Vec2::new(values[0].as_f32()?, values[1].as_f32()?))
    }

    /// Reads a `[r, g, b]` or `[r, g, b, a]` array of 0-255 components.
    pub fn as_color(&self) -> Option<miu_core::Color> {
        if let Some(text) = self.as_str() {
            return parse_hex_color(text);
        }
        let values = self.as_array()?;
        if values.len() < 3 {
            return None;
        }
        let channel = |value: &JsonValue| -> u8 {
            value.as_f32().unwrap_or(0.0).clamp(0.0, 255.0) as u8
        };
        let alpha = values.get(3).map(channel).unwrap_or(255);
        Some(miu_core::Color::from_rgba8(
            channel(&values[0]),
            channel(&values[1]),
            channel(&values[2]),
            alpha,
        ))
    }
}

/// Parses `#rgb`, `#rgba`, `#rrggbb`, or `#rrggbbaa` into a premultiplied colour.
pub fn parse_hex_color(text: &str) -> Option<miu_core::Color> {
    let digits = text.strip_prefix('#')?;
    let parse = |slice: &str| u8::from_str_radix(slice, 16).ok();
    let (r, g, b, a) = match digits.len() {
        3 => {
            let r = parse(&digits[0..1])?;
            let g = parse(&digits[1..2])?;
            let b = parse(&digits[2..3])?;
            (r * 17, g * 17, b * 17, 255)
        }
        4 => {
            let r = parse(&digits[0..1])?;
            let g = parse(&digits[1..2])?;
            let b = parse(&digits[2..3])?;
            let a = parse(&digits[3..4])?;
            (r * 17, g * 17, b * 17, a * 17)
        }
        6 => (parse(&digits[0..2])?, parse(&digits[2..4])?, parse(&digits[4..6])?, 255),
        8 => (
            parse(&digits[0..2])?,
            parse(&digits[2..4])?,
            parse(&digits[4..6])?,
            parse(&digits[6..8])?,
        ),
        _ => return None,
    };
    Some(miu_core::Color::from_rgba8(r, g, b, a))
}

struct Parser<'a> {
    bytes: &'a [u8],
    index: usize,
    depth: u32,
}

const MAX_DEPTH: u32 = 128;

impl<'a> Parser<'a> {
    fn error(&self, message: &'static str) -> JsonError {
        JsonError { offset: self.index, message }
    }

    fn skip_whitespace(&mut self) {
        while let Some(byte) = self.bytes.get(self.index) {
            match byte {
                b' ' | b'\t' | b'\n' | b'\r' => self.index += 1,
                _ => break,
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.index).copied()
    }

    fn expect(&mut self, expected: u8) -> Result<(), JsonError> {
        if self.peek() == Some(expected) {
            self.index += 1;
            Ok(())
        } else {
            Err(self.error("unexpected character"))
        }
    }

    fn parse_value(&mut self) -> Result<JsonValue, JsonError> {
        self.skip_whitespace();
        match self.peek() {
            Some(b'{') => self.parse_object(),
            Some(b'[') => self.parse_array(),
            Some(b'"') => self.parse_string().map(JsonValue::String),
            Some(b't') => self.parse_literal(b"true", JsonValue::Bool(true)),
            Some(b'f') => self.parse_literal(b"false", JsonValue::Bool(false)),
            Some(b'n') => self.parse_literal(b"null", JsonValue::Null),
            Some(b'-') | Some(b'0'..=b'9') => self.parse_number(),
            Some(_) => Err(self.error("invalid value")),
            None => Err(self.error("unexpected end of input")),
        }
    }

    fn parse_literal(&mut self, token: &[u8], value: JsonValue) -> Result<JsonValue, JsonError> {
        let end = self.index + token.len();
        if self.bytes.get(self.index..end) == Some(token) {
            self.index = end;
            Ok(value)
        } else {
            Err(self.error("invalid literal"))
        }
    }

    fn parse_object(&mut self) -> Result<JsonValue, JsonError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(self.error("nesting too deep"));
        }
        self.expect(b'{')?;
        let mut members = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.index += 1;
            self.depth -= 1;
            return Ok(JsonValue::Object(members));
        }
        loop {
            self.skip_whitespace();
            let key = self.parse_string()?;
            self.skip_whitespace();
            self.expect(b':')?;
            let value = self.parse_value()?;
            members.push((key, value));
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => {
                    self.index += 1;
                }
                Some(b'}') => {
                    self.index += 1;
                    break;
                }
                _ => return Err(self.error("expected ',' or '}'")),
            }
        }
        self.depth -= 1;
        Ok(JsonValue::Object(members))
    }

    fn parse_array(&mut self) -> Result<JsonValue, JsonError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(self.error("nesting too deep"));
        }
        self.expect(b'[')?;
        let mut values = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.index += 1;
            self.depth -= 1;
            return Ok(JsonValue::Array(values));
        }
        loop {
            let value = self.parse_value()?;
            values.push(value);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => {
                    self.index += 1;
                }
                Some(b']') => {
                    self.index += 1;
                    break;
                }
                _ => return Err(self.error("expected ',' or ']'")),
            }
        }
        self.depth -= 1;
        Ok(JsonValue::Array(values))
    }

    fn parse_string(&mut self) -> Result<String, JsonError> {
        self.expect(b'"')?;
        let mut text = String::new();
        loop {
            let byte = self.peek().ok_or_else(|| self.error("unterminated string"))?;
            self.index += 1;
            match byte {
                b'"' => return Ok(text),
                b'\\' => {
                    let escape = self.peek().ok_or_else(|| self.error("unterminated escape"))?;
                    self.index += 1;
                    match escape {
                        b'"' => text.push('"'),
                        b'\\' => text.push('\\'),
                        b'/' => text.push('/'),
                        b'b' => text.push('\u{8}'),
                        b'f' => text.push('\u{c}'),
                        b'n' => text.push('\n'),
                        b'r' => text.push('\r'),
                        b't' => text.push('\t'),
                        b'u' => {
                            let code = self.parse_hex4()?;
                            if (0xD800..0xDC00).contains(&code) {
                                // Surrogate pair: expect a following `\u` low surrogate.
                                if self.peek() == Some(b'\\') {
                                    self.index += 1;
                                    if self.peek() == Some(b'u') {
                                        self.index += 1;
                                        let low = self.parse_hex4()?;
                                        if (0xDC00..0xE000).contains(&low) {
                                            let combined = 0x10000
                                                + ((code - 0xD800) << 10)
                                                + (low - 0xDC00);
                                            text.push(
                                                char::from_u32(combined).unwrap_or('\u{FFFD}'),
                                            );
                                            continue;
                                        }
                                        text.push('\u{FFFD}');
                                        continue;
                                    }
                                }
                                text.push('\u{FFFD}');
                            } else {
                                text.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
                            }
                        }
                        _ => return Err(self.error("invalid escape")),
                    }
                }
                _ => {
                    // Re-decode UTF-8 sequences byte by byte.
                    let start = self.index - 1;
                    let length = utf8_length(byte);
                    let end = start + length;
                    if end > self.bytes.len() {
                        return Err(self.error("truncated utf-8"));
                    }
                    match core::str::from_utf8(&self.bytes[start..end]) {
                        Ok(slice) => {
                            text.push_str(slice);
                            self.index = end;
                        }
                        Err(_) => return Err(self.error("invalid utf-8")),
                    }
                }
            }
        }
    }

    fn parse_hex4(&mut self) -> Result<u32, JsonError> {
        let slice = self
            .bytes
            .get(self.index..self.index + 4)
            .ok_or_else(|| self.error("truncated unicode escape"))?;
        let text = core::str::from_utf8(slice).map_err(|_| self.error("invalid unicode escape"))?;
        let value = u32::from_str_radix(text, 16).map_err(|_| self.error("invalid unicode escape"))?;
        self.index += 4;
        Ok(value)
    }

    fn parse_number(&mut self) -> Result<JsonValue, JsonError> {
        let start = self.index;
        if self.peek() == Some(b'-') {
            self.index += 1;
        }
        while let Some(byte) = self.peek() {
            match byte {
                b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-' => self.index += 1,
                _ => break,
            }
        }
        let slice = &self.bytes[start..self.index];
        let text = core::str::from_utf8(slice).map_err(|_| self.error("invalid number"))?;
        text.parse::<f64>()
            .map(JsonValue::Number)
            .map_err(|_| self.error("invalid number"))
    }
}

fn utf8_length(first: u8) -> usize {
    if first < 0x80 {
        1
    } else if first >> 5 == 0b110 {
        2
    } else if first >> 4 == 0b1110 {
        3
    } else {
        4
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_documents() {
        let value = JsonValue::parse(
            r#"{"a": [1, 2.5, {"b": null}], "c": true, "d": "text\nline"}"#,
        )
        .expect("valid json");
        assert_eq!(value.get("c").and_then(JsonValue::as_bool), Some(true));
        assert_eq!(
            value.get("a").and_then(|array| array.at(1)).and_then(JsonValue::as_f64),
            Some(2.5)
        );
        assert_eq!(value.get("d").and_then(JsonValue::as_str), Some("text\nline"));
    }

    #[test]
    fn decodes_unicode_escapes() {
        let value = JsonValue::parse(r#""\u00e9\u4e2d""#).expect("valid json");
        assert_eq!(value.as_str(), Some("é中"));
    }

    #[test]
    fn rejects_trailing_and_malformed_input() {
        assert!(JsonValue::parse("{} x").is_err());
        assert!(JsonValue::parse("{").is_err());
        assert!(JsonValue::parse("[1,]").is_err());
        assert!(JsonValue::parse("").is_err());
    }

    #[test]
    fn parses_hex_colours() {
        let color = parse_hex_color("#ff8000cc").expect("valid colour");
        let texel = color.to_rgba8();
        assert_eq!((texel.r, texel.g, texel.b, texel.a), (204, 102, 0, 204));
        assert!(parse_hex_color("ff00ff").is_none());
        assert!(parse_hex_color("#12345").is_none());
    }

    #[test]
    fn vectors_and_colour_arrays_parse() {
        let value = JsonValue::parse(r#"{"p": [4, 5], "c": [255, 0, 0, 128]}"#).unwrap();
        assert_eq!(value.get("p").and_then(JsonValue::as_vec2), Some(miu_core::Vec2::new(4.0, 5.0)));
        let color = value.get("c").and_then(JsonValue::as_color).unwrap();
        assert_eq!(color.to_rgba8().a, 128);
    }

    #[test]
    fn deep_nesting_is_rejected() {
        let text = "[".repeat(MAX_DEPTH as usize + 4);
        assert!(JsonValue::parse(&text).is_err());
    }
}
