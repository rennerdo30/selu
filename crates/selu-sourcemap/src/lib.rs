//! Selu source maps (docs/SIMULATOR.md decisions 9 and 23).
//!
//! One map per generated Lua file, stored as readable JSON beside the build
//! output. A map is a sorted list of generated line ranges, each pointing at
//! a position in a `.selu` source. Lines that no range covers (the shared
//! runtime, preludes, blank lines) are unmapped on purpose.
//!
//! Round 8 groundwork: the format, reader, writer, both lookups and
//! [`map_lua_error`] are implemented and tested here. Package S3 makes the
//! lowering and `selu build` emit this format (today the lowering writes its
//! own simpler `dist/.selu/<Addon>/<file>.lua.map.json`, see
//! `selu_lower::SourceMap`) and adds the end-to-end example tests.

use serde::{Deserialize, Serialize};

/// The format version written into every map. Readers reject other versions.
pub const VERSION: u32 = 1;

/// One source file a map refers to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    /// Project-relative path with forward slashes, e.g. `src/main.selu`.
    pub path: String,
    /// [`content_hash`] of the source text the map was built from, so a
    /// reader can tell a stale map from a current one.
    pub hash: String,
}

/// Generated lines `gen_start..=gen_end` (1-based, inclusive) come from
/// `sources[source]` at `line`/`column` (1-based).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Range {
    pub gen_start: u32,
    pub gen_end: u32,
    pub source: u32,
    pub line: u32,
    pub column: u32,
}

/// A source position a generated line maps to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position<'a> {
    pub path: &'a str,
    pub line: u32,
    pub column: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceMap {
    pub version: u32,
    /// The generated file's name as the client reports it in error
    /// positions, e.g. `main.lua` (matched by base name, case-insensitive).
    pub chunk: String,
    pub sources: Vec<Source>,
    /// Sorted by `gen_start`; nested ranges are allowed and the innermost
    /// (the one starting last among those covering a line) wins.
    pub ranges: Vec<Range>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapError {
    Json(String),
    Version(u32),
    BadSource { range: usize },
    BadRange { range: usize },
}

impl std::fmt::Display for MapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MapError::Json(e) => write!(f, "invalid source map: {e}"),
            MapError::Version(v) => write!(
                f,
                "source map version {v} is not supported (expected {VERSION})"
            ),
            MapError::BadSource { range } => {
                write!(f, "source map range {range} names a missing source")
            }
            MapError::BadRange { range } => {
                write!(f, "source map range {range} is empty or out of order")
            }
        }
    }
}

impl std::error::Error for MapError {}

impl SourceMap {
    pub fn new(chunk: impl Into<String>) -> SourceMap {
        SourceMap {
            version: VERSION,
            chunk: chunk.into(),
            sources: Vec::new(),
            ranges: Vec::new(),
        }
    }

    /// Register a source file and return its index. Paths are normalised to
    /// forward slashes; registering the same path twice returns one index.
    pub fn add_source(&mut self, path: &str, text: &str) -> u32 {
        let path = normalise(path);
        if let Some(i) = self.sources.iter().position(|s| s.path == path) {
            return i as u32;
        }
        self.sources.push(Source {
            path,
            hash: content_hash(text),
        });
        (self.sources.len() - 1) as u32
    }

    /// Append a range. Call [`SourceMap::finish`] once all ranges are in.
    pub fn push(&mut self, range: Range) {
        self.ranges.push(range);
    }

    /// Sort ranges by start (longer first on ties, so the inner one wins).
    pub fn finish(&mut self) {
        self.ranges.sort_by(|a, b| {
            a.gen_start
                .cmp(&b.gen_start)
                .then(b.gen_end.cmp(&a.gen_end))
        });
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("source maps always serialise")
    }

    pub fn from_json(text: &str) -> Result<SourceMap, MapError> {
        let map: SourceMap =
            serde_json::from_str(text).map_err(|e| MapError::Json(e.to_string()))?;
        if map.version != VERSION {
            return Err(MapError::Version(map.version));
        }
        let mut last = 0;
        for (i, r) in map.ranges.iter().enumerate() {
            if r.source as usize >= map.sources.len() {
                return Err(MapError::BadSource { range: i });
            }
            if r.gen_start == 0 || r.gen_end < r.gen_start || r.gen_start < last {
                return Err(MapError::BadRange { range: i });
            }
            last = r.gen_start;
        }
        Ok(map)
    }

    /// The source position of a generated line, or `None` when it is
    /// unmapped. The innermost covering range wins.
    pub fn lookup(&self, gen_line: u32) -> Option<Position<'_>> {
        let end = self.ranges.partition_point(|r| r.gen_start <= gen_line);
        self.ranges[..end]
            .iter()
            .rev()
            .find(|r| r.gen_end >= gen_line)
            .map(|r| Position {
                path: &self.sources[r.source as usize].path,
                line: r.line,
                column: r.column,
            })
    }

    /// Every generated line that starts code for `path:line`, ascending.
    /// Breakpoints use the first one.
    pub fn generated_lines(&self, path: &str, line: u32) -> Vec<u32> {
        let path = normalise(path);
        let Some(source) = self.sources.iter().position(|s| s.path == path) else {
            return Vec::new();
        };
        let mut out: Vec<u32> = self
            .ranges
            .iter()
            .filter(|r| r.source as usize == source && r.line == line)
            .map(|r| r.gen_start)
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    fn matches_chunk(&self, name: &str) -> bool {
        base_name(name).eq_ignore_ascii_case(base_name(&self.chunk))
    }
}

/// FNV-1a 64-bit over the UTF-8 bytes, as `fnv1a64:<16 hex digits>`. Not a
/// security hash; it only detects a map that is older than its source.
pub fn content_hash(text: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("fnv1a64:{h:016x}")
}

fn normalise(path: &str) -> String {
    path.replace('\\', "/")
}

fn base_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Rewrite generated positions in a Lua error message or traceback into
/// source positions: `[string "main.lua"]:42:`, `main.lua:42:` and
/// `Interface\AddOns\Demo\main.lua:42:` become `src/main.selu:7:` when a map
/// for `main.lua` maps line 42. Unknown files and unmapped lines are left as
/// they are, so nothing is ever hidden.
pub fn map_lua_error(message: &str, maps: &[SourceMap]) -> String {
    let bytes = message.as_bytes();
    let mut out = String::with_capacity(message.len());
    let mut copied = 0;
    let mut search = 0;
    while let Some(found) = message[search..].find(".lua") {
        let ext_end = search + found + 4;
        search = ext_end;
        // `[string "x.lua"]:N:` or `x.lua:N:`
        let (quoted, colon) = if message[ext_end..].starts_with("\"]:") {
            (true, ext_end + 2)
        } else if message[ext_end..].starts_with(':') {
            (false, ext_end)
        } else {
            continue;
        };
        let digits_start = colon + 1;
        let digits_end = digits_start
            + message[digits_start..]
                .bytes()
                .take_while(u8::is_ascii_digit)
                .count();
        if digits_end == digits_start || bytes.get(digits_end) != Some(&b':') {
            continue;
        }
        let Ok(line) = message[digits_start..digits_end].parse::<u32>() else {
            continue;
        };
        // Walk back to the start of the file token.
        let mut start = ext_end - 4;
        while start > copied {
            let c = bytes[start - 1];
            if c.is_ascii_whitespace() || c == b'"' || c == b'(' || c == b'\'' {
                break;
            }
            start -= 1;
        }
        let name = &message[start..ext_end];
        let mut token_start = start;
        if quoted {
            const PREFIX: &str = "[string \"";
            if start >= PREFIX.len() && &message[start - PREFIX.len()..start] == PREFIX {
                token_start = start - PREFIX.len();
            } else {
                continue;
            }
        }
        let Some(pos) = maps
            .iter()
            .filter(|m| m.matches_chunk(name))
            .find_map(|m| m.lookup(line))
        else {
            continue;
        };
        out.push_str(&message[copied..token_start]);
        out.push_str(&format!("{}:{}", pos.path, pos.line));
        copied = digits_end;
        search = digits_end;
    }
    out.push_str(&message[copied..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SourceMap {
        let mut m = SourceMap::new("main.lua");
        let s = m.add_source("src\\main.selu", "let x = 1;\n");
        // Line 3..=8 is a function statement at source line 2; its body
        // lines 4 and 6 map to their own statements (decision 23).
        m.push(Range {
            gen_start: 6,
            gen_end: 6,
            source: s,
            line: 4,
            column: 5,
        });
        m.push(Range {
            gen_start: 3,
            gen_end: 8,
            source: s,
            line: 2,
            column: 1,
        });
        m.push(Range {
            gen_start: 4,
            gen_end: 4,
            source: s,
            line: 3,
            column: 5,
        });
        m.finish();
        m
    }

    #[test]
    fn innermost_range_wins_and_gaps_are_unmapped() {
        let m = sample();
        assert_eq!(m.lookup(1), None, "runtime lines stay unmapped");
        assert_eq!(m.lookup(3).unwrap().line, 2);
        assert_eq!(m.lookup(4).unwrap().line, 3);
        assert_eq!(m.lookup(5).unwrap().line, 2);
        assert_eq!(m.lookup(6).unwrap().line, 4);
        assert_eq!(m.lookup(8).unwrap().line, 2);
        assert_eq!(m.lookup(9), None);
        assert_eq!(m.lookup(4).unwrap().path, "src/main.selu");
    }

    #[test]
    fn source_line_to_generated_lines() {
        let m = sample();
        assert_eq!(m.generated_lines("src/main.selu", 3), vec![4]);
        assert_eq!(m.generated_lines("src\\main.selu", 2), vec![3]);
        assert!(m.generated_lines("src/main.selu", 99).is_empty());
        assert!(m.generated_lines("src/other.selu", 2).is_empty());
    }

    #[test]
    fn json_round_trip_and_validation() {
        let m = sample();
        let back = SourceMap::from_json(&m.to_json()).unwrap();
        assert_eq!(back, m);
        let mut wrong = m.clone();
        wrong.version = 9;
        assert_eq!(
            SourceMap::from_json(&wrong.to_json()),
            Err(MapError::Version(9))
        );
        let mut bad = m.clone();
        bad.ranges[0].source = 7;
        assert_eq!(
            SourceMap::from_json(&bad.to_json()),
            Err(MapError::BadSource { range: 0 })
        );
        let mut unsorted = m;
        unsorted.ranges.swap(0, 2);
        assert!(matches!(
            SourceMap::from_json(&unsorted.to_json()),
            Err(MapError::BadRange { .. })
        ));
    }

    #[test]
    fn hashes_detect_changes() {
        assert_eq!(content_hash("a"), content_hash("a"));
        assert_ne!(content_hash("a"), content_hash("b"));
        assert!(content_hash("").starts_with("fnv1a64:"));
    }

    #[test]
    fn lua_errors_are_rewritten_to_source_positions() {
        let maps = [sample()];
        assert_eq!(
            map_lua_error("[string \"main.lua\"]:4: boom", &maps),
            "src/main.selu:3: boom"
        );
        assert_eq!(
            map_lua_error(
                "Interface\\AddOns\\Demo\\main.lua:6: attempt to index nil",
                &maps
            ),
            "src/main.selu:4: attempt to index nil"
        );
        let trace =
            "stack traceback:\n\tmain.lua:4: in function <main.lua:3>\n\tMain.lua:1: in main chunk";
        assert_eq!(
            map_lua_error(trace, &maps),
            "stack traceback:\n\tsrc/main.selu:3: in function <main.lua:3>\n\tMain.lua:1: in main chunk",
            "`<main.lua:3>` has no trailing colon and line 1 is unmapped"
        );
        assert_eq!(map_lua_error("other.lua:4: x", &maps), "other.lua:4: x");
        assert_eq!(map_lua_error("no positions", &maps), "no positions");
    }
}
