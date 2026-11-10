//! Numbered frames that a mixin method creates for every frame using the
//! mixin, with the count and the name taken from the frame itself:
//!
//! ```lua
//! function BarMixin:OnLoad()
//!     local barName = self:GetName();
//!     for i = 1, self.numButtons do
//!         local buttonName;
//!         if self == MainBar then buttonName = "ActionButton"..i;
//!         else buttonName = barName.."Button"..i; end
//!         CreateFrame("CheckButton", buttonName, self, self.buttonTemplate, i);
//!     end
//! end
//! ```
//!
//! The loop bound and the template are XML `KeyValue`s of each named frame
//! whose (inherited) mixins include `BarMixin`. Static only: nothing runs.
//! Anything outside the recognised shape is skipped, never guessed.
use full_moon::{ast::*, node::Node, tokenizer::TokenType};
use selu_manifest::{Evidence, FrameFamily, GlobalEntry, Manifest};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
enum Prefix {
    /// `"Literal"..i`
    Literal(String),
    /// `self:GetName().."Infix"..i`
    SelfName(String),
}

#[derive(Debug, Clone, PartialEq)]
struct NameForm {
    prefix: Prefix,
    /// `if self == Frame then`: only that frame.
    only_for: Option<String>,
    /// The `else` of such a chain: every frame but these.
    except: Vec<String>,
}

#[derive(Debug, Clone)]
struct LoopFamily {
    mixin: String,
    widget: String,
    low: u32,
    bound_field: String,
    template_field: Option<String>,
    names: Vec<NameForm>,
    source: String,
}

/// A named, non-virtual XML frame with what it inherits.
#[derive(Debug, Clone, Default)]
pub struct Instance {
    pub name: String,
    pub mixins: Vec<String>,
    pub numbers: BTreeMap<String, f64>,
    pub strings: BTreeMap<String, String>,
}

#[derive(Default)]
pub struct MixinLoops {
    families: Vec<LoopFamily>,
}

#[derive(Clone, PartialEq)]
enum Ctx {
    Always,
    Only(String),
    Else(Vec<String>),
    Unknown,
}

fn ident(t: &full_moon::tokenizer::TokenReference) -> Option<String> {
    match t.token().token_type() {
        TokenType::Identifier { identifier } => Some(identifier.to_string()),
        _ => None,
    }
}

fn text(n: &impl std::fmt::Display) -> String {
    n.to_string().trim().to_string()
}

fn literal(s: &str) -> Option<String> {
    let s = s.trim();
    let quote = s.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let inner = s.strip_prefix(quote)?.strip_suffix(quote)?;
    (!inner.is_empty() && !inner.contains(['"', '\'', '\\'])).then(|| inner.to_string())
}

fn self_field(s: &str) -> Option<String> {
    let field = s.trim().strip_prefix("self.")?;
    (!field.is_empty() && field.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
        .then(|| field.to_string())
}

#[derive(Default)]
struct Body {
    /// Locals initialised with `self:GetName()`.
    self_names: Vec<String>,
    assignments: BTreeMap<String, Vec<(String, Ctx)>>,
    /// widget, name argument, template argument, line, context.
    creations: Vec<(String, String, Option<String>, usize, Ctx)>,
    index_written: bool,
}

impl Body {
    fn creation(&mut self, call: &FunctionCall, ctx: &Ctx) {
        let full_moon::ast::Prefix::Name(callee) = call.prefix() else {
            return;
        };
        if ident(callee).as_deref() != Some("CreateFrame") {
            return;
        }
        let Some(Suffix::Call(Call::AnonymousCall(call_args))) = call.suffixes().next() else {
            return;
        };
        let FunctionArgs::Parentheses { arguments, .. } = call_args.as_ref() else {
            return;
        };
        let args: Vec<String> = arguments.iter().map(text).collect();
        if args.len() < 2 {
            return;
        }
        let Some(widget) = literal(&args[0]) else {
            return;
        };
        self.creations.push((
            widget,
            args[1].clone(),
            args.get(3).cloned(),
            call.start_position().map_or(0, |p| p.line()),
            ctx.clone(),
        ));
    }

    fn expression(&mut self, e: &Expression, ctx: &Ctx) {
        if let Expression::FunctionCall(call) = e {
            self.creation(call, ctx);
        }
    }

    fn block(&mut self, block: &Block, index: &str, ctx: &Ctx) {
        for stmt in block.stmts() {
            match stmt {
                Stmt::LocalAssignment(local) => {
                    for (i, name) in local.names().iter().enumerate() {
                        let Some(name) = ident(name) else { continue };
                        if name == index {
                            self.index_written = true;
                        }
                        if let Some(e) = local.expressions().iter().nth(i) {
                            let value = text(e);
                            if value == "self:GetName()" {
                                self.self_names.push(name.clone());
                            }
                            self.assignments
                                .entry(name)
                                .or_default()
                                .push((value, ctx.clone()));
                            self.expression(e, ctx);
                        }
                    }
                }
                Stmt::Assignment(assignment) => {
                    for (i, var) in assignment.variables().iter().enumerate() {
                        let Var::Name(name) = var else { continue };
                        let Some(name) = ident(name) else { continue };
                        if name == index {
                            self.index_written = true;
                        }
                        if let Some(e) = assignment.expressions().iter().nth(i) {
                            self.assignments
                                .entry(name)
                                .or_default()
                                .push((text(e), ctx.clone()));
                            self.expression(e, ctx);
                        }
                    }
                }
                Stmt::FunctionCall(call) => self.creation(call, ctx),
                Stmt::If(branch) => {
                    let only = |condition: &Expression| -> Option<String> {
                        let condition = text(condition);
                        let (left, right) = condition.split_once("==")?;
                        let frame = right.trim();
                        (left.trim() == "self"
                            && !frame.is_empty()
                            && frame.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
                        .then(|| frame.to_string())
                    };
                    let mut frames = Vec::new();
                    let mut plain = *ctx == Ctx::Always;
                    let mut arm = |body: &mut Body, condition: &Expression, block: &Block| {
                        let inner = match (plain, only(condition)) {
                            (true, Some(frame)) => {
                                frames.push(frame.clone());
                                Ctx::Only(frame)
                            }
                            _ => {
                                plain = false;
                                Ctx::Unknown
                            }
                        };
                        body.block(block, index, &inner);
                    };
                    arm(self, branch.condition(), branch.block());
                    for else_if in branch.else_if().into_iter().flatten() {
                        arm(self, else_if.condition(), else_if.block());
                    }
                    if let Some(block) = branch.else_block() {
                        let inner = if plain {
                            Ctx::Else(frames)
                        } else {
                            Ctx::Unknown
                        };
                        self.block(block, index, &inner);
                    }
                }
                Stmt::Do(inner) => self.block(inner.block(), index, ctx),
                Stmt::NumericFor(inner) => self.block(inner.block(), index, &Ctx::Unknown),
                Stmt::GenericFor(inner) => self.block(inner.block(), index, &Ctx::Unknown),
                Stmt::While(inner) => self.block(inner.block(), index, &Ctx::Unknown),
                Stmt::Repeat(inner) => self.block(inner.block(), index, &Ctx::Unknown),
                _ => {}
            }
        }
    }
}

impl MixinLoops {
    pub fn parse(&mut self, file: &str, source: &str) -> Result<(), String> {
        let ast = full_moon::parse(source.trim_start_matches('\u{feff}'))
            .map_err(|e| format!("{file}: Lua parse error: {e:?}"))?;
        for stmt in ast.nodes().stmts() {
            let Stmt::FunctionDeclaration(declaration) = stmt else {
                continue;
            };
            let names: Vec<_> = declaration.name().names().iter().collect();
            if declaration.name().method_name().is_none() || names.len() != 1 {
                continue;
            }
            let Some(mixin) = ident(names[0]) else {
                continue;
            };
            // `local barName = self:GetName()` before the loop.
            let mut outer = Body::default();
            let block = declaration.body().block();
            for stmt in block.stmts() {
                if let Stmt::LocalAssignment(local) = stmt {
                    for (i, name) in local.names().iter().enumerate() {
                        if let (Some(name), Some(e)) =
                            (ident(name), local.expressions().iter().nth(i))
                            && text(e) == "self:GetName()"
                        {
                            outer.self_names.push(name);
                        }
                    }
                }
            }
            for stmt in block.stmts() {
                let Stmt::NumericFor(lp) = stmt else { continue };
                if lp.step().is_some() {
                    continue;
                }
                let (Some(index), Ok(low), Some(bound_field)) = (
                    ident(lp.index_variable()),
                    text(lp.start()).parse::<u32>(),
                    self_field(&text(lp.end())),
                ) else {
                    continue;
                };
                let mut body = Body {
                    self_names: outer.self_names.clone(),
                    ..Default::default()
                };
                body.block(lp.block(), &index, &Ctx::Always);
                if body.index_written {
                    continue;
                }
                let form = |value: &str, ctx: &Ctx| -> Option<NameForm> {
                    let parts: Vec<&str> = value.split("..").map(str::trim).collect();
                    let prefix = match parts.as_slice() {
                        [lit, i] if *i == index => Prefix::Literal(literal(lit)?),
                        [owner, lit, i]
                            if *i == index
                                && (*owner == "self:GetName()"
                                    || body.self_names.iter().any(|n| n == owner)) =>
                        {
                            Prefix::SelfName(literal(lit)?)
                        }
                        _ => return None,
                    };
                    let (only_for, except) = match ctx {
                        Ctx::Always => (None, vec![]),
                        Ctx::Only(frame) => (Some(frame.clone()), vec![]),
                        Ctx::Else(frames) => (None, frames.clone()),
                        Ctx::Unknown => return None,
                    };
                    Some(NameForm {
                        prefix,
                        only_for,
                        except,
                    })
                };
                for (widget, name_arg, template_arg, line, ctx) in &body.creations {
                    if *ctx != Ctx::Always {
                        continue;
                    }
                    let names: Option<Vec<NameForm>> = match body.assignments.get(name_arg) {
                        Some(values) => values.iter().map(|(v, c)| form(v, c)).collect(),
                        None => form(name_arg, &Ctx::Always).map(|f| vec![f]),
                    };
                    let Some(names) = names.filter(|n| !n.is_empty()) else {
                        continue;
                    };
                    self.families.push(LoopFamily {
                        mixin: mixin.clone(),
                        widget: widget.clone(),
                        low,
                        bound_field: bound_field.clone(),
                        template_field: template_arg.as_deref().and_then(self_field),
                        names,
                        source: format!("{file}:{line}"),
                    });
                }
            }
        }
        Ok(())
    }

    /// How many creation loops were recognised (reported by the generator).
    pub fn len(&self) -> usize {
        self.families.len()
    }
    pub fn is_empty(&self) -> bool {
        self.families.is_empty()
    }

    /// Adds the numbered globals (and family evidence) every instance gets.
    pub fn extend_manifest(&self, instances: &[Instance], m: &mut Manifest) {
        for family in &self.families {
            for instance in instances
                .iter()
                .filter(|i| i.mixins.contains(&family.mixin))
            {
                let Some(high) = instance
                    .numbers
                    .get(&family.bound_field)
                    .filter(|v| v.fract() == 0.0 && **v >= f64::from(family.low) && **v <= 1000.0)
                    .map(|v| *v as u32)
                else {
                    continue;
                };
                let Some(form) = family.names.iter().find(|f| {
                    f.only_for
                        .as_ref()
                        .is_none_or(|only| *only == instance.name)
                        && !f.except.contains(&instance.name)
                }) else {
                    continue;
                };
                let prefix = match &form.prefix {
                    Prefix::Literal(prefix) => prefix.clone(),
                    Prefix::SelfName(infix) => format!("{}{infix}", instance.name),
                };
                let templates: Vec<String> = family
                    .template_field
                    .as_ref()
                    .and_then(|field| instance.strings.get(field))
                    .map(|t| t.split(',').map(|s| s.trim().to_string()).collect())
                    .unwrap_or_default();
                m.frame_families.push(FrameFamily {
                    prefix: prefix.clone(),
                    suffix: String::new(),
                    widget: family.widget.clone(),
                    min_index: family.low,
                    max_index: high,
                    sources: vec![family.source.clone()],
                });
                for i in family.low..=high {
                    let name = format!("{prefix}{i}");
                    if !m
                        .globals
                        .iter()
                        .any(|g| g.name == name && g.ty.as_deref() == Some(family.widget.as_str()))
                    {
                        m.globals.push(GlobalEntry {
                            name,
                            ty: Some(family.widget.clone()),
                            value: None,
                            evidence: Evidence::Source,
                            templates: templates.clone(),
                        });
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const SOURCE: &str = r#"
BarMixin = {}
function BarMixin:OnLoad()
    local barName = self:GetName();
    for i = 1, self.numButtons do
        local container = CreateFrame("Frame", barName.."ButtonContainer"..i, self, "ContainerTemplate", i);
        local buttonName;
        if self == MainBar then
            buttonName = "ActionButton"..i;
        elseif self == PetBar then
            buttonName = "PetActionButton"..i;
        else
            buttonName = barName.."Button"..i;
        end
        local button = CreateFrame("CheckButton", buttonName, container, self.buttonTemplate, i);
    end
end
function BarMixin:Unsound()
    for i = 1, self.numButtons do
        local n = "Guess"..i
        if math.random() > 0.5 then n = "Other"..i end
        CreateFrame("Button", n)
    end
    for i = 1, self.numButtons do i = 2; CreateFrame("Button", "Written"..i) end
end
"#;
    fn instance(name: &str, buttons: f64) -> Instance {
        Instance {
            name: name.into(),
            mixins: vec!["BarMixin".into()],
            numbers: [("numButtons".to_string(), buttons)].into(),
            strings: [(
                "buttonTemplate".to_string(),
                "BarButtonTemplate".to_string(),
            )]
            .into(),
        }
    }
    #[test]
    fn names_and_counts_come_from_each_frame() {
        let mut loops = MixinLoops::default();
        loops.parse("Bar.lua", SOURCE).unwrap();
        let mut m: Manifest = serde_json::from_str(r#"{"flavour":"fixture","build":{"major":1,"minor":0,"patch":0,"build":1},"interface":1}"#).unwrap();
        let other = Instance {
            name: "Unrelated".into(),
            ..Default::default()
        };
        loops.extend_manifest(
            &[
                instance("MainBar", 12.0),
                instance("PetBar", 10.0),
                instance("MultiBarLeft", 12.0),
                other,
            ],
            &mut m,
        );
        let family = |prefix: &str| m.frame_families.iter().find(|f| f.prefix == prefix);
        let main = family("ActionButton").expect("literal for MainBar");
        assert_eq!(
            (main.widget.as_str(), main.min_index, main.max_index),
            ("CheckButton", 1, 12)
        );
        assert_eq!(family("PetActionButton").unwrap().max_index, 10);
        assert_eq!(family("MultiBarLeftButton").unwrap().max_index, 12);
        assert!(
            family("MainBarButton").is_none(),
            "the else arm excludes MainBar"
        );
        assert_eq!(family("MainBarButtonContainer").unwrap().widget, "Frame");
        assert!(
            family("Guess").is_none() && family("Other").is_none() && family("Written").is_none()
        );
        let button = m
            .globals
            .iter()
            .find(|g| g.name == "MultiBarLeftButton3")
            .unwrap();
        assert_eq!(button.templates, ["BarButtonTemplate"]);
        assert!(!m.globals.iter().any(|g| g.name.starts_with("Unrelated")));
    }
}
