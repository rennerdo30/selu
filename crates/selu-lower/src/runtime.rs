//! Addon runtime registry. Pieces carry dependencies and are emitted in key order.
use crate::{Lowered, Lowerer, errors, lua, raw, saved, wow};
use lua::{LBlock, LStmtKind};
use selu_syntax::Program;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub(crate) struct Piece {
    key: String,
    dependencies: Vec<String>,
    block: LBlock,
    exports: Vec<(String, String)>,
}
pub(crate) fn piece(key: &str, block: LBlock, exports: &[&str], dependencies: &[&str]) -> Piece {
    Piece {
        key: key.into(),
        dependencies: dependencies.iter().map(|s| (*s).into()).collect(),
        block,
        exports: exports.iter().map(|s| ((*s).into(), (*s).into())).collect(),
    }
}
impl Lowerer<'_> {
    /// One ordered prelude for standalone and namespace module lowering.
    pub(crate) fn assemble_file(&mut self, program: &Program, module: bool) -> LBlock {
        let mut captures = vec![];
        self.file_prelude(program, &mut captures);
        let mut body = vec![];
        if module {
            self.module_prologue(&mut body);
        }
        let handlers = program.stmts.iter().any(crate::targets::contains_event);
        if handlers {
            body.push(raw(&if module {
                crate::module_event_prologue(self.addon_errors)
            } else {
                crate::event_prologue(self.addon_errors)
            }));
            body.push(raw(""));
        }
        body.extend(self.block(&program.stmts));
        let entry = self.error_entry_setup(program, handlers);
        let wow = self.wow_prologue();
        let namespace = module
            || wow.is_some()
            || saved::has_saved(program)
            || errors::uses_errors(program)
            || !entry.is_empty();
        let mut out = vec![];
        if namespace {
            out.push(raw("local __selu_addon, __selu_ns = ..."));
        }
        out.extend(captures);
        if namespace && !self.module.type_only {
            out.push(raw("__selu_ns = __selu_ns or {}"));
        }
        out.extend(wow.into_iter().flatten());
        out.extend(saved::setup(program));
        out.extend(self.error_setup(program));
        out.extend(crate::restrictions::emit_restriction_runtime(
            program,
            self.types,
            &self.target.universe,
        ));
        out.extend(entry);
        out.extend(body);
        out
    }
    pub(crate) fn runtime_pieces(&mut self, program: &Program) -> Vec<Piece> {
        let mut pieces =
            crate::restrictions::runtime_pieces(program, self.types, &self.target.universe);
        if self.addon_errors {
            pieces.push(piece(
                "10-errors-state",
                vec![raw(errors::ERROR_STATE)],
                &["__selu_errors"],
                &[],
            ));
        }
        if errors::uses_errors(program) {
            let mut captures = vec![];
            self.error_captures(program, &mut captures);
            pieces.push(piece(
                "11-errors-captures",
                captures,
                &[
                    "__selu_pcall",
                    "__selu_error",
                    "__selu_tostring",
                    "__selu_error_setmetatable",
                    "__selu_error_getmetatable",
                ],
                &[],
            ));
            pieces.push(piece(
                "12-errors",
                vec![raw(errors::ERROR_RUNTIME)],
                &["__selu_new_error", "__selu_wrap_error", "__selu_raise"],
                &["10-errors-state", "11-errors-captures"],
            ));
            if self.wow.error_handler {
                pieces.push(piece(
                    "12-native-error-boundary",
                    vec![raw(errors::ON_ERROR_RUNTIME)],
                    &["__selu_on_error"],
                    &["10-errors-state", "11-errors-captures", "12-errors"],
                ));
            }
            let setup = self.error_setup(program);
            let protect = setup.last().unwrap().clone();
            // Each return width has its own scalar helper; never pack results.
            let text = lua::print_block(&vec![protect.clone()]).text;
            let width = text.matches("value").count();
            let key = format!("13-protect-{width:04}");
            let mut p = piece(
                &key,
                vec![protect],
                &["__selu_protect"],
                &["10-errors-state", "11-errors-captures"],
            );
            p.exports[0].1 = format!("__selu_protect_{width}");
            pieces.push(p);
        }
        if self.error_entry {
            pieces.push(piece(
                "14-entry",
                vec![raw(errors::ERROR_ENTRY)],
                &["__selu_entry"],
                &["10-errors-state"],
            ));
        }
        if program.stmts.iter().any(crate::targets::contains_event) {
            let deps = if self.addon_errors {
                vec!["10-errors-state"]
            } else {
                vec![]
            };
            let mut p = piece(
                "20-events",
                vec![raw(&crate::event_prologue(self.addon_errors))],
                &["__selu_on"],
                &deps,
            );
            // Both standalone and module prologues are matched below.
            p.exports[0].1 = "__selu_events".into();
            pieces.push(p);
        }
        if saved::has_saved(program) {
            let mut b = saved::captures(program);
            b.extend(saved::setup(program));
            let mut p = piece("19-saved", b, &["__selu_saved_runtime"], &[]);
            // Reuse the slot the ADDON_LOADED cleanup clears. Captures stay
            // private to this piece so the cold init helpers can be collected.
            p.exports[0].1 = "__selu_saved".into();
            pieces.push(p);
        }
        if self.wow.used {
            pieces.push(piece(
                "40-wow",
                vec![raw(wow::RUNTIME)],
                &["__selu_wow"],
                &[],
            ));
        }
        let mut target = vec![];
        self.target_prelude(program, &mut target);
        if !target.is_empty() {
            pieces.push(piece("50-has", target, &["__selu_has_event"], &[]));
        }
        pieces
    }
}
fn aliases(p: &Piece, used: &str) -> String {
    let identifiers: std::collections::HashSet<_> = used
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .collect();
    p.exports
        .iter()
        .filter(|(local, _)| identifiers.contains(local.as_str()))
        .map(|(local, field)| format!("local {local} = __selu_ns.{field}\n"))
        .collect()
}
/// Move used runtime pieces into one first file, rebuilding module source maps.
/// Standalone lowering stays self-contained for API consumers and simulator tests.
pub fn share(files: &mut [Lowered]) -> Option<String> {
    let mut registry = BTreeMap::new();
    for file in files.iter() {
        for p in &file.pieces {
            registry.entry(p.key.clone()).or_insert_with(|| p.clone());
        }
    }
    if registry.is_empty() {
        return None;
    }
    let mut runtime = String::from("local __selu_addon, __selu_ns = ...\n");
    for p in registry.values() {
        let text = lua::print_block(&p.block).text;
        let mut block = vec![];
        for dep in &p.dependencies {
            block.push(raw(&aliases(&registry[dep], &text)));
        }
        block.extend(p.block.clone());
        for (local, field) in &p.exports {
            block.push(raw(&format!("__selu_ns.{field} = {local}")));
        }
        runtime.push_str(&format!("-- Selu: {}\n", p.key.split_once('-').unwrap().1));
        runtime.push_str(
            &lua::print_block(&vec![crate::LStmt {
                kind: LStmtKind::Do(block),
                span: None,
            }])
            .text,
        );
    }
    for file in files {
        for p in &file.pieces {
            for s in &p.block {
                file.block.retain(|existing| existing != s);
            }
            if p.key == "20-events" {
                file.block.retain(|s| !matches!(&s.kind, LStmtKind::Raw(t) if t == &crate::module_event_prologue(p.dependencies.len() == 1)));
            }
        }
        // All helper aliases precede source bindings and protected captures.
        let mut prelude = String::from("local __selu_addon, __selu_ns = ...\n");
        let used = lua::print_block(&file.block).text;
        for p in &file.pieces {
            prelude.push_str(&aliases(p, &used));
        }
        file.block.retain(
            |s| !matches!(&s.kind, LStmtKind::Raw(t) if t == "local __selu_addon, __selu_ns = ..." || t == "__selu_ns = __selu_ns or {}"),
        );
        // Module lowering uses an AST local rather than Raw for the varargs.
        file.block.retain(|s| !matches!(&s.kind, LStmtKind::Local(names, _) if names == &["__selu_addon", "__selu_ns"]));
        file.block.insert(0, raw(&prelude));
        let printed = lua::print_block(&file.block);
        file.lua = printed.text;
        file.statement_lines = printed.lines;
    }
    Some(runtime)
}
