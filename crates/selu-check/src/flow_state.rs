//! Effect-aware flow states. Variable facts are keyed by declaration identity.
use crate::{BindingId, Ctx, FactKey, FlowState, Narrow, Narrowings, Type};
use selu_syntax::{
    Expr, ExprKind, Stmt,
    visit::{self, Child},
};
use std::collections::HashSet;
pub(crate) fn intersect(states: Vec<FlowState>) -> FlowState {
    let mut states = states.into_iter();
    let Some(mut out) = states.next() else {
        return FlowState::new();
    };
    for state in states {
        out.retain(|key, value| {
            if let Some(other) = state.get(key)
                && value.ty == other.ty
                && value.fn_depth == other.fn_depth
            {
                value.external |= other.external;
                value.field |= other.field;
                true
            } else {
                false
            }
        });
    }
    out
}
impl Ctx<'_> {
    pub(crate) fn fact_key(&self, name: &str) -> FactKey {
        FactKey::Binding(self.lookup(name).expect("checked binding").1.id)
    }
    pub(crate) fn field_fact_key(&self, name: &str) -> FactKey {
        FactKey::Field(self.lookup(name).expect("checked binding").1.id)
    }
    fn binding_by_id(&self, id: BindingId) -> Option<&crate::Binding> {
        self.scopes
            .iter()
            .rev()
            .flat_map(|s| s.names.values())
            .find(|b| b.id == id)
    }
    pub(crate) fn flow_state(&self) -> FlowState {
        let mut state = FlowState::new();
        for scope in &self.scopes {
            state.extend(scope.narrowed.clone());
        }
        state
    }
    pub(crate) fn restore_flow(&mut self, mut state: FlowState) {
        state.retain(|key, _| match key {
            FactKey::Binding(id) | FactKey::Field(id) => self.binding_by_id(*id).is_some(),
            FactKey::Capability(_) => true,
        });
        for scope in &mut self.scopes {
            scope.narrowed.clear();
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.narrowed = state;
        }
    }
    pub(crate) fn close_scope(&mut self) {
        let state = self.flow_state();
        self.scopes.pop();
        self.restore_flow(state);
    }
    pub(crate) fn kill_binding(&mut self, id: BindingId) {
        for scope in &mut self.scopes {
            scope.narrowed.remove(&FactKey::Binding(id));
        }
    }
    pub(crate) fn invalidate_external_facts(&mut self) {
        for scope in &mut self.scopes {
            scope.narrowed.retain(|_, n| !n.external);
        }
    }
    pub(crate) fn narrow_here(&mut self, hints: Narrowings) {
        for (key, ty) in hints {
            let (key, external, field) = match key {
                FactKey::Field(id) => (FactKey::Binding(id), true, true),
                FactKey::Binding(id) => (
                    FactKey::Binding(id),
                    self.binding_by_id(id).is_some_and(|b| b.external),
                    false,
                ),
                key => (key, false, false),
            };
            if let Some(scope) = self.scopes.last_mut() {
                scope.narrowed.insert(
                    key,
                    Narrow {
                        ty,
                        external,
                        field,
                        fn_depth: self.fn_depth,
                    },
                );
            }
        }
    }
    fn with_hints(&mut self, state: FlowState, hints: Narrowings) -> FlowState {
        self.restore_flow(state);
        self.narrow_here(hints);
        self.flow_state()
    }
    pub(crate) fn flow_condition(&mut self, e: &Expr) -> (FlowState, FlowState) {
        let result = match &e.kind {
            ExprKind::Binary { op, left, right } if op == "&&" || op == "||" => {
                let (yes, no) = self.flow_condition(left);
                self.restore_flow(if op == "&&" { yes.clone() } else { no.clone() });
                let (r_yes, r_no) = self.flow_condition(right);
                let l = self.types.get(left.id).cloned().unwrap_or(Type::Error);
                let r = self.types.get(right.id).cloned().unwrap_or(Type::Error);
                self.types.set(
                    e.id,
                    if l == Type::Boolean && r == Type::Boolean {
                        Type::Boolean
                    } else {
                        Type::Cond
                    },
                );
                if op == "&&" {
                    (r_yes, intersect(vec![no, r_no]))
                } else {
                    (intersect(vec![yes, r_yes]), r_no)
                }
            }
            ExprKind::Unary { op, expr } if op == "!" => {
                let (yes, no) = self.flow_condition(expr);
                self.types.set(e.id, Type::Boolean);
                (no, yes)
            }
            _ => {
                let t = self.expr(e);
                self.check_condition_type(e, &t);
                let after = self.flow_state();
                let (yes, no) = self.narrowings(e);
                (
                    self.with_hints(after.clone(), yes),
                    self.with_hints(after, no),
                )
            }
        };
        self.restore_flow(intersect(vec![result.0.clone(), result.1.clone()]));
        result
    }
    /// Conservative loop/exception entry: no fact survives a possible write,
    /// and calls (or field writes) discard all externally dependent facts.
    pub(crate) fn prepare_region(&mut self, body: &[Stmt], extra: &[&Expr]) {
        let mut writes = HashSet::new();
        let mut calls = false;
        fn expr(e: &Expr, w: &mut HashSet<selu_syntax::ExprId>, calls: &mut bool) {
            if matches!(e.kind, ExprKind::Call { .. } | ExprKind::New { .. }) {
                *calls = true;
            }
            if matches!(
                e.kind,
                ExprKind::Assign { .. } | ExprKind::CompoundAssign { .. } | ExprKind::Update { .. }
            ) {
                w.insert(e.id);
            }
            visit::child_exprs_and_stmts(e, &mut |c| child(c, w, calls));
        }
        fn stmt(s: &Stmt, w: &mut HashSet<selu_syntax::ExprId>, calls: &mut bool) {
            visit::child_stmts_and_exprs(s, &mut |c| child(c, w, calls));
        }
        fn child(c: Child<'_>, w: &mut HashSet<selu_syntax::ExprId>, calls: &mut bool) {
            match c {
                Child::Expr(e) => expr(e, w, calls),
                Child::Stmt(s) => stmt(s, w, calls),
            }
        }
        for s in body {
            stmt(s, &mut writes, &mut calls);
        }
        for e in extra {
            expr(e, &mut writes, &mut calls);
        }
        let mut origins = HashSet::new();
        let mut field_writes = false;
        for e in writes {
            if let Some(origin) = self.flow_info.assignments.get(&e) {
                origins.insert(*origin);
            } else {
                field_writes = true;
            }
        }
        let ids: HashSet<_> = self
            .scopes
            .iter()
            .flat_map(|s| s.names.values())
            .filter(|b| origins.contains(&b.origin))
            .map(|b| b.id)
            .collect();
        for scope in &mut self.scopes {
            scope.narrowed.retain(|key, n| {
                !matches!(key,FactKey::Binding(id) if ids.contains(id))
                    && !(calls && n.external)
                    && !(field_writes && n.field)
            });
        }
    }
}
