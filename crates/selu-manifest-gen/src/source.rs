//! Static Lua 5.1 AST discovery. Never executes addon code.
use full_moon::{
    ast::*,
    node::Node,
    tokenizer::{TokenReference, TokenType},
    visitors::Visitor,
};
use selu_manifest::{Evidence, FunctionEntry, FunctionSource, Manifest};
use std::collections::{BTreeMap, BTreeSet};
fn name(t: &TokenReference) -> String {
    match t.token().token_type() {
        TokenType::Identifier { identifier } => identifier.to_string(),
        _ => "...".into(),
    }
}
fn start(n: &impl Node) -> usize {
    n.start_position().map_or(0, |p| p.bytes())
}
fn end(n: &impl Node) -> usize {
    n.end_position().map_or(usize::MAX, |p| p.bytes())
}
#[derive(Default)]
struct Scan {
    blocks: Vec<(usize, usize)>,
    depth: usize,
    repeat_ends: BTreeMap<usize, usize>,
    locals: Vec<(String, usize, usize)>,
    definitions: Vec<(String, usize, Vec<String>, bool)>,
    calls: Vec<(String, usize)>,
}
impl Scan {
    fn bind(&mut self, n: String, from: usize, to: usize) {
        self.locals.push((n, from, to));
    }
    fn local(&self, n: &str, pos: usize) -> bool {
        self.locals
            .iter()
            .any(|(v, a, b)| v == n && *a <= pos && pos <= *b)
    }
}
impl Visitor for Scan {
    fn visit_block(&mut self, n: &Block) {
        self.blocks.push((
            start(n),
            self.repeat_ends
                .get(&start(n))
                .copied()
                .unwrap_or_else(|| end(n)),
        ));
    }
    fn visit_repeat(&mut self, n: &Repeat) {
        self.repeat_ends.insert(start(n.block()), end(n));
    }
    fn visit_block_end(&mut self, _: &Block) {
        self.blocks.pop();
    }
    fn visit_local_assignment(&mut self, n: &LocalAssignment) {
        let to = self.blocks.last().unwrap().1;
        for t in n.names() {
            self.bind(name(t), end(n), to);
        }
    }
    fn visit_local_function(&mut self, n: &LocalFunction) {
        self.bind(name(n.name()), start(n), self.blocks.last().unwrap().1);
    }
    fn visit_function_body(&mut self, n: &FunctionBody) {
        self.depth += 1;
        for p in n.parameters() {
            if let Parameter::Name(t) = p {
                self.bind(name(t), start(n.block()), end(n.block()));
            }
        }
        // Method self is handled by the declaration; implicit self is never a global call.
    }
    fn visit_function_body_end(&mut self, _: &FunctionBody) {
        self.depth -= 1;
    }
    fn visit_function_declaration(&mut self, n: &FunctionDeclaration) {
        if n.name().method_name().is_some() {
            self.bind(
                "self".into(),
                start(n.body().block()),
                end(n.body().block()),
            );
        } else if n.name().names().len() == 1 {
            let t = n.name().names().iter().next().unwrap();
            self.definitions.push((
                name(t),
                start(n),
                n.body()
                    .parameters()
                    .iter()
                    .map(|p| match p {
                        Parameter::Name(t) => name(t),
                        _ => "...".into(),
                    })
                    .collect(),
                self.depth == 0,
            ));
        }
    }
    fn visit_assignment(&mut self, n: &Assignment) {
        for (v, e) in n.variables().iter().zip(n.expressions().iter()) {
            if let (Var::Name(t), Expression::Function(f)) = (v, e) {
                self.definitions.push((
                    name(t),
                    start(n),
                    f.body()
                        .parameters()
                        .iter()
                        .map(|p| match p {
                            Parameter::Name(t) => name(t),
                            _ => "...".into(),
                        })
                        .collect(),
                    self.depth == 0,
                ));
            }
        }
    }
    fn visit_function_call(&mut self, n: &FunctionCall) {
        if let Prefix::Name(t) = n.prefix()
            && matches!(
                n.suffixes().next(),
                Some(Suffix::Call(Call::AnonymousCall(_)))
            )
        {
            self.calls.push((name(t), start(n)));
        }
    }
    fn visit_numeric_for(&mut self, n: &NumericFor) {
        self.bind(name(n.index_variable()), start(n.block()), end(n.block()));
    }
    fn visit_generic_for(&mut self, n: &GenericFor) {
        for t in n.names() {
            self.bind(name(t), start(n.block()), end(n.block()));
        }
    }
}
#[derive(Default)]
pub struct Sources {
    definitions: BTreeMap<String, (String, Vec<String>)>,
    all_definitions: BTreeSet<String>,
    calls: BTreeMap<String, String>,
}
impl Sources {
    pub fn parse(&mut self, file: &str, source: &str) -> Result<(), String> {
        let ast = full_moon::parse(source.trim_start_matches('\u{feff}'))
            .map_err(|e| format!("{file}: Lua parse error: {e:?}"))?;
        let mut scan = Scan::default();
        scan.visit_ast(&ast);
        for (n, pos, args, top) in &scan.definitions {
            if !scan.local(n, *pos) {
                self.all_definitions.insert(n.clone());
                if *top {
                    self.definitions
                        .entry(n.clone())
                        .or_insert((file.into(), args.clone()));
                }
            }
        }
        for (n, pos) in &scan.calls {
            if !scan.local(n, *pos) {
                self.calls.entry(n.clone()).or_insert(file.into());
            }
        }
        Ok(())
    }
    pub fn extend_manifest(self, m: &mut Manifest) -> (usize, usize) {
        let mut known: BTreeSet<String> = m.functions.iter().map(|f| f.name.clone()).collect();
        let mut counts = (0, 0);
        for (n, (file, args)) in self.definitions {
            if known.insert(n.clone()) {
                m.functions.push(entry(n, file, "defined", args));
                counts.0 += 1;
            }
        }
        for (n, file) in self.calls {
            if !self.all_definitions.contains(&n) && known.insert(n.clone()) {
                m.functions.push(entry(n, file, "called", vec![]));
                counts.1 += 1;
            }
        }
        counts
    }
}
fn entry(name: String, file: String, kind: &str, params: Vec<String>) -> FunctionEntry {
    FunctionEntry {
        name,
        params,
        available: true,
        restrictions: vec![],
        returns_secret: false,
        evidence: Evidence::Source,
        signature: None,
        provenance: Some(FunctionSource {
            file: file
                .strip_prefix("Interface/AddOns/")
                .unwrap_or(&file)
                .into(),
            kind: kind.into(),
        }),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lexical_globals_and_first_provenance() {
        let mut s = Sources::default();
        s.parse(
            "a.lua",
            r#"
  Native(); CrossFile(); Before(); local Before = function() end; Before();
  function Defined(x, ...) Native(); end
  Assigned = function(a,b) end
  local function Hidden() end; Hidden();
  local Alias = Native; Alias();
  object.Field(); object:Method();
  function object.Field() end
  function Wrapper(Native) Native(); end
  do local Scoped = function() end; Scoped(); end; Scoped();
  for Key in pairs({}) do Key(); end
  for Number=1,3 do Number(); end
  local Ghost = "TextFake()" -- CommentFake()
  "#,
        )
        .unwrap();
        s.parse("b.lua", "function CrossFile() end; Native()")
            .unwrap();
        let mut m: Manifest = serde_json::from_str(
            r#"{"flavour":"test","build":{"major":1,"minor":0,"patch":0,"build":1},"interface":1}"#,
        )
        .unwrap();
        s.extend_manifest(&mut m);
        for n in [
            "Native",
            "CrossFile",
            "Defined",
            "Assigned",
            "Wrapper",
            "Scoped",
            "Before",
            "pairs",
        ] {
            assert!(m.function(n).is_some(), "{n}");
        }
        for n in [
            "Hidden",
            "Alias",
            "Field",
            "Method",
            "Key",
            "Number",
            "Ghost",
            "TextFake",
            "CommentFake",
        ] {
            assert!(m.function(n).is_none(), "{n}");
        }
        assert_eq!(m.function("Defined").unwrap().params, ["x", "..."]);
        assert_eq!(m.function("Assigned").unwrap().params, ["a", "b"]);
        assert_eq!(
            m.function("Native")
                .unwrap()
                .provenance
                .as_ref()
                .unwrap()
                .file,
            "a.lua"
        );
        assert_eq!(
            m.function("CrossFile")
                .unwrap()
                .provenance
                .as_ref()
                .unwrap()
                .kind,
            "defined"
        );
        assert!(Sources::default().parse("bad.lua", "function (").is_err());
    }
}
