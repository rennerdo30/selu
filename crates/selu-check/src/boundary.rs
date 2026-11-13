//! The checked boundary to other addons (owner, 2026-11-10; syntax approved
//! 2026-11-13). Another addon's values enter as `unknown`; nothing of the
//! game may be reached this way.
//!
//! - `addon("Name").global("Global")`: the other addon's global, or null
//!   while that addon is not loaded.
//! - `addonFields(frame).field`: a field another addon stored on a frame,
//!   never a member the game data declares for it.
//! - `value.field` on an `unknown` value: a protected read, `unknown` again.
use crate::{Ctx, Type, err};
use selu_syntax::{Expr, ExprKind};

/// A boundary read the lowering emits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddonGlobal {
    pub addon: String,
    pub global: String,
    /// The load check every selected target has: `C_AddOns.IsAddOnLoaded`
    /// or the older global `IsAddOnLoaded`.
    pub loaded: String,
}

fn identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !matches!(
            s,
            "and"
                | "break"
                | "do"
                | "else"
                | "elseif"
                | "end"
                | "false"
                | "for"
                | "function"
                | "if"
                | "in"
                | "local"
                | "nil"
                | "not"
                | "or"
                | "repeat"
                | "return"
                | "then"
                | "true"
                | "until"
                | "while"
        )
}

impl Ctx<'_> {
    /// Whether any selected target's game data knows `name`: a function,
    /// a global, a frame template or a mixin. Such names stay typed.
    pub(crate) fn game_knows(&self, name: &str) -> bool {
        self.availability.manifests().iter().any(|m| {
            m.function(name).is_some()
                || m.globals.iter().any(|g| g.name == name)
                || m.templates.iter().any(|t| t.name == name)
                || m.data_globals.iter().any(|d| d.name == name)
                || m.widgets.iter().any(|w| w.name == name)
                || m.tables.iter().any(|t| t.name == name)
        }) || self.index.has_global(name)
    }

    fn literal<'e>(
        &mut self,
        arg: Option<&'e Expr>,
        what: &str,
        span: &selu_diagnostics::Span,
    ) -> Option<&'e str> {
        match arg.map(|a| &a.kind) {
            Some(ExprKind::Str(s)) if !s.is_empty() => Some(s.as_str()),
            _ => {
                self.out.push(
                    err("SELU0700", format!("{what} must be a literal string"), span)
                        .with_help("write the name in the source, for example addon(\"Questie\").global(\"QuestieLoader\")"),
                );
                None
            }
        }
    }

    /// `addon("Name").global("Global")`, or a misuse of `addon(...)`.
    pub(crate) fn addon_global(&mut self, e: &Expr, callee: &Expr, args: &[Expr]) -> Option<Type> {
        let ExprKind::Member { object, name } = &callee.kind else {
            if matches!(&callee.kind, ExprKind::Ident(n) if n == "addon" && self.lookup(n).is_none())
            {
                self.out.push(
                    err("SELU0701", "addon(...) names another addon; read one of its globals with .global(\"Name\")", &e.span)
                        .with_help("addon(\"Lorewalker\").global(\"LWDialogFrame\")"),
                );
                return Some(Type::Error);
            }
            return None;
        };
        let ExprKind::Call {
            callee: inner,
            args: inner_args,
        } = &object.kind
        else {
            return None;
        };
        if !matches!(&inner.kind, ExprKind::Ident(n) if n == "addon" && self.lookup(n).is_none()) {
            return None;
        }
        if name != "global" {
            self.out.push(err(
                "SELU0701",
                format!("an addon has no `.{name}`; read its globals with .global(\"Name\")"),
                &callee.span,
            ));
            return Some(Type::Error);
        }
        let addon = self.literal(inner_args.first(), "the addon name", &object.span);
        let global = self.literal(args.first(), "the global name", &e.span);
        if inner_args.len() != 1 || args.len() != 1 {
            self.out.push(err(
                "SELU0701",
                "addon(\"Name\").global(\"Global\") takes one name each",
                &e.span,
            ));
            return Some(Type::Error);
        }
        let (Some(addon), Some(global)) = (addon, global) else {
            return Some(Type::Error);
        };
        let (addon, global) = (addon.to_owned(), global.to_owned());
        if addon.starts_with("Blizzard_") {
            self.out.push(
                err("SELU0702", format!("`{addon}` is part of the game; its values are typed game data, not another addon"), &inner_args[0].span)
                    .with_help("use the game's typed API instead"),
            );
            return Some(Type::Error);
        }
        if !identifier(&global) {
            self.out.push(err(
                "SELU0700",
                format!("`{global}` is not a Lua global name"),
                &args[0].span,
            ));
            return Some(Type::Error);
        }
        if self.game_knows(&global) {
            self.out.push(
                err(
                    "SELU0702",
                    format!(
                        "`{global}` belongs to the game; the boundary is only for other addons"
                    ),
                    &args[0].span,
                )
                .with_help("use it directly: game values are typed"),
            );
            return Some(Type::Error);
        }
        let manifests = self.availability.manifests();
        let loaded = if manifests
            .iter()
            .all(|m| m.function("C_AddOns.IsAddOnLoaded").is_some())
        {
            "C_AddOns.IsAddOnLoaded"
        } else if manifests
            .iter()
            .all(|m| m.function("IsAddOnLoaded").is_some())
        {
            "IsAddOnLoaded"
        } else {
            self.out.push(err(
                "SELU0703",
                "no addon load check exists on every selected target",
                &e.span,
            ));
            return Some(Type::Error);
        };
        self.types.mark_addon_global(
            e.id,
            AddonGlobal {
                addon,
                global,
                loaded: loaded.into(),
            },
        );
        Some(Type::Unknown)
    }

    /// `addonFields(frame).field`.
    pub(crate) fn addon_field(&mut self, e: &Expr, object: &Expr, name: &str) -> Option<Type> {
        let ExprKind::Call { callee, args } = &object.kind else {
            return None;
        };
        if !matches!(&callee.kind, ExprKind::Ident(n) if n == "addonFields" && self.lookup(n).is_none())
        {
            return None;
        }
        if args.len() != 1 {
            self.out.push(err(
                "SELU0704",
                "addonFields(frame) takes one frame",
                &object.span,
            ));
            return Some(Type::Error);
        }
        let frame = self.value(&args[0]);
        let shaped = crate::shape_of(&frame);
        if !matches!(shaped, Type::Widget(..)) {
            self.out.push(err(
                "SELU0704",
                format!("addonFields needs a frame, not {}", crate::a(&frame)),
                &args[0].span,
            ));
            return Some(Type::Error);
        }
        let declared = self.declares_member(&shaped, name) || self.keyed_child(&shaped, name);
        let reviewed = self
            .availability
            .manifests()
            .iter()
            .any(|m| m.member_fields.iter().any(|f| f.entry.name == name));
        if declared || reviewed {
            self.out.push(
                err(
                    "SELU0705",
                    format!("`{name}` is a member the game declares; read it as a typed member"),
                    &e.span,
                )
                .with_help("addonFields only reads fields other addons put on a frame"),
            );
            return Some(Type::Error);
        }
        self.types.mark_addon_field(e.id);
        Some(Type::Unknown)
    }

    /// `addonFields(...)` used as a value: only `.field` reads are allowed.
    pub(crate) fn addon_fields_misuse(&mut self, e: &Expr, callee: &Expr) -> bool {
        if matches!(&callee.kind, ExprKind::Ident(n) if n == "addonFields" && self.lookup(n).is_none())
        {
            self.out.push(err(
                "SELU0704",
                "addonFields(frame) is only read as addonFields(frame).field",
                &e.span,
            ));
            return true;
        }
        false
    }
}
