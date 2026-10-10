//! The options written after the language of a fenced code block.
//!
//! lumis writes its own HTML, so an option only has to say what the block
//! holds; everything else is decided by the formatter. A value is a key and a
//! word separated by `=`, a word surrounded by quotes, or a key on its own
//! which reads as `true`:
//!
//! ```text
//! rust line_numbers highlight_lines=3,7-9
//! ```
//!
//! Options this crate does not read — a title, a frame, a marker — are still
//! read out of the string, so that what they hold is never mistaken for a key
//! of its own.

use std::ops::RangeInclusive;

/// The options of the meta string of a code block
#[derive(Debug, Default, PartialEq)]
pub(crate) struct MetaOptions {
    entries: Vec<(String, Value)>,
}

/// One option, which is either a word or a flag
#[derive(Debug, PartialEq)]
enum Value {
    Word(String),
    Flag(bool),
}

/// The two parts of the info string of a fence: its language, and the options
///
/// The language ends at the first space, as rushdown reads it.
pub(crate) fn parts(info: &str) -> (&str, &str) {
    match info.find(' ') {
        Some(index) => (&info[..index], info[index..].trim_start()),
        None => (info, ""),
    }
}

impl MetaOptions {
    /// Reads the options written after the language of a fence
    pub(crate) fn parse(meta: &str) -> Self {
        let bytes = meta.as_bytes();

        // Values surrounded by delimiters are read first, and blanked out of
        // the string: a value may hold spaces, which the scan below would
        // otherwise read as separate options
        let mut masked = bytes.to_vec();
        let mut entries = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            let (closing, is_word) = match bytes[i] {
                b'"' => (b'"', true),
                b'\'' => (b'\'', true),
                // A regular expression or a line range holds no option of
                // this crate, but its delimiters are read all the same
                b'/' => (b'/', false),
                b'{' => (b'}', false),
                _ => {
                    i += 1;
                    continue;
                }
            };

            let key = key_of(meta, i);
            let opens = i == 0 || bytes[i - 1].is_ascii_whitespace() || key.is_some();
            if !opens {
                i += 1;
                continue;
            }
            let Some((value, end)) = delimited(meta, i, closing) else {
                i += 1;
                continue;
            };
            // A value is only a value when the word ends with it
            if bytes.get(end + 1).is_some_and(|b| !b.is_ascii_whitespace()) {
                i += 1;
                continue;
            }

            if is_word && let Some((_, key)) = key {
                entries.push((key.to_string(), Value::Word(value)));
            }
            let start = key.map(|(start, _)| start).unwrap_or(i);
            masked[start..=end].fill(b' ');
            i = end + 1;
        }

        // What is left holds keys with a word as their value, keys on their
        // own, and `key=true` or `key=false`
        for option in String::from_utf8_lossy(&masked).split_ascii_whitespace() {
            let (key, word) = match option.split_once('=') {
                // `key=` names no value, and reads as a bare key
                Some((key, word)) if !word.is_empty() => (key, word),
                Some((key, _)) => (key, ""),
                None => (option, ""),
            };
            if key.is_empty() {
                continue;
            }
            let value = match word {
                "" | "true" => Value::Flag(true),
                "false" => Value::Flag(false),
                word => Value::Word(word.to_string()),
            };
            entries.push((key.to_string(), value));
        }

        Self { entries }
    }

    /// The flag of the last option with this key, if it holds one
    pub(crate) fn flag(&self, key: &str) -> Option<bool> {
        self.last(key).and_then(|value| match value {
            Value::Flag(flag) => Some(*flag),
            Value::Word(_) => None,
        })
    }

    /// The title the last `title` option gives the block
    ///
    /// The title is written as a word, so that a name holding spaces is
    /// written between quotes. A `title` which holds no name gives none.
    pub(crate) fn title(&self) -> Option<&str> {
        self.last("title").and_then(|value| match value {
            Value::Word(word) if !word.is_empty() => Some(word.as_str()),
            Value::Flag(_) | Value::Word(_) => None,
        })
    }

    /// The lines the last `highlight_lines` option marks
    ///
    /// The lines are written as numbers and ranges of numbers, separated by
    /// commas, or by spaces inside a quoted value, as lumis writes them on its
    /// own command line. A line which cannot be read is left out, and the
    /// lines a block marks count from one.
    pub(crate) fn highlight_lines(&self) -> Vec<RangeInclusive<usize>> {
        const KEYS: [&str; 2] = ["highlight_lines", "hl_lines"];

        KEYS.iter()
            .filter_map(|key| self.last(key))
            .filter_map(|value| match value {
                Value::Word(word) => Some(word),
                Value::Flag(_) => None,
            })
            .flat_map(|word| word.split([',', ' ']))
            .filter_map(line_range)
            .collect()
    }

    /// The value of the last option with this key
    fn last(&self, key: &str) -> Option<&Value> {
        self.entries
            .iter()
            .rev()
            .find(|(option, _)| option.eq_ignore_ascii_case(key))
            .map(|(_, value)| value)
    }
}

/// Reads `3` or `7-9` as a range of lines
fn line_range(word: &str) -> Option<RangeInclusive<usize>> {
    let word = word.trim();
    if word.is_empty() {
        return None;
    }
    match word.split_once('-') {
        Some((from, to)) => {
            let from: usize = from.trim().parse().ok()?;
            let to: usize = to.trim().parse().ok()?;
            Some(from.min(to)..=from.max(to))
        }
        None => {
            let line: usize = word.parse().ok()?;
            Some(line..=line)
        }
    }
}

/// Reads the value which opens with `closing`, returning it with the index of
/// the delimiter which ends it
fn delimited(meta: &str, start: usize, closing: u8) -> Option<(String, usize)> {
    let bytes = meta.as_bytes();
    let mut value = String::new();
    let mut i = start + 1;

    while i < bytes.len() {
        let byte = bytes[i];
        if byte == b'\\' && i + 1 < bytes.len() {
            let escaped = bytes[i + 1];
            if escaped == b'\\' || escaped == closing {
                value.push(escaped as char);
                i += 2;
                continue;
            }
        }
        if byte == closing {
            return Some((value, i));
        }
        let next = meta[i..].chars().next()?;
        value.push(next);
        i += next.len_utf8();
    }

    None
}

/// Reads the key a value opening at `start` belongs to, with the index it
/// starts at
fn key_of(meta: &str, start: usize) -> Option<(usize, &str)> {
    let before = meta[..start].trim_end();
    let end = before.strip_suffix('=')?.len();
    let key = before[..end].trim_end();
    let start = key
        .rfind(|c: char| c.is_whitespace() || matches!(c, '=' | '"' | '\'' | '/' | '{'))
        .map(|i| i + 1)
        .unwrap_or(0);

    Some((start, &key[start..])).filter(|(_, key)| !key.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The options a meta string holds, as the pairs the parser read
    fn entries(meta: &str) -> Vec<(String, Value)> {
        MetaOptions::parse(meta).entries
    }

    fn word(key: &str, value: &str) -> (String, Value) {
        (key.to_string(), Value::Word(value.to_string()))
    }

    fn flag(key: &str) -> (String, Value) {
        (key.to_string(), Value::Flag(true))
    }

    #[test]
    fn reads_a_flag_and_a_word() {
        assert_eq!(
            entries("line_numbers theme=dracula"),
            [flag("line_numbers"), word("theme", "dracula")]
        );
        assert_eq!(MetaOptions::parse("nothing").flag("line_numbers"), None);
    }

    #[test]
    fn reads_a_quoted_word_holding_spaces() {
        assert_eq!(
            entries(r#"title='my file.rs' line_numbers"#),
            [word("title", "my file.rs"), flag("line_numbers")]
        );
    }

    #[test]
    fn reads_a_quoted_word_which_escapes_its_delimiter() {
        assert_eq!(
            entries(r#"title="a \"quoted\" name""#),
            [word("title", r#"a "quoted" name"#)]
        );
    }

    #[test]
    fn reads_a_flag_written_out_as_a_word() {
        assert_eq!(
            MetaOptions::parse("line_numbers=false").flag("line_numbers"),
            Some(false)
        );
    }

    #[test]
    fn reads_the_last_option_written() {
        assert_eq!(
            entries("theme=dracula theme=nord"),
            [word("theme", "dracula"), word("theme", "nord")]
        );
        assert_eq!(
            MetaOptions::parse("theme=dracula theme=nord").last("theme"),
            Some(&Value::Word("nord".to_string()))
        );
    }

    #[test]
    fn matches_a_key_without_regard_to_case() {
        assert_eq!(
            MetaOptions::parse("lineNumbers").flag("LINENUMBERS"),
            Some(true)
        );
    }

    #[test]
    fn reads_the_lines_a_block_marks() {
        assert_eq!(
            MetaOptions::parse("highlight_lines=3,7-9").highlight_lines(),
            vec![3..=3, 7..=9]
        );
    }

    #[test]
    fn reads_the_lines_of_a_quoted_value() {
        assert_eq!(
            MetaOptions::parse(r#"hl_lines="3 7-9""#).highlight_lines(),
            vec![3..=3, 7..=9]
        );
    }

    #[test]
    fn reads_the_lines_of_a_range_written_backwards() {
        assert_eq!(
            MetaOptions::parse("hl_lines=9-7").highlight_lines(),
            vec![7..=9]
        );
    }

    #[test]
    fn leaves_a_line_it_cannot_read_out() {
        assert_eq!(
            MetaOptions::parse("hl_lines=1-3,nope").highlight_lines(),
            vec![1..=3]
        );
    }

    #[test]
    fn leaves_the_inside_of_a_range_and_a_pattern_alone() {
        assert_eq!(
            entries(r#"{1,3-5} ins={2} /foo bar/ theme=nord"#),
            [word("theme", "nord")]
        );
    }

    #[test]
    fn reads_the_title_of_a_block() {
        assert_eq!(
            MetaOptions::parse(r#"title='my file.rs' line_numbers"#).title(),
            Some("my file.rs")
        );
        assert_eq!(MetaOptions::parse("line_numbers theme=nord").title(), None);
        assert_eq!(MetaOptions::parse(r#"title="""#).title(), None);
    }

    #[test]
    fn leaves_a_word_holding_an_equals_sign_alone() {
        assert_eq!(
            entries("theme=nord title=a=b"),
            [word("theme", "nord"), word("title", "a=b")]
        );
    }
}
