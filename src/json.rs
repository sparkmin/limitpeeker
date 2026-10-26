//! A small JSON reader for what providers answer. It reads a whole document
//! into values and refuses anything malformed, so a reply that is not what
//! was asked for fails instead of showing a wrong figure. No crate is added
//! for it.

/// One JSON value. An object keeps its fields in order.
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    Text(String),
    List(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    /// A field of an object; the last one when a key repeats.
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(fields) => fields.iter().rev().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn number(&self) -> Option<f64> {
        match self {
            Json::Number(n) => Some(*n),
            _ => None,
        }
    }
    pub fn text(&self) -> Option<&str> {
        match self {
            Json::Text(t) => Some(t),
            _ => None,
        }
    }
}

/// How deep objects and lists may nest.
const DEEPEST: usize = 64;

/// The value `text` holds, with nothing but white space around it.
pub fn parse(text: &str) -> Option<Json> {
    let mut reader = Reader {
        text,
        bytes: text.as_bytes(),
        at: 0,
    };
    let value = reader.value(0)?;
    reader.space();
    (reader.at == reader.bytes.len()).then_some(value)
}

/// Where in `text` the value at `keys` is, as a range of bytes: the field
/// named by each key in turn, from objects one inside another. The last
/// field wins when a key repeats, as in `get`. None when `text` is not JSON
/// or a key is not there. With it a file can be changed in one place and
/// left as it was everywhere else.
pub fn span(text: &str, keys: &[&str]) -> Option<std::ops::Range<usize>> {
    parse(text)?;
    let mut reader = Reader {
        text,
        bytes: text.as_bytes(),
        at: 0,
    };
    reader.space();
    let start = reader.at;
    reader.value(0)?;
    let mut range = start..reader.at;
    for key in keys {
        reader.at = range.start;
        if !reader.eat(b'{') {
            return None;
        }
        let mut found = None;
        if !reader.eat(b'}') {
            loop {
                reader.space();
                let name = reader.string()?;
                if !reader.eat(b':') {
                    return None;
                }
                reader.space();
                let start = reader.at;
                reader.value(0)?;
                if name == *key {
                    found = Some(start..reader.at);
                }
                if reader.eat(b'}') {
                    break;
                }
                if !reader.eat(b',') {
                    return None;
                }
            }
        }
        range = found?;
    }
    Some(range)
}

/// `text` without the field `name` of the object at `keys`, each time it is
/// there, and every other byte as it was. A field goes with the comma after
/// it, the last one with the comma before it. Of the space around it, what
/// stays is what the next field is set out with: the space before a field
/// written out on its own line, or the line break after one put in at the
/// start of its object. None when `text` is not JSON or there is no object
/// at `keys`; the same text when the field is not there.
pub fn without(text: &str, keys: &[&str], name: &str) -> Option<String> {
    /// A field: its name, where the space before it begins, where its name
    /// starts, where its value ends, and where the comma after it ends.
    struct Field {
        key: String,
        lead: usize,
        start: usize,
        end: usize,
        comma: Option<usize>,
    }
    let mut text = text.to_string();
    loop {
        let object = span(&text, keys)?;
        let cut = {
            let mut reader = Reader {
                text: &text,
                bytes: text.as_bytes(),
                at: object.start,
            };
            if !reader.eat(b'{') {
                return None;
            }
            let mut fields = Vec::new();
            // Looking for the end steps over the space, so the first field's
            // begins just inside the brace.
            let mut lead = object.start + 1;
            if !reader.eat(b'}') {
                loop {
                    reader.space();
                    let start = reader.at;
                    let key = reader.string()?;
                    if !reader.eat(b':') {
                        return None;
                    }
                    reader.value(0)?;
                    let end = reader.at;
                    let comma = reader.eat(b',').then_some(reader.at);
                    fields.push(Field {
                        key,
                        lead,
                        start,
                        end,
                        comma,
                    });
                    match comma {
                        Some(comma) => lead = comma,
                        None if reader.eat(b'}') => break,
                        None => return None,
                    }
                }
            }
            let Some(at) = fields.iter().position(|f| f.key == name) else {
                return Some(text);
            };
            let field = &fields[at];
            match (field.comma, at.checked_sub(1)) {
                (Some(comma), _) => {
                    let next = fields[at + 1].start;
                    let spaced = field.start > field.lead;
                    if spaced || !text[comma..next].contains('\n') {
                        field.start..next
                    } else {
                        field.start..comma
                    }
                }
                (None, Some(before)) => fields[before].end..field.end,
                (None, None) => object.start + 1..object.end - 1,
            }
        };
        text.replace_range(cut, "");
    }
}

/// Text for inside a JSON string.
pub fn escaped(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c < ' ' => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

struct Reader<'a> {
    text: &'a str,
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn space(&mut self) {
        while matches!(self.bytes.get(self.at), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }
    fn eat(&mut self, byte: u8) -> bool {
        self.space();
        let found = self.bytes.get(self.at) == Some(&byte);
        if found {
            self.at += 1;
        }
        found
    }
    fn word(&mut self, word: &str) -> bool {
        let found = self.bytes[self.at..].starts_with(word.as_bytes());
        if found {
            self.at += word.len();
        }
        found
    }
    fn value(&mut self, depth: usize) -> Option<Json> {
        if depth > DEEPEST {
            return None;
        }
        self.space();
        match *self.bytes.get(self.at)? {
            b'{' => {
                self.at += 1;
                let mut fields = Vec::new();
                if self.eat(b'}') {
                    return Some(Json::Object(fields));
                }
                loop {
                    self.space();
                    let key = self.string()?;
                    if !self.eat(b':') {
                        return None;
                    }
                    fields.push((key, self.value(depth + 1)?));
                    if self.eat(b'}') {
                        return Some(Json::Object(fields));
                    }
                    if !self.eat(b',') {
                        return None;
                    }
                }
            }
            b'[' => {
                self.at += 1;
                let mut items = Vec::new();
                if self.eat(b']') {
                    return Some(Json::List(items));
                }
                loop {
                    items.push(self.value(depth + 1)?);
                    if self.eat(b']') {
                        return Some(Json::List(items));
                    }
                    if !self.eat(b',') {
                        return None;
                    }
                }
            }
            b'"' => self.string().map(Json::Text),
            b't' => self.word("true").then_some(Json::Bool(true)),
            b'f' => self.word("false").then_some(Json::Bool(false)),
            b'n' => self.word("null").then_some(Json::Null),
            b'-' | b'0'..=b'9' => self.number().map(Json::Number),
            _ => None,
        }
    }
    fn number(&mut self) -> Option<f64> {
        let start = self.at;
        self.word("-");
        let digits = |reader: &mut Self| {
            let from = reader.at;
            while reader.bytes.get(reader.at).is_some_and(u8::is_ascii_digit) {
                reader.at += 1;
            }
            reader.at > from
        };
        if self.word("0") {
            if self.bytes.get(self.at).is_some_and(u8::is_ascii_digit) {
                return None;
            }
        } else if !digits(self) {
            return None;
        }
        if self.word(".") && !digits(self) {
            return None;
        }
        if self.word("e") || self.word("E") {
            let _ = self.word("+") || self.word("-");
            if !digits(self) {
                return None;
            }
        }
        self.text[start..self.at].parse().ok()
    }
    fn string(&mut self) -> Option<String> {
        if self.bytes.get(self.at) != Some(&b'"') {
            return None;
        }
        self.at += 1;
        let mut out = String::new();
        loop {
            // Plain text runs to the next quote or escape, both ASCII, so
            // the slice always ends on a character's edge.
            let from = self.at;
            while !matches!(self.bytes.get(self.at), None | Some(b'"' | b'\\')) {
                if self.bytes[self.at] < 0x20 {
                    return None;
                }
                self.at += 1;
            }
            out.push_str(&self.text[from..self.at]);
            match *self.bytes.get(self.at)? {
                b'"' => {
                    self.at += 1;
                    return Some(out);
                }
                _ => {
                    self.at += 1;
                    let escaped = *self.bytes.get(self.at)?;
                    self.at += 1;
                    out.push(match escaped {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let first = self.hex()?;
                            let code = if (0xD800..0xDC00).contains(&first) {
                                if !self.word("\\u") {
                                    return None;
                                }
                                let second = self.hex()?;
                                if !(0xDC00..0xE000).contains(&second) {
                                    return None;
                                }
                                0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00)
                            } else {
                                first
                            };
                            char::from_u32(code)?
                        }
                        _ => return None,
                    });
                }
            }
        }
    }
    fn hex(&mut self) -> Option<u32> {
        let digits = self.text.get(self.at..self.at + 4)?;
        if !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        self.at += 4;
        u32::from_str_radix(digits, 16).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reply_reads_as_values() {
        let reply = parse(
            r#" {"id": 2, "result": {"rateLimits": {"primary": {"usedPercent": 28,
                "windowDurationMins": 300, "resetsAt": 1790000000}, "secondary": null,
                "planType": "plus", "credits": [true, false, -1.5e2]}}} "#,
        )
        .unwrap();
        let limits = reply
            .get("result")
            .and_then(|r| r.get("rateLimits"))
            .unwrap();
        let primary = limits.get("primary").unwrap();
        assert_eq!(
            primary.get("usedPercent").and_then(Json::number),
            Some(28.0)
        );
        assert_eq!(primary.get("resetsAt").and_then(Json::number), Some(1.79e9));
        assert_eq!(limits.get("secondary"), Some(&Json::Null));
        assert_eq!(limits.get("planType").and_then(Json::text), Some("plus"));
        assert_eq!(
            limits.get("credits"),
            Some(&Json::List(vec![
                Json::Bool(true),
                Json::Bool(false),
                Json::Number(-150.0)
            ]))
        );
        assert_eq!(reply.get("missing"), None);
    }

    #[test]
    fn text_keeps_every_character() {
        assert_eq!(
            parse(r#""a\"b\\c\/\n\t\u00e9\ud83d\ude00 한""#),
            Some(Json::Text("a\"b\\c/\n\té\u{1f600} 한".into()))
        );
    }

    #[test]
    fn anything_malformed_is_refused() {
        // A reply that is not JSON fails instead of reading as something
        // else.
        for wrong in [
            "",
            "{",
            "{\"a\" 1}",
            "{\"a\":1,}",
            "[1 2]",
            "01",
            "1.",
            "-",
            ".5",
            "+1",
            "tru",
            "nul",
            "\"open",
            "\"\\x\"",
            "\"\\ud83d\"",
            "\"\\udc00\"",
            "\"a\u{1}\"",
            "{} {}",
            "NaN",
        ] {
            assert_eq!(parse(wrong), None, "{wrong:?}");
        }
        let deep = "[".repeat(DEEPEST + 2) + &"]".repeat(DEEPEST + 2);
        assert_eq!(parse(&deep), None);
        let fine = "[".repeat(DEEPEST) + &"]".repeat(DEEPEST);
        assert!(parse(&fine).is_some());
    }

    #[test]
    fn a_value_is_found_where_it_is_written() {
        // The bytes of a field, so one value can be changed and the rest of
        // the text left as it was.
        let text = r#" {"a": {"b\\c": [1, {"x": 2}], "d": false, "d": true}, "e": {}} "#;
        let at = |keys: &[&str]| span(text, keys).map(|r| &text[r]);
        assert_eq!(at(&[]), Some(text.trim()));
        assert_eq!(at(&["a", "b\\c"]), Some(r#"[1, {"x": 2}]"#));
        // The last of a repeated key, as `get` reads it.
        assert_eq!(at(&["a", "d"]), Some("true"));
        assert_eq!(at(&["e"]), Some("{}"));
        assert_eq!(at(&["e", "x"]), None);
        assert_eq!(at(&["a", "b\\c", "x"]), None);
        assert_eq!(at(&["missing"]), None);
        assert_eq!(span("{\"a\": 1", &["a"]), None);
        assert_eq!(escaped("a\"b\\c\n"), "a\\\"b\\\\c\\u000a");
    }

    #[test]
    fn a_field_is_taken_out_and_the_rest_left() {
        // Only the field and one comma go, so a file another program keeps
        // reads as it did.
        let text = r#"{"p": {"a": {"x": [1, 2]}, "b": 2, "c": "3"}, "q": true}"#;
        let out = |name: &str| without(text, &["p"], name).unwrap();
        assert_eq!(out("a"), r#"{"p": {"b": 2, "c": "3"}, "q": true}"#);
        assert_eq!(
            out("b"),
            r#"{"p": {"a": {"x": [1, 2]}, "c": "3"}, "q": true}"#
        );
        assert_eq!(
            out("c"),
            r#"{"p": {"a": {"x": [1, 2]}, "b": 2}, "q": true}"#
        );
        assert_eq!(out("missing"), text);
        assert_eq!(
            without(r#"{"p": {"only": 1}, "q": 2}"#, &["p"], "only").as_deref(),
            Some(r#"{"p": {}, "q": 2}"#)
        );
        // Written out over lines, as Claude Code writes its file.
        let lines = "{\n  \"p\": {\n    \"a\": 1,\n    \"b\": {\n      \"y\": 2\n    }\n  },\n  \"z\": 3\n}";
        assert_eq!(
            without(lines, &["p"], "b").unwrap(),
            "{\n  \"p\": {\n    \"a\": 1\n  },\n  \"z\": 3\n}"
        );
        assert_eq!(
            without(lines, &["p"], "a").unwrap(),
            "{\n  \"p\": {\n    \"b\": {\n      \"y\": 2\n    }\n  },\n  \"z\": 3\n}"
        );
        // One put in at the start of its object, as trusting a folder puts
        // it, goes and leaves the line as it was before.
        let put = "{\n  \"p\": {\"k\": {\"t\": true},\n    \"a\": 1\n  }\n}";
        assert_eq!(
            without(put, &["p"], "k").unwrap(),
            "{\n  \"p\": {\n    \"a\": 1\n  }\n}"
        );
        // A key with an escape, and one written twice: each goes.
        let twice = r#"{"p": {"C:\\a": 1, "k": 2, "C:\\a": 3}}"#;
        assert_eq!(
            without(twice, &["p"], "C:\\a").as_deref(),
            Some(r#"{"p": {"k": 2}}"#)
        );
        for (from, name) in [(text, "a"), (text, "c"), (lines, "b"), (twice, "C:\\a")] {
            assert!(parse(&without(from, &["p"], name).unwrap()).is_some());
        }
        assert_eq!(without(text, &["q"], "a"), None);
        assert_eq!(without(text, &["missing"], "a"), None);
        assert_eq!(without("{\"p\": {", &["p"], "a"), None);
    }
}
