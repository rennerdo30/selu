//! Data-only scenario inputs with source-located diagnostics.
use crate::RunError;
use selu_diagnostics::{Diagnostic, Span};
use std::{collections::BTreeMap, ops::Range, path::Path};

#[derive(Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Data {
    pub character: Option<String>,
    #[serde(default)]
    pub saved: BTreeMap<String, selu_sim::saved::SavedValue>,
}
pub(crate) struct Scenario {
    pub data: Data,
    path: String,
    text: String,
    keys: Vec<(Vec<String>, Range<usize>)>,
}
impl Scenario {
    pub fn error(&self, message: impl std::fmt::Display, key: &[String]) -> RunError {
        let range = self
            .keys
            .iter()
            .rev()
            .find(|(path, _)| path == key)
            .map(|(_, range)| range.clone())
            .unwrap_or(0..0);
        self.at(message.to_string(), range)
    }
    fn at(&self, message: String, range: Range<usize>) -> RunError {
        RunError::Fixture {
            diagnostic: Diagnostic::error("SELU0631", message).with_span(Span::new(
                &self.path,
                range.start,
                range.end,
            )),
            source: self.text.clone(),
        }
    }
    fn json_error(&self, error: serde_json::Error) -> RunError {
        let mut offset = self
            .text
            .split_inclusive('\n')
            .take(error.line().saturating_sub(1))
            .map(str::len)
            .sum::<usize>()
            + error.column().saturating_sub(1);
        offset = offset.min(self.text.len());
        while !self.text.is_char_boundary(offset) {
            offset -= 1;
        }
        let range = self
            .keys
            .iter()
            .rev()
            .find(|(_, r)| r.start <= offset)
            .map(|(_, r)| r.clone())
            .unwrap_or(offset..offset);
        self.at(format!("invalid scenario fixture: {error}"), range)
    }
}
pub(crate) fn load(path: &Path) -> Result<Scenario, RunError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => "{}".into(),
        Err(e) => return Err(RunError::Io(format!("{}: {e}", path.display()))),
    };
    let mut scenario = Scenario {
        data: Data::default(),
        path: path.to_string_lossy().replace('\\', "/"),
        text,
        keys: vec![],
    };
    // Validate syntax before scanning keys; the scanner only indexes valid JSON.
    serde_json::from_str::<serde_json::Value>(&scenario.text)
        .map_err(|e| scenario.json_error(e))?;
    index_keys(&scenario.text, &mut 0, &mut vec![], &mut scenario.keys);
    scenario.data = serde_json::from_str(&scenario.text).map_err(|e| scenario.json_error(e))?;
    Ok(scenario)
}

// Track object paths, not text matches: strings in values and repeated keys in
// different objects must not steal the span of a saved-global declaration.
fn index_keys(
    text: &str,
    pos: &mut usize,
    path: &mut Vec<String>,
    keys: &mut Vec<(Vec<String>, Range<usize>)>,
) {
    fn ws(text: &str, pos: &mut usize) {
        while text
            .as_bytes()
            .get(*pos)
            .is_some_and(u8::is_ascii_whitespace)
        {
            *pos += 1;
        }
    }
    fn string(text: &str, pos: &mut usize) -> Range<usize> {
        let start = *pos;
        *pos += 1;
        loop {
            match text.as_bytes()[*pos] {
                b'\\' => *pos += 2,
                b'"' => {
                    *pos += 1;
                    return start..*pos;
                }
                _ => *pos += 1,
            }
        }
    }
    ws(text, pos);
    match text.as_bytes()[*pos] {
        b'{' => {
            *pos += 1;
            ws(text, pos);
            while text.as_bytes()[*pos] != b'}' {
                let range = string(text, pos);
                let key: String =
                    serde_json::from_str(&text[range.clone()]).expect("validated JSON key");
                path.push(key);
                keys.push((path.clone(), range));
                ws(text, pos);
                *pos += 1; // colon
                index_keys(text, pos, path, keys);
                path.pop();
                ws(text, pos);
                if text.as_bytes()[*pos] == b',' {
                    *pos += 1;
                    ws(text, pos);
                } else {
                    break;
                }
            }
            *pos += 1;
        }
        b'[' => {
            *pos += 1;
            ws(text, pos);
            let mut index = 0;
            while text.as_bytes()[*pos] != b']' {
                path.push(index.to_string());
                index_keys(text, pos, path, keys);
                path.pop();
                index += 1;
                ws(text, pos);
                if text.as_bytes()[*pos] == b',' {
                    *pos += 1;
                    ws(text, pos);
                } else {
                    break;
                }
            }
            *pos += 1;
        }
        b'"' => {
            string(text, pos);
        }
        _ => {
            while text
                .as_bytes()
                .get(*pos)
                .is_some_and(|b| !b",]} \t\r\n".contains(b))
            {
                *pos += 1;
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saved_global_span_uses_decoded_path_not_a_matching_value_string() {
        let path =
            std::env::temp_dir().join(format!("selu-scenario-spans-{}.json", std::process::id()));
        let text = r#"{"character":"notDeclared","saved":{"not\u0044eclared":{"Number":1}}}"#;
        std::fs::write(&path, text).unwrap();
        let scenario = load(&path).unwrap();
        let error = scenario.error(
            "undeclared saved global",
            &["saved".into(), "notDeclared".into()],
        );
        let RunError::Fixture { diagnostic, source } = error else {
            panic!("missing structured fixture error");
        };
        let span = diagnostic.span.as_ref().unwrap();
        assert_eq!(&source[span.range()], r#""not\u0044eclared""#);
        assert_eq!(span.file, path.to_string_lossy().replace('\\', "/"));
        std::fs::remove_file(path).unwrap();
    }
}
