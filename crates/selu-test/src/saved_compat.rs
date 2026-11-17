//! Compatibility of recorded saved-data descriptors (`selu.saved.toml`).
//!
//! A saved field whose descriptor changed normally needs a version bump and a
//! checked migration (SELU0468). One change is safe without it: an object
//! inside a map, array, set, tuple, union or optional value gains fields that
//! are all optional. Data written under the old shape has no such field, which
//! is exactly what an optional field allows, and no default has to be filled
//! into existing elements. `widens(old, new)` accepts that change and nothing
//! else; anything it cannot parse must match exactly.

#[derive(Debug, PartialEq)]
enum Shape {
    Atom(String),
    Optional(Box<Shape>),
    Array(Box<Shape>),
    Generic(String, Vec<Shape>),
    Union(Vec<Shape>),
    Tuple(Vec<Shape>),
    Object(Vec<(String, Shape)>),
}

struct Parser<'a> {
    text: &'a [u8],
    at: usize,
}

const STOP: &[u8] = b"()[]{}<>,;|?: \t";

impl<'a> Parser<'a> {
    fn skip(&mut self) {
        while self.at < self.text.len() && matches!(self.text[self.at], b' ' | b'\t') {
            self.at += 1;
        }
    }
    fn eat(&mut self, token: &str) -> bool {
        self.skip();
        if self.text[self.at..].starts_with(token.as_bytes()) {
            self.at += token.len();
            true
        } else {
            false
        }
    }
    fn word(&mut self) -> Option<String> {
        self.skip();
        let start = self.at;
        if self.at < self.text.len() && self.text[self.at] == b'"' {
            self.at += 1;
            while self.at < self.text.len() && self.text[self.at] != b'"' {
                if self.text[self.at] == b'\\' {
                    self.at += 1;
                }
                self.at += 1;
            }
            if self.at >= self.text.len() {
                return None;
            }
            self.at += 1;
        } else {
            while self.at < self.text.len() && !STOP.contains(&self.text[self.at]) {
                self.at += 1;
            }
        }
        (self.at > start).then(|| String::from_utf8_lossy(&self.text[start..self.at]).into_owned())
    }
    fn list(&mut self, close: &str, separator: &str) -> Option<Vec<Shape>> {
        let mut items = vec![self.shape()?];
        while self.eat(separator) {
            items.push(self.shape()?);
        }
        self.eat(close).then_some(items)
    }
    fn primary(&mut self) -> Option<Shape> {
        if self.eat("(") {
            let items = self.list(")", "|")?;
            return Some(if items.len() == 1 {
                items.into_iter().next()?
            } else {
                Shape::Union(items)
            });
        }
        if self.eat("[") {
            return Some(Shape::Tuple(self.list("]", ",")?));
        }
        if self.eat("{") {
            let mut fields = Vec::new();
            if self.eat("}") {
                return Some(Shape::Object(fields));
            }
            loop {
                let name = self.word()?;
                if !self.eat(":") {
                    return None;
                }
                fields.push((name, self.shape()?));
                if self.eat(";") {
                    continue;
                }
                return self.eat("}").then_some(Shape::Object(fields));
            }
        }
        let name = self.word()?;
        if name == "recursive" && self.eat("(") {
            let index = self.word()?;
            return self.eat(")").then(|| Shape::Atom(format!("recursive({index})")));
        }
        if self.eat("<") {
            return Some(Shape::Generic(name, self.list(">", ",")?));
        }
        Some(Shape::Atom(name))
    }
    fn shape(&mut self) -> Option<Shape> {
        let mut shape = self.primary()?;
        loop {
            if self.eat("[]") {
                shape = Shape::Array(Box::new(shape));
            } else if self.eat("?") {
                shape = Shape::Optional(Box::new(shape));
            } else {
                return Some(shape);
            }
        }
    }
}

fn parse(text: &str) -> Option<Shape> {
    let mut parser = Parser {
        text: text.as_bytes(),
        at: 0,
    };
    let shape = parser.shape()?;
    parser.skip();
    (parser.at == parser.text.len()).then_some(shape)
}

fn pairwise(old: &[Shape], new: &[Shape]) -> bool {
    old.len() == new.len() && old.iter().zip(new).all(|(a, b)| accepts(a, b))
}

fn accepts(old: &Shape, new: &Shape) -> bool {
    match (old, new) {
        (Shape::Atom(a), Shape::Atom(b)) => a == b,
        (Shape::Optional(a), Shape::Optional(b)) | (Shape::Array(a), Shape::Array(b)) => {
            accepts(a, b)
        }
        (Shape::Generic(a, xs), Shape::Generic(b, ys)) => a == b && pairwise(xs, ys),
        (Shape::Union(xs), Shape::Union(ys)) | (Shape::Tuple(xs), Shape::Tuple(ys)) => {
            pairwise(xs, ys)
        }
        (Shape::Object(xs), Shape::Object(ys)) => {
            xs.iter().all(|(name, a)| {
                ys.iter()
                    .find(|(other, _)| other == name)
                    .is_some_and(|(_, b)| accepts(a, b))
            }) && ys.iter().all(|(name, b)| {
                xs.iter().any(|(other, _)| other == name) || matches!(b, Shape::Optional(_))
            })
        }
        _ => false,
    }
}

/// True when data saved under descriptor `old` is valid under `new` without
/// a migration: the same descriptor, or objects that only gained optional
/// fields.
pub(crate) fn widens(old: &str, new: &str) -> bool {
    if old == new {
        return true;
    }
    match (parse(old), parse(new)) {
        (Some(a), Some(b)) => accepts(&a, &b),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::widens;

    #[test]
    fn an_element_may_gain_optional_fields() {
        assert!(widens(
            "Map<number, { a: number; b: string? }>",
            "Map<number, { a: number; b: string?; required: Map<number, number>? }>"
        ));
        assert!(widens("{ at: number }[]", "{ at: number; note: string? }[]"));
        assert!(widens("{ at: number }?", "{ at: number; seen: boolean? }?"));
    }

    #[test]
    fn every_other_change_still_needs_a_migration() {
        // A new required field leaves old elements without it.
        assert!(!widens("Map<number, { a: number }>", "Map<number, { a: number; b: number }>"));
        // Removing or retyping a field.
        assert!(!widens("{ a: number; b: string? }[]", "{ a: number }[]"));
        assert!(!widens("{ a: number }[]", "{ a: string }[]"));
        assert!(!widens("{ a: number? }[]", "{ a: number }[]"));
        // Keys, containers and unions.
        assert!(!widens("Map<number, string>", "Map<string, string>"));
        assert!(!widens("string[]", "Set<string>"));
        assert!(!widens("(number | string)", "(number | string | boolean)"));
        assert!(!widens("number", "number?"));
    }

    #[test]
    fn literal_and_recursive_atoms_compare_exactly() {
        assert!(widens(
            "{ kind: (\"a;b\" | \"c\") }[]",
            "{ kind: (\"a;b\" | \"c\"); n: number? }[]"
        ));
        assert!(!widens("{ kind: \"a\" }[]", "{ kind: \"b\" }[]"));
        assert!(widens(
            "{ next: recursive(0) }[]",
            "{ next: recursive(0); tag: string? }[]"
        ));
        // Unparseable text never widens.
        assert!(!widens("Map<number", "Map<number, string>"));
    }
}
