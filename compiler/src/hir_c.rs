//! C generation straight from the HIR, for the functions it can already handle: this is the
//! first set of node families moved off the AST in the plan of
//! `docs/design/20-hir-y-ir.md`. Unsupported families still make `generate` return `None`,
//! so the AST generator remains the safe fallback.
//!
//! The emitted C mirrors what the AST generator writes for the same constructs (same
//! operators, `ostrin_idiv` for `Int / Int`, C blocks for scopes), so both paths behave
//! identically; the differential tests compare them through the interpreter.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

use crate::ast::{BinOp, Expr, IntKind, RangeKind, UnaryOp};
use crate::hir::{HirArg, HirBlock, HirExpr, HirFunction, HirKind, HirStmt};
use crate::types::Ty;

type Bail<T> = Result<T, ()>;

/// What the emitter needs to know about the program, taken from the native backend's registry
/// (C type names are compared as strings, so this module never sees the backend's own types).
pub struct World {
    /// User functions: name -> (C parameter types, C return type).
    pub functions: HashMap<String, (Vec<String>, String)>,
    /// Optional direct C names for synthesized functions. Ordinary source
    /// functions use `c_name`; monomorphized generic instances already have
    /// their final C name (`identity__Int`) and must not receive the normal
    /// `ostrin_fn_` prefix a second time.
    pub function_c_names: HashMap<String, String>,
    /// Non-generic records: name -> [(field, C type)] in declaration order. Empty when reads of
    /// records must be tracked (E1101), which only the AST path knows how to do.
    pub records: HashMap<String, Vec<(String, String)>>,
    /// Applied generic records/enums are keyed by their HIR spelling
    /// (`Box<Int>`) and point at the concrete C instance (`Box__Int`).
    pub applied_records: HashMap<String, String>,
    pub applied_enums: HashMap<String, String>,
    /// Methods of records: (record, method) -> (C name, C parameter types with `self` first, C return type).
    pub methods: HashMap<(String, String), (String, Vec<String>, String)>,
    pub c_name: fn(&str) -> String,
    /// Non-generic enums (tagged unions passed by value) and their variants by name.
    pub enums: HashSet<String>,
    pub variants: HashMap<String, VariantView>,
    /// Generic variants need the concrete enum name in their key because
    /// `Just<Int>` and `Just<String>` share the source-level variant name.
    pub applied_variants: HashMap<(String, String), VariantView>,
    /// C declarations produced while lowering HIR lambdas/function values.
    /// They are drained by `codegen.rs` after all HIR bodies have been visited.
    pub closure_protos: RefCell<Vec<String>>,
    pub closure_bodies: RefCell<Vec<(String, String)>>,
    pub closure_counter: Cell<usize>,
}

/// One enum variant: its enum, tag and fields with C types (in declaration order).
#[derive(Clone)]
pub struct VariantView {
    pub enum_name: String,
    pub tag: usize,
    pub fields: Vec<(String, String)>,
}

fn is_scalar(ty: &Ty) -> bool {
    matches!(ty, Ty::Int | Ty::Float | Ty::Float32 | Ty::Sized(_) | Ty::Bool | Ty::String)
}

/// Scalars compatible in C without a conversion helper (`Int` where a `Float` goes, …).
fn c_compatible(arg: &str, param: &str) -> bool {
    arg == param
        || (matches!(arg, "int64_t" | "double" | "bool")
            && matches!(param, "int64_t" | "double" | "bool"))
}

struct Emitter<'a> {
    world: &'a World,
    scopes: Vec<HashSet<String>>,
    ret: Ty,
    temp: usize,
    /// Direct locals in the current callable that own one native reference.
    /// Nested HIR blocks use `owned_block_locals` and are cleaned at block exit.
    owned_locals: Vec<(String, Ty)>,
    owned_block_locals: Vec<Vec<(String, Ty)>>,
    ownership_control_frames: Vec<usize>,
}

impl Emitter<'_> {
    /// The C type of a value the emitter handles: scalars, records, the
    /// monomorphized built-in wrappers and the collection structs emitted by
    /// codegen.
    fn c_type(&self, ty: &Ty) -> Bail<String> {
        match ty {
            Ty::Int => Ok("int64_t".to_string()),
            Ty::Float => Ok("double".to_string()),
            Ty::Float32 => Ok("float".to_string()),
            Ty::Sized(kind) => Ok(kind.c_type().to_string()),
            Ty::Bool => Ok("bool".to_string()),
            Ty::String => Ok("const char*".to_string()),
            Ty::Named(n) if self.world.records.contains_key(n) => Ok(format!("{n}*")),
            Ty::Named(n) if self.world.enums.contains(n) => Ok(n.clone()),
            Ty::Applied(..) if self.world.applied_records.contains_key(&ty.describe()) => {
                Ok(format!("{}*", self.world.applied_records[&ty.describe()]))
            }
            Ty::Applied(..) if self.world.applied_enums.contains_key(&ty.describe()) => {
                Ok(self.world.applied_enums[&ty.describe()].clone())
            }
            Ty::List(elem) => Ok(format!("List_{}*", self.mangle_type(elem)?)),
            Ty::Map(key, value) => Ok(format!(
                "Map_{}_{}*",
                self.mangle_type(key)?,
                self.mangle_type(value)?
            )),
            Ty::Set(elem) => Ok(format!("Set_{}*", self.mangle_type(elem)?)),
            Ty::Fn(..) => Ok("OstrinClosure".to_string()),
            Ty::Applied(name, args) if name == "Option" && args.len() == 1 => {
                Ok(format!("Option_{}", self.mangle_type(&args[0])?))
            }
            Ty::Applied(name, args) if name == "Result" && args.len() == 2 => Ok(format!(
                "Result_{}_{}",
                self.mangle_type(&args[0])?,
                self.mangle_type(&args[1])?
            )),
            _ => Err(()),
        }
    }

    fn managed_c_type(ty: &str) -> bool {
        ty == "const char*" || ty.ends_with('*')
    }

    fn mangle_type(&self, ty: &Ty) -> Bail<String> {
        Ok(match ty {
            Ty::Int => "Int".to_string(),
            Ty::Float => "Float".to_string(),
            Ty::Float32 => "Float32".to_string(),
            Ty::Sized(kind) => kind.name().to_string(),
            Ty::Bool => "Bool".to_string(),
            Ty::String => "String".to_string(),
            Ty::Void => "Void".to_string(),
            Ty::Named(n) => n.clone(),
            Ty::List(elem) => format!("List_{}", self.mangle_type(elem)?),
            Ty::Map(key, value) => format!(
                "Map_{}_{}",
                self.mangle_type(key)?,
                self.mangle_type(value)?
            ),
            Ty::Set(elem) => format!("Set_{}", self.mangle_type(elem)?),
            Ty::Fn(params, ret) => format!(
                "Fn_{}_to_{}",
                params
                    .iter()
                    .map(|param| self.mangle_type(param))
                    .collect::<Bail<Vec<_>>>()?
                    .join("_"),
                self.mangle_type(ret)?
            ),
            Ty::Applied(name, args) if name == "Option" && args.len() == 1 => {
                format!("Option_{}", self.mangle_type(&args[0])?)
            }
            Ty::Applied(name, args) if name == "Result" && args.len() == 2 => {
                format!(
                    "Result_{}_{}",
                    self.mangle_type(&args[0])?,
                    self.mangle_type(&args[1])?
                )
            }
            Ty::Applied(..) if self.world.applied_records.contains_key(&ty.describe()) => {
                self.world.applied_records[&ty.describe()].clone()
            }
            Ty::Applied(..) if self.world.applied_enums.contains_key(&ty.describe()) => {
                self.world.applied_enums[&ty.describe()].clone()
            }
            _ => return Err(()),
        })
    }

    fn field(&self, record: &Ty, field: &str) -> Bail<String> {
        let name = self.record_name(record)?;
        self.world
            .records
            .get(&name)
            .and_then(|fs| fs.iter().find(|(f, _)| f == field))
            .map(|(_, t)| t.clone())
            .ok_or(())
    }

    fn record_name(&self, ty: &Ty) -> Bail<String> {
        match ty {
            Ty::Named(name) if self.world.records.contains_key(name) => Ok(name.clone()),
            Ty::Applied(..) => self.world.applied_records.get(&ty.describe()).cloned().ok_or(()),
            _ => Err(()),
        }
    }

    fn enum_name(&self, ty: &Ty) -> Bail<String> {
        match ty {
            Ty::Named(name) if self.world.enums.contains(name) => Ok(name.clone()),
            Ty::Applied(..) => self.world.applied_enums.get(&ty.describe()).cloned().ok_or(()),
            _ => Err(()),
        }
    }

    fn variant(&self, name: &str, ty: &Ty) -> Option<VariantView> {
        let enum_name = self.enum_name(ty).ok()?;
        self.world
            .applied_variants
            .get(&(enum_name.clone(), name.to_string()))
            .cloned()
            .or_else(|| self.world.variants.get(name).filter(|variant| variant.enum_name == enum_name).cloned())
    }

    fn variant_constructor(&mut self, e: &HirExpr, name: &str, args: &[HirArg]) -> Bail<String> {
        let v = self.variant(name, &e.ty).ok_or(())?;
        if v.fields.len() != args.len() || args.iter().any(|arg| arg.name.is_some()) || self.c_type(&e.ty)? != v.enum_name {
            return Err(());
        }
        let mut inits = Vec::new();
        for (arg, (field, want)) in args.iter().zip(&v.fields) {
            if !c_compatible(&self.c_type(&arg.value.ty)?, want) {
                return Err(());
            }
            inits.push(format!(".{field} = {}", self.expr(&arg.value)?));
        }
        Ok(format!(
            "(({}){{ .tag = {}, .data.{name} = {{ {} }} }})",
            v.enum_name,
            v.tag,
            inits.join(", ")
        ))
    }
}

/// The body (without braces) of an eligible function, or `None`.
pub fn generate(f: &HirFunction, world: &World) -> Option<String> {
    let mut e = Emitter {
        world,
        scopes: vec![f.params.iter().map(|(n, _)| n.clone()).collect()],
        ret: f.ret.clone(),
        temp: 0,
        owned_locals: Vec::new(),
        owned_block_locals: Vec::new(),
        ownership_control_frames: Vec::new(),
    };
    if !f.generics.is_empty() || f.params.iter().any(|(_, t)| e.c_type(t).is_err()) {
        return None;
    }
    if f.ret != Ty::Void && e.c_type(&f.ret).is_err() {
        return None;
    }
    let mut out = String::new();
    e.body(&f.body, &mut out).ok()?;
    Some(out)
}

impl Emitter<'_> {
    fn next_temp(&mut self) -> String {
        self.temp += 1;
        format!("__hir_t{}", self.temp)
    }

    fn managed(&self, ty: &Ty) -> bool {
        match ty {
            Ty::String | Ty::List(_) | Ty::Map(_, _) | Ty::Set(_) => true,
            Ty::Named(name) => self.world.records.contains_key(name),
            Ty::Applied(..) => self.world.applied_records.contains_key(&ty.describe()),
            _ => false,
        }
    }

    fn borrowed_expr(expr: &HirExpr) -> bool {
        matches!(expr.kind, HirKind::Local(_) | HirKind::Field(..) | HirKind::Index(..))
    }

    fn owned_local(&self, name: &str) -> bool {
        self.owned_block_locals
            .iter()
            .rev()
            .any(|frame| frame.iter().any(|(owned, _)| owned == name))
            || self.owned_locals.iter().any(|(owned, _)| owned == name)
    }

    fn owned_local_expr(&self, expr: &HirExpr) -> Option<String> {
        let HirKind::Local(name) = &expr.kind else { return None };
        self.owned_local(name).then(|| name.clone())
    }

    fn track_owned_local(&mut self, name: &str, ty: &Ty, borrowed: bool, out: &mut String) {
        if !self.managed(ty) {
            return;
        }
        if borrowed {
            out.push_str(&format!("    ostrin_retain((void*){name});\n"));
        }
        if self.scopes.len() == 1 {
            if !self.owned_local(name) {
                self.owned_locals.push((name.to_string(), ty.clone()));
            }
        } else if let Some(frame) = self.owned_block_locals.last_mut() {
            if !frame.iter().any(|(owned, _)| owned == name) {
                frame.push((name.to_string(), ty.clone()));
            }
        } else if !self.owned_local(name) {
            self.owned_locals.push((name.to_string(), ty.clone()));
        }
    }

    fn cleanup(&self, out: &mut String, transfer: Option<&str>) {
        for frame in self.owned_block_locals.iter().rev() {
            for (name, ty) in frame.iter().rev() {
                if transfer == Some(name.as_str()) || !self.managed(ty) {
                    continue;
                }
                out.push_str(&format!("    ostrin_release((void*){name});\n"));
            }
        }
        for (name, ty) in self.owned_locals.iter().rev() {
            if transfer == Some(name.as_str()) || !self.managed(ty) {
                continue;
            }
            out.push_str(&format!("    ostrin_release((void*){name});\n"));
        }
    }

    fn emit_control_cleanup(&self, out: &mut String) {
        let Some(&start) = self.ownership_control_frames.last() else { return };
        for frame in self.owned_block_locals[start..].iter().rev() {
            for (name, ty) in frame.iter().rev() {
                if self.managed(ty) {
                    out.push_str(&format!("    ostrin_release((void*){name});\n"));
                }
            }
        }
    }

    fn begin_loop(&mut self) -> usize {
        let frame = self.owned_block_locals.len();
        self.owned_block_locals.push(Vec::new());
        self.ownership_control_frames.push(frame);
        frame
    }

    fn end_loop(&mut self, frame: usize, out: &mut String) {
        for (name, ty) in self.owned_block_locals[frame].iter().rev() {
            if self.managed(ty) {
                out.push_str(&format!("    ostrin_release((void*){name});\n"));
            }
        }
        self.owned_block_locals.pop().expect("loop ownership frame must exist");
        self.ownership_control_frames.pop().expect("loop control frame must exist");
    }

    fn emit_return(&mut self, expr: Option<&HirExpr>, code: String, ty: &Ty, out: &mut String) -> Bail<()> {
        if self.managed(ty) {
            let cty = self.c_type(ty)?;
            let temp = self.next_temp();
            out.push_str(&format!("    {cty} {temp} = {code};\n"));
            let transfer = expr.and_then(|value| self.owned_local_expr(value));
            if transfer.is_none() && expr.is_some_and(Self::borrowed_expr) {
                out.push_str(&format!("    ostrin_retain((void*){temp});\n"));
            }
            self.cleanup(out, transfer.as_deref());
            out.push_str(&format!("    return {temp};\n"));
        } else if *ty != Ty::Void
            && (!self.owned_locals.is_empty() || self.owned_block_locals.iter().any(|frame| !frame.is_empty()))
        {
            // Evaluate wrapper returns before releasing managed locals. A
            // by-value Result/Option may still contain a managed pointer.
            let cty = self.c_type(ty)?;
            let temp = self.next_temp();
            out.push_str(&format!("    {cty} {temp} = {code};\n"));
            self.cleanup(out, None);
            out.push_str(&format!("    return {temp};\n"));
        } else {
            self.cleanup(out, None);
            out.push_str(&format!("    return {code};\n"));
        }
        Ok(())
    }

    fn declared(&self, name: &str) -> bool {
        self.scopes.iter().any(|s| s.contains(name))
    }

    fn body(&mut self, block: &HirBlock, out: &mut String) -> Bail<()> {
        for stmt in &block.stmts {
            self.stmt(stmt, out)?;
        }
        match &block.tail {
            Some(tail) if self.ret == Ty::Void => {
                self.expr_stmt(tail, out)?;
                self.cleanup(out, None);
                out.push_str("    return;\n");
            }
            Some(tail) => {
                let code = self.expr(tail)?;
                let ret = self.ret.clone();
                self.emit_return(Some(tail), code, &ret, out)?;
            }
            None => {
                self.cleanup(out, None);
                out.push_str("    return;\n");
            }
        }
        Ok(())
    }

    fn scoped_stmts(&mut self, block: &HirBlock, out: &mut String) -> Bail<()> {
        let frame = self.owned_block_locals.len();
        self.owned_block_locals.push(Vec::new());
        self.scopes.push(HashSet::new());
        for stmt in &block.stmts {
            self.stmt(stmt, out)?;
        }
        if let Some(tail) = &block.tail {
            self.expr_stmt(tail, out)?;
        }
        self.scopes.pop();
        for (name, ty) in self.owned_block_locals[frame].iter().rev() {
            if self.managed(ty) {
                out.push_str(&format!("    ostrin_release((void*){name});\n"));
            }
        }
        self.owned_block_locals.pop().expect("statement ownership frame must exist");
        Ok(())
    }

    fn stmt(&mut self, stmt: &HirStmt, out: &mut String) -> Bail<()> {
        match stmt {
            HirStmt::Let {
                name,
                declared,
                value,
                ..
            } => {
                let ty = self.c_type(&value.ty)?;
                if let Some(declared) = declared {
                    let declared = crate::typeck::resolve_type(declared);
                    if declared != value.ty || self.c_type(&declared)? != ty {
                        return Err(());
                    }
                }
                let code = self.expr(value)?;
                out.push_str(&format!("    {ty} {name} = {code};\n"));
                self.track_owned_local(name, &value.ty, Self::borrowed_expr(value), out);
                self.scopes.last_mut().expect("scope").insert(name.clone());
            }
            HirStmt::Assign { name, value } => {
                let code = self.expr(value)?;
                if self.declared(name) {
                    if self.owned_local(name) && self.managed(&value.ty) {
                        let temp = self.next_temp();
                        let ty = self.c_type(&value.ty)?;
                        out.push_str(&format!("    {ty} {temp} = {code};\n"));
                        if Self::borrowed_expr(value) {
                            out.push_str(&format!("    ostrin_retain((void*){temp});\n"));
                        }
                        out.push_str(&format!("    ostrin_release((void*){name}); {name} = {temp};\n"));
                    } else {
                        out.push_str(&format!("    {name} = {code};\n"));
                    }
                } else {
                    let ty = self.c_type(&value.ty)?;
                    out.push_str(&format!("    {ty} {name} = {code};\n"));
                    self.track_owned_local(name, &value.ty, Self::borrowed_expr(value), out);
                    self.scopes.last_mut().expect("scope").insert(name.clone());
                }
            }
            HirStmt::FieldAssign { target, value } => {
                let HirKind::Field(obj, field) = &target.kind else {
                    return Err(());
                };
                let field_ty = self.field(&obj.ty, field)?;
                if !c_compatible(&self.c_type(&value.ty)?, &field_ty)
                    && self.c_type(&value.ty)? != field_ty
                {
                    return Err(());
                }
                let (o, v) = (self.expr(obj)?, self.expr(value)?);
                out.push_str(&format!("    {o}->{field} = {v};\n"));
            }
            HirStmt::Return(Some(value)) => {
                let code = self.expr(value)?;
                let ret = self.ret.clone();
                self.emit_return(Some(value), code, &ret, out)?;
            }
            HirStmt::Return(None) => {
                self.cleanup(out, None);
                out.push_str("    return;\n");
            }
            HirStmt::Break(None) => {
                self.emit_control_cleanup(out);
                out.push_str("    break;\n");
            }
            HirStmt::Continue => {
                self.emit_control_cleanup(out);
                out.push_str("    continue;\n");
            }
            HirStmt::While { cond, body } => {
                let c = self.expr(cond)?;
                out.push_str(&format!("    while ({c}) {{\n"));
                let frame = self.begin_loop();
                self.scoped_stmts(body, out)?;
                self.end_loop(frame, out);
                out.push_str("    }\n");
            }
            HirStmt::For { var, iter, body } => {
                if let HirKind::Range(start, kind, end, None) = &iter.kind {
                    if start.ty != Ty::Int || end.ty != Ty::Int {
                        return Err(());
                    }
                    let (s, e) = (self.expr(start)?, self.expr(end)?);
                    let cmp = if *kind == RangeKind::To { "<=" } else { "<" };
                    out.push_str(&format!(
                        "    for (int64_t {var} = {s}; {var} {cmp} {e}; {var}++) {{\n"
                    ));
                    let frame = self.begin_loop();
                    self.scopes.push(HashSet::from([var.clone()]));
                    self.scoped_stmts(body, out)?;
                    self.scopes.pop();
                    self.end_loop(frame, out);
                    out.push_str("    }\n");
                    return Ok(());
                }

                let Ty::List(elem) = &iter.ty else {
                    return Err(());
                };
                let list_c = self.c_type(&iter.ty)?;
                let elem_c = self.c_type(elem)?;
                let list_temp = self.next_temp();
                let index_temp = self.next_temp();
                let iter_code = self.expr(iter)?;
                out.push_str(&format!(
                    "    {{ {list_c} {list_temp} = {iter_code}; for (int64_t {index_temp} = 0; {index_temp} < {list_temp}->length; {index_temp}++) {{ {elem_c} {var} = {list_temp}->items[{index_temp}];\n"
                ));
                let frame = self.begin_loop();
                self.scopes.push(HashSet::from([var.clone()]));
                self.track_owned_local(var, elem, true, out);
                self.scoped_stmts(body, out)?;
                self.scopes.pop();
                self.end_loop(frame, out);
                out.push_str("    }\n");
                // A temporary list (literal or call result) is owned by the loop.
                if !Self::borrowed_expr(iter) {
                    out.push_str(&format!("    ostrin_release((void*){list_temp});\n"));
                }
                out.push_str("    }\n");
            }
            HirStmt::Expr(e) => self.expr_stmt(e, out)?,
            _ => return Err(()),
        }
        Ok(())
    }

    /// An expression whose value is discarded (statement position).
    fn expr_stmt(&mut self, e: &HirExpr, out: &mut String) -> Bail<()> {
        match &e.kind {
            HirKind::If(cond, then_block, else_block) => {
                let c = self.expr(cond)?;
                out.push_str(&format!("    if ({c}) {{\n"));
                self.scoped_stmts(then_block, out)?;
                out.push_str("    }");
                if let Some(else_block) = else_block {
                    out.push_str(" else {\n");
                    self.scoped_stmts(else_block, out)?;
                    out.push_str("    }");
                }
                out.push('\n');
            }
            HirKind::Block(block) => {
                out.push_str("    {\n");
                self.scoped_stmts(block, out)?;
                out.push_str("    }\n");
            }
            _ => {
                let code = self.expr(e)?;
                out.push_str(&format!("    {code};\n"));
            }
        }
        Ok(())
    }

    /// `match`, compiled like the AST path: a scrutinee variable, a `matched` flag, a result
    /// variable and one `if (!matched && <pattern>)` per arm, in source order.
    fn matching(
        &mut self,
        e: &HirExpr,
        scrutinee: &HirExpr,
        arms: &[crate::hir::HirArm],
    ) -> Bail<String> {
        if arms.is_empty() {
            return Err(());
        }
        let scrutinee_c = self.c_type(&scrutinee.ty)?;
        let void = e.ty == Ty::Void;
        let result_c = if void {
            String::new()
        } else {
            self.c_type(&e.ty)?
        };
        let scrutinee_code = self.expr(scrutinee)?;
        self.temp += 1;
        let (svar, mvar, rvar) = (
            format!("__hir_s{}", self.temp),
            format!("__hir_m{}", self.temp),
            format!("__hir_r{}", self.temp),
        );
        let mut blocks = String::new();
        for arm in arms {
            let mut condition = format!("!{mvar}");
            let mut bindings = String::new();
            let mut bound = HashSet::new();
            self.pattern(
                &arm.pattern,
                &svar,
                &scrutinee.ty,
                &mut condition,
                &mut bindings,
                &mut bound,
            )?;
            self.scopes.push(bound);
            let guard = match &arm.guard {
                Some(g) => Some(self.expr(g)?),
                None => None,
            };
            let body = self.block_value(&arm.body);
            self.scopes.pop();
            let body = body?;
            let commit = if void {
                format!("{body}; {mvar} = 1;")
            } else {
                format!("{rvar} = {body}; {mvar} = 1;")
            };
            let commit = match guard {
                Some(g) => format!("if ({g}) {{ {commit} }}"),
                None => commit,
            };
            blocks.push_str(&format!("if ({condition}) {{ {bindings} {commit} }} "));
        }
        let decl = if void {
            String::new()
        } else {
            format!("{result_c} {rvar};")
        };
        let yielded = if void { "(void)0".to_string() } else { rvar };
        Ok(format!(
            "({{ {scrutinee_c} {svar} = {scrutinee_code}; int {mvar} = 0; {decl} {blocks} if (!{mvar}) {{ fprintf(stderr, \"ostrin: non-exhaustive match at runtime\\n\"); abort(); }} {yielded}; }})"
        ))
    }

    /// One pattern: its condition (appended to `condition`) and the bindings it introduces.
    fn pattern(
        &mut self,
        pattern: &crate::ast::Pattern,
        var: &str,
        ty: &Ty,
        condition: &mut String,
        bindings: &mut String,
        bound: &mut HashSet<String>,
    ) -> Bail<()> {
        use crate::ast::Pattern;
        let c = self.c_type(ty)?;
        match pattern {
            Pattern::Wildcard => Ok(()),
            Pattern::Ident(name) if name == "None" && is_option(ty) => {
                condition.push_str(&format!(" && !{var}.has"));
                Ok(())
            }
            Pattern::Variant(name, fields) if is_option(ty) && name == "Some" => {
                let inner = option_inner(ty)?;
                if fields.len() != 1 {
                    return Err(());
                }
                condition.push_str(&format!(" && {var}.has"));
                self.pattern(
                    &fields[0].1,
                    &format!("{var}.value"),
                    &inner,
                    condition,
                    bindings,
                    bound,
                )
            }
            Pattern::Variant(name, fields)
                if is_result(ty) && matches!(name.as_str(), "Ok" | "Err") =>
            {
                let (ok, err) = result_types(ty)?;
                let (field_ty, flag, field) = if name == "Ok" {
                    (ok, "ok", "value")
                } else {
                    (err, "!ok", "error")
                };
                if fields.len() != 1 {
                    return Err(());
                }
                if flag == "ok" {
                    condition.push_str(&format!(" && {var}.ok"));
                } else {
                    condition.push_str(&format!(" && !{var}.ok"));
                }
                self.pattern(
                    &fields[0].1,
                    &format!("{var}.{field}"),
                    &field_ty,
                    condition,
                    bindings,
                    bound,
                )
            }
            Pattern::Ident(name) => {
                let unit = self.variant(name, ty).filter(|v| v.fields.is_empty() && self.c_type(ty).ok().as_deref() == Some(v.enum_name.as_str()));
                match unit {
                    Some(v) => {
                        condition.push_str(&format!(" && ({var}.tag == {})", v.tag));
                    }
                    None => {
                        bindings.push_str(&format!("{c} {name} = {var}; "));
                        bound.insert(name.clone());
                    }
                }
                Ok(())
            }
            Pattern::Variant(name, fields) => {
                let v = self.variant(name, ty).filter(|v| self.c_type(ty).ok().as_deref() == Some(v.enum_name.as_str())).ok_or(())?;
                condition.push_str(&format!(" && ({var}.tag == {})", v.tag));
                for (position, (field_name, sub)) in fields.iter().enumerate() {
                    let (actual, field_c) = v
                        .fields
                        .iter()
                        .find(|(n, _)| n == field_name)
                        .or_else(|| v.fields.get(position))
                        .ok_or(())?
                        .clone();
                    let field_ty = self.ty_of_c(&field_c)?;
                    self.pattern(
                        sub,
                        &format!("{var}.data.{name}.{actual}"),
                        &field_ty,
                        condition,
                        bindings,
                        bound,
                    )?;
                }
                Ok(())
            }
            Pattern::Literal(lit) => {
                let code = match lit.unlocated() {
                    Expr::IntLiteral(v) if *ty == Ty::Int => format!("INT64_C({v})"),
                    Expr::BoolLiteral(v) if *ty == Ty::Bool => v.to_string(),
                    Expr::StringLiteral(s) if *ty == Ty::String => {
                        crate::codegen::c_string_literal(s)
                    }
                    _ => return Err(()),
                };
                condition.push_str(&if *ty == Ty::String {
                    format!(" && (strcmp({var}, {code}) == 0)")
                } else {
                    format!(" && ({var} == {code})")
                });
                Ok(())
            }
            Pattern::Range(lo, kind, hi) => {
                let (Expr::IntLiteral(lo), Expr::IntLiteral(hi)) = (lo.unlocated(), hi.unlocated())
                else {
                    return Err(());
                };
                if *ty != Ty::Int {
                    return Err(());
                }
                let upper = if *kind == RangeKind::To { "<=" } else { "<" };
                condition.push_str(&format!(
                    " && ({var} >= INT64_C({lo}) && {var} {upper} INT64_C({hi}))"
                ));
                Ok(())
            }
        }
    }

    /// The `Ty` a variant field's C type stands for (only the types this emitter handles).
    fn ty_of_c(&self, c: &str) -> Bail<Ty> {
        Ok(match c {
            "int64_t" => Ty::Int,
            "double" => Ty::Float,
            "float" => Ty::Float32,
            "int8_t" => Ty::Sized(IntKind::I8),
            "int16_t" => Ty::Sized(IntKind::I16),
            "int32_t" => Ty::Sized(IntKind::I32),
            "uint8_t" => Ty::Sized(IntKind::U8),
            "uint16_t" => Ty::Sized(IntKind::U16),
            "uint32_t" => Ty::Sized(IntKind::U32),
            "uint64_t" => Ty::Sized(IntKind::U64),
            "bool" => Ty::Bool,
            "const char*" => Ty::String,
            other => {
                if let Some(name) = other.strip_suffix('*') {
                    if self.world.records.contains_key(name) {
                        return Ok(Ty::Named(name.to_string()));
                    }
                }
                if self.world.enums.contains(other) {
                    return Ok(Ty::Named(other.to_string()));
                }
                return Err(());
            }
        })
    }

    fn block_value(&mut self, block: &HirBlock) -> Bail<String> {
        let mut body = String::new();
        let frame = self.owned_block_locals.len();
        self.owned_block_locals.push(Vec::new());
        self.scopes.push(HashSet::new());
        for stmt in &block.stmts {
            self.stmt(stmt, &mut body)?;
        }
        let tail_expr = block.tail.as_ref();
        let tail = match tail_expr {
            Some(t) => self.expr(t)?,
            None => "(void)0".to_string(),
        };
        let transfer = tail_expr.and_then(|value| self.owned_local_expr(value));
        let tail_ty = tail_expr.map(|value| value.ty.clone()).unwrap_or(Ty::Void);
        self.scopes.pop();
        let result = if self.managed(&tail_ty) {
            let cty = self.c_type(&tail_ty)?;
            let temp = self.next_temp();
            body.push_str(&format!("    {cty} {temp} = {tail};\n"));
            if transfer.is_none() && tail_expr.is_some_and(|value| Self::borrowed_expr(value)) {
                body.push_str(&format!("    ostrin_retain((void*){temp});\n"));
            }
            for (name, ty) in self.owned_block_locals[frame].iter().rev() {
                if transfer == Some(name.clone()) || !self.managed(ty) {
                    continue;
                }
                body.push_str(&format!("    ostrin_release((void*){name});\n"));
            }
            format!("({{ {body} {temp}; }})")
        } else {
            for (name, ty) in self.owned_block_locals[frame].iter().rev() {
                if !self.managed(ty) {
                    continue;
                }
                body.push_str(&format!("    ostrin_release((void*){name});\n"));
            }
            format!("({{ {body} {tail}; }})")
        };
        self.owned_block_locals.pop().expect("block ownership frame must exist");
        Ok(result)
    }

    fn sized_binary(&mut self, op: BinOp, left: &HirExpr, right: &HirExpr) -> Bail<String> {
        let (Ty::Sized(kind), Ty::Sized(other)) = (&left.ty, &right.ty) else {
            return Err(());
        };
        if kind != other {
            return Err(());
        }
        let (lc, rc) = (self.expr(left)?, self.expr(right)?);
        let c = kind.c_type();
        let a = self.next_temp();
        let b = self.next_temp();
        let result = self.next_temp();
        let decl = format!("{c} {a} = {lc}; {c} {b} = {rc};");
        let overflow = "fprintf(stderr, \"runtime error: integer overflow\\n\"); exit(1);";
        Ok(match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul => {
                let builtin = match op {
                    BinOp::Add => "__builtin_add_overflow",
                    BinOp::Sub => "__builtin_sub_overflow",
                    BinOp::Mul => "__builtin_mul_overflow",
                    _ => unreachable!(),
                };
                format!("({{ {decl} {c} {result}; if ({builtin}({a}, {b}, &{result})) {{ {overflow} }} {result}; }})")
            }
            BinOp::Div => format!(
                "({{ {decl} if ({b} == 0) {{ fprintf(stderr, \"runtime error: division by zero\\n\"); exit(1); }} __int128 __q = (__int128){a} / (__int128){b}; if (__q < (__int128){} || __q > (__int128){}) {{ {overflow} }} ({c})__q; }})",
                kind.min(),
                kind.max()
            ),
            BinOp::Rem => format!(
                "({{ {decl} if ({b} == 0) {{ fprintf(stderr, \"runtime error: division by zero\\n\"); exit(1); }} ({c})((__int128){a} % (__int128){b}); }})"
            ),
            BinOp::Eq => format!("(({lc}) == ({rc}))"),
            BinOp::NotEq => format!("(({lc}) != ({rc}))"),
            BinOp::Lt => format!("(({lc}) < ({rc}))"),
            BinOp::Gt => format!("(({lc}) > ({rc}))"),
            BinOp::LtEq => format!("(({lc}) <= ({rc}))"),
            BinOp::GtEq => format!("(({lc}) >= ({rc}))"),
            BinOp::And | BinOp::Or => return Err(()),
        })
    }

    fn float32_binary(&mut self, op: BinOp, left: &HirExpr, right: &HirExpr) -> Bail<String> {
        if left.ty != Ty::Float32 || right.ty != Ty::Float32 {
            return Err(());
        }
        let (lc, rc) = (self.expr(left)?, self.expr(right)?);
        let c_op = match op {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Rem => return Ok(format!("fmodf({lc}, {rc})")),
            BinOp::Eq => "==",
            BinOp::NotEq => "!=",
            BinOp::Lt => "<",
            BinOp::Gt => ">",
            BinOp::LtEq => "<=",
            BinOp::GtEq => ">=",
            BinOp::And | BinOp::Or => return Err(()),
        };
        let result = matches!(op, BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div);
        Ok(if result { format!("((float)(({lc}) {c_op} ({rc})))") } else { format!("(({lc}) {c_op} ({rc}))") })
    }

    fn expr(&mut self, e: &HirExpr) -> Bail<String> {
        if e.ty != Ty::Void {
            self.c_type(&e.ty)?;
        }
        match &e.kind {
            HirKind::Int(v) => match &e.ty {
                Ty::Sized(kind) => Ok(format!("(({}){})", kind.c_type(), v)),
                _ => Ok(format!("INT64_C({v})")),
            },
            HirKind::Sized(v, kind) => Ok(format!("(({}){})", kind.c_type(), v)),
            HirKind::Float(v) => match e.ty {
                Ty::Float32 => Ok(format!("((float)({v:?}))")),
                _ => Ok(format!("{v:?}")),
            },
            HirKind::Float32(v) => Ok(format!("((float)({v:?}))")),
            HirKind::Bool(v) => Ok(if *v { "true" } else { "false" }.to_string()),
            HirKind::Str(s) => Ok(crate::codegen::c_string_literal(s)),
            HirKind::Local(name) => Ok(name.clone()),
            HirKind::Lambda(params, body) => self.lambda_expr(e, params, body),
            HirKind::Global(name) if self.variant(name, &e.ty).is_some_and(|v| v.fields.is_empty()) =>
            {
                let v = self.variant(name, &e.ty).ok_or(())?;
                if self.c_type(&e.ty)? != v.enum_name {
                    return Err(());
                }
                Ok(format!("(({}){{ .tag = {} }})", v.enum_name, v.tag))
            }
            HirKind::Global(name) if name == "None" && is_option(&e.ty) => {
                Ok(format!("(({}){{ .has = false }})", self.c_type(&e.ty)?))
            }
            HirKind::Global(name) if self.world.functions.contains_key(name) && is_fn(&e.ty) => {
                self.function_value(e, name)
            }
            HirKind::Match(scrutinee, arms) => self.matching(e, scrutinee, arms),
            HirKind::Field(obj, field) => {
                let field_ty = self.field(&obj.ty, field)?;
                if field_ty != self.c_type(&e.ty)? {
                    return Err(());
                }
                Ok(format!("{}->{field}", self.expr(obj)?))
            }
            HirKind::Record { fields, .. } => {
                let record = match &e.ty {
                    Ty::Named(name) if self.world.records.contains_key(name) => name.clone(),
                    Ty::Applied(..) => self.world.applied_records.get(&e.ty.describe()).cloned().ok_or(())?,
                    _ => return Err(()),
                };
                let declared = self.world.records.get(&record).ok_or(())?.clone();
                if fields.len() != declared.len() {
                    return Err(());
                }
                // Same shape as the AST path: allocate, then assign each field in source order.
                let temp = format!("__hir_rec{}", self.scopes.len());
                let mut body = format!(
                    "{record}* {temp} = ({record}*)ostrin_calloc_with_drop(1, sizeof({record}), (void (*)(void*))(ostrin_drop_{record})); if (!{temp}) {{ fprintf(stderr, \"ostrin: out of memory\\n\"); exit(1); }} "
                );
                for (field, value) in fields {
                    let want = declared
                        .iter()
                        .find(|(f, _)| f == field)
                        .map(|(_, t)| t.clone())
                        .ok_or(())?;
                    let have = self.c_type(&value.ty)?;
                    if !c_compatible(&have, &want) {
                        return Err(());
                    }
                    let code = self.expr(value)?;
                    body.push_str(&format!("{temp}->{field} = {code}; "));
                    if Self::managed_c_type(&want) {
                        body.push_str(&format!("ostrin_retain((void*){temp}->{field}); "));
                    }
                }
                Ok(format!("({{ {body} {temp}; }})"))
            }
            HirKind::List(values) => self.list_literal(e, values),
            HirKind::Set(values) => self.set_literal(e, values),
            HirKind::Map(values) => self.map_literal(e, values),
            HirKind::EmptyCollection(_, _) => self.empty_collection(e),
            HirKind::Index(obj, index) => {
                let Ty::List(elem) = &obj.ty else {
                    return Err(());
                };
                if index.ty != Ty::Int || e.ty != **elem {
                    return Err(());
                }
                let list_name = self.mangle_type(&obj.ty)?;
                let obj_code = self.expr(obj)?;
                let index_code = self.expr(index)?;
                Ok(format!("{list_name}_get({obj_code}, {index_code})"))
            }
            HirKind::MethodCall {
                recv,
                method,
                args,
                subst: None,
                type_args,
            } if type_args.is_empty() => {
                if is_option(&recv.ty) {
                    return self.option_method(e, recv, method, args);
                }
                if is_result(&recv.ty) {
                    return self.result_method(e, recv, method, args);
                }
                if is_list(&recv.ty) {
                    return self.list_method(e, recv, method, args);
                }
                if is_map(&recv.ty) {
                    return self.map_method(e, recv, method, args);
                }
                if is_set(&recv.ty) {
                    return self.set_method(e, recv, method, args);
                }
                if args.is_empty() && e.ty == Ty::String {
                    let recv_code = self.expr(recv)?;
                    match &recv.ty {
                        Ty::Float32 => return Ok(format!("ostrin_single_to_string({recv_code})")),
                        Ty::Sized(kind) if kind.is_signed() => return Ok(format!("ostrin_int_to_string({recv_code})")),
                        Ty::Sized(_) => return Ok(format!("ostrin_uint_to_string({recv_code})")),
                        _ => {}
                    }
                }
                let record = self.record_name(&recv.ty)?;
                let (c_name, params, ret) = self
                    .world
                    .methods
                    .get(&(record, method.clone()))
                    .ok_or(())?
                    .clone();
                if params.len() != args.len() + 1
                    || params[0] != self.c_type(&recv.ty)?
                    || args.iter().any(|a| a.name.is_some())
                {
                    return Err(());
                }
                if e.ty == Ty::Void {
                    if ret != "void" {
                        return Err(());
                    }
                } else if ret != self.c_type(&e.ty)? {
                    return Err(());
                }
                let mut codes = vec![self.expr(recv)?];
                for (arg, want) in args.iter().zip(&params[1..]) {
                    if !c_compatible(&self.c_type(&arg.value.ty)?, want) {
                        return Err(());
                    }
                    codes.push(self.expr(&arg.value)?);
                }
                Ok(format!("{c_name}({})", codes.join(", ")))
            }
            HirKind::Unary(op, inner) => {
                if !is_scalar(&inner.ty) {
                    return Err(());
                }
                let code = self.expr(inner)?;
                match op {
                    UnaryOp::Neg if matches!(inner.ty, Ty::Int | Ty::Float | Ty::Float32) => {
                        Ok(format!("(-{code})"))
                    }
                    UnaryOp::Neg if matches!(inner.ty, Ty::Sized(kind) if kind.is_signed()) => Ok(format!("(-{code})")),
                    UnaryOp::Not if inner.ty == Ty::Bool => Ok(format!("(!{code})")),
                    _ => Err(()),
                }
            }
            HirKind::Binary(op, l, r) => {
                if !is_scalar(&l.ty) || !is_scalar(&r.ty) {
                    return Err(());
                }
                if matches!(l.ty, Ty::Sized(_)) || matches!(r.ty, Ty::Sized(_)) {
                    return self.sized_binary(*op, l, r);
                }
                if matches!(l.ty, Ty::Float32) || matches!(r.ty, Ty::Float32) {
                    return self.float32_binary(*op, l, r);
                }
                let (lc, rc) = (self.expr(l)?, self.expr(r)?);
                if l.ty == Ty::String || r.ty == Ty::String {
                    return match op {
                        BinOp::Add if l.ty == Ty::String && r.ty == Ty::String => {
                            // A fresh operand (another concatenation or a call's result)
                            // is released once it has been copied into the new string.
                            let (lf, rf) = (fresh_string(l), fresh_string(r));
                            if !lf && !rf {
                                return Ok(format!("ostrin_str_concat({lc}, {rc})"));
                            }
                            let (a, b, out) = (self.next_temp(), self.next_temp(), self.next_temp());
                            let release_a = if lf { format!("ostrin_release((void*){a}); ") } else { String::new() };
                            let release_b = if rf { format!("ostrin_release((void*){b}); ") } else { String::new() };
                            Ok(format!(
                                "({{ const char* {a} = {lc}; const char* {b} = {rc}; const char* {out} = ostrin_str_concat({a}, {b}); {release_a}{release_b}{out}; }})"
                            ))
                        }
                        BinOp::Eq if l.ty == r.ty => Ok(format!("(strcmp({lc}, {rc}) == 0)")),
                        BinOp::NotEq if l.ty == r.ty => Ok(format!("(strcmp({lc}, {rc}) != 0)")),
                        BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq if l.ty == r.ty => {
                            let c_op = match op {
                                BinOp::Lt => "<",
                                BinOp::Gt => ">",
                                BinOp::LtEq => "<=",
                                _ => ">=",
                            };
                            Ok(format!("(strcmp({lc}, {rc}) {c_op} 0)"))
                        }
                        _ => Err(()),
                    };
                }
                if *op == BinOp::Div && l.ty == Ty::Int && r.ty == Ty::Int {
                    return Ok(format!("ostrin_idiv({lc}, {rc})"));
                }
                if *op == BinOp::Rem {
                    return Ok(if l.ty == Ty::Int && r.ty == Ty::Int {
                        format!("ostrin_irem({lc}, {rc})")
                    } else {
                        format!("fmod({lc}, {rc})")
                    });
                }
                let c_op = match op {
                    BinOp::Add => "+",
                    BinOp::Sub => "-",
                    BinOp::Mul => "*",
                    BinOp::Div => "/",
                    BinOp::Rem => unreachable!("handled above"),
                    BinOp::Eq => "==",
                    BinOp::NotEq => "!=",
                    BinOp::Lt => "<",
                    BinOp::Gt => ">",
                    BinOp::LtEq => "<=",
                    BinOp::GtEq => ">=",
                    BinOp::And => "&&",
                    BinOp::Or => "||",
                };
                Ok(format!("({lc} {c_op} {rc})"))
            }
            HirKind::Call { callee, args, .. }
                if matches!(&callee.kind, HirKind::Global(name) if name == "None")
                    && args.is_empty()
                    && is_option(&e.ty) =>
            {
                Ok(format!("(({}){{ .has = false }})", self.c_type(&e.ty)?))
            }
            HirKind::Call { callee, args, .. } if matches!(&callee.kind, HirKind::Global(name) if matches!(name.as_str(), "Some" | "Ok" | "Err")) =>
            {
                let HirKind::Global(name) = &callee.kind else {
                    return Err(());
                };
                self.constructor(e, name, args)
            }
            HirKind::Call { callee, args, .. }
                if matches!(&callee.kind, HirKind::Global(name) if self.variant(name, &e.ty).is_some()) =>
            {
                let HirKind::Global(name) = &callee.kind else {
                    return Err(());
                };
                self.variant_constructor(e, name, args)
            }
            HirKind::Call {
                callee,
                args,
                subst: None,
                type_args,
            } if type_args.is_empty() => {
                if matches!(&callee.kind, HirKind::Local(_) | HirKind::Lambda(..))
                    && is_fn(&callee.ty)
                {
                    return self.closure_call(e, callee, args);
                }
                let HirKind::Global(name) = &callee.kind else {
                    return Err(());
                };
                if name == "print"
                    && !self.world.functions.contains_key(name)
                    && args.len() == 1
                    && args[0].name.is_none()
                {
                    let arg = &args[0].value;
                    if !is_scalar(&arg.ty) {
                        return Err(());
                    }
                    let code = self.expr(arg)?;
                    return match arg.ty {
                        Ty::Int => Ok(format!("printf(\"%lld\\n\", (long long)({code}))")),
                        Ty::Float => Ok(format!("ostrin_print_float({code})")),
                        Ty::Float32 => Ok(format!("ostrin_print_single({code})")),
                        Ty::Sized(kind) if kind.is_signed() => Ok(format!("printf(\"%lld\\n\", (long long)({code}))")),
                        Ty::Sized(_) => Ok(format!("printf(\"%llu\\n\", (unsigned long long)({code}))")),
                        Ty::Bool => Ok(format!(
                            "printf(\"%s\\n\", (({code}) ? \"true\" : \"false\"))"
                        )),
                        Ty::String => Ok(format!("printf(\"%s\\n\", {code})")),
                        _ => Err(()),
                    };
                }
                let (params, ret) = self.world.functions.get(name).ok_or(())?.clone();
                if params.len() != args.len() || args.iter().any(|a| a.name.is_some()) {
                    return Err(());
                }
                for (arg, want) in args.iter().zip(&params) {
                    if !c_compatible(&self.c_type(&arg.value.ty)?, want) {
                        return Err(());
                    }
                }
                if e.ty == Ty::Void {
                    if ret != "void" {
                        return Err(());
                    }
                } else if !c_compatible(&self.c_type(&e.ty)?, &ret) {
                    return Err(());
                }
                let codes = args
                    .iter()
                    .map(|a| self.expr(&a.value))
                    .collect::<Bail<Vec<_>>>()?;
                let emitted_name = self
                    .world
                    .function_c_names
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| (self.world.c_name)(name));
                Ok(format!("{}({})", emitted_name, codes.join(", ")))
            }
            HirKind::If(cond, then_block, Some(else_block)) if e.ty != Ty::Void => {
                let c = self.expr(cond)?;
                let (t, f) = (self.block_value(then_block)?, self.block_value(else_block)?);
                Ok(format!("(({c}) ? {t} : {f})"))
            }
            HirKind::Block(block) => self.block_value(block),
            HirKind::Try(inner, handler) => self.try_expr(inner, handler.as_deref()),
            HirKind::As(inner, target) => {
                let code = self.expr(inner)?;
                match target.as_str() {
                    "Float" | "Float64" => Ok(format!("((double)({code}))")),
                    "Float32" => Ok(format!("((float)({code}))")),
                    _ => Err(()),
                }
            }
            _ => Err(()),
        }
    }

    fn constructor(
        &mut self,
        e: &HirExpr,
        name: &str,
        args: &[crate::hir::HirArg],
    ) -> Bail<String> {
        if args.len() != 1 || args[0].name.is_some() {
            return Err(());
        }
        let (container, field, flag, expected) = match (name, &e.ty) {
            ("Some", Ty::Applied(n, ts)) if n == "Option" && ts.len() == 1 => {
                (self.c_type(&e.ty)?, "value", ".has = true", ts[0].clone())
            }
            ("Ok", Ty::Applied(n, ts)) if n == "Result" && ts.len() == 2 => {
                (self.c_type(&e.ty)?, "value", ".ok = true", ts[0].clone())
            }
            ("Err", Ty::Applied(n, ts)) if n == "Result" && ts.len() == 2 => {
                (self.c_type(&e.ty)?, "error", ".ok = false", ts[1].clone())
            }
            _ => return Err(()),
        };
        if args[0].value.ty != expected
            || !c_compatible(&self.c_type(&args[0].value.ty)?, &self.c_type(&expected)?)
        {
            return Err(());
        }
        let value = self.expr(&args[0].value)?;
        if self.managed(&expected) && Self::borrowed_expr(&args[0].value) {
            let temp = self.next_temp();
            Ok(format!(
                "({{ {container} {temp} = (({container}){{ {flag}, .{field} = {value} }}); ostrin_retain((void*){temp}.{field}); {temp}; }})"
            ))
        } else {
            Ok(format!("(({container}){{ {flag}, .{field} = {value} }})"))
        }
    }

    fn option_method(
        &mut self,
        e: &HirExpr,
        recv: &HirExpr,
        method: &str,
        args: &[crate::hir::HirArg],
    ) -> Bail<String> {
        let inner = option_inner(&recv.ty)?;
        if args.iter().any(|a| a.name.is_some()) {
            return Err(());
        }
        let rc = self.c_type(&recv.ty)?;
        let recv_code = self.expr(recv)?;
        let temp = self.next_temp();
        match method {
            "is_some" if args.is_empty() && e.ty == Ty::Bool => Ok(format!("({{ {rc} {temp} = {recv_code}; {temp}.has; }})")),
            "is_none" if args.is_empty() && e.ty == Ty::Bool => Ok(format!("({{ {rc} {temp} = {recv_code}; !{temp}.has; }})")),
            "unwrap" if args.is_empty() && e.ty == inner => Ok(format!("({{ {rc} {temp} = {recv_code}; if (!{temp}.has) {{ fprintf(stderr, \"ostrin: unwrap on None\\n\"); exit(1); }} {temp}.value; }})")),
            "unwrap_or" if args.len() == 1 && e.ty == inner && args[0].value.ty == inner => {
                let fallback = self.expr(&args[0].value)?;
                Ok(format!("({{ {rc} {temp} = {recv_code}; {temp}.has ? {temp}.value : ({fallback}); }})"))
            }
            "ok_or" if args.len() == 1 => {
                let error = args[0].value.ty.clone();
                let Ty::Applied(n, result_args) = &e.ty else { return Err(()) };
                if n != "Result" || result_args.len() != 2 || result_args[0] != inner || result_args[1] != error {
                    return Err(());
                }
                let result_c = self.c_type(&e.ty)?;
                let error_code = self.expr(&args[0].value)?;
                Ok(format!("({{ {rc} {temp} = {recv_code}; {result_c} __hir_r; memset(&__hir_r, 0, sizeof __hir_r); if ({temp}.has) {{ __hir_r.ok = true; __hir_r.value = {temp}.value; }} else {{ __hir_r.ok = false; __hir_r.error = {error_code}; }} __hir_r; }})"))
            }
            _ => Err(()),
        }
    }

    fn result_method(
        &mut self,
        e: &HirExpr,
        recv: &HirExpr,
        method: &str,
        args: &[crate::hir::HirArg],
    ) -> Bail<String> {
        let (ok, _err) = result_types(&recv.ty)?;
        if args.iter().any(|a| a.name.is_some()) {
            return Err(());
        }
        let rc = self.c_type(&recv.ty)?;
        let recv_code = self.expr(recv)?;
        let temp = self.next_temp();
        match method {
            "is_ok" if args.is_empty() && e.ty == Ty::Bool => Ok(format!("({{ {rc} {temp} = {recv_code}; {temp}.ok; }})")),
            "is_err" if args.is_empty() && e.ty == Ty::Bool => Ok(format!("({{ {rc} {temp} = {recv_code}; !{temp}.ok; }})")),
            "unwrap" if args.is_empty() && e.ty == ok => Ok(format!("({{ {rc} {temp} = {recv_code}; if (!{temp}.ok) {{ fprintf(stderr, \"ostrin: unwrap on Err\\n\"); exit(1); }} {temp}.value; }})")),
            "unwrap_or" if args.len() == 1 && e.ty == ok && args[0].value.ty == ok => {
                let fallback = self.expr(&args[0].value)?;
                Ok(format!("({{ {rc} {temp} = {recv_code}; {temp}.ok ? {temp}.value : ({fallback}); }})"))
            }
            "ok" if args.is_empty() => {
                let option = Ty::Applied("Option".to_string(), vec![ok.clone()]);
                if e.ty != option {
                    return Err(());
                }
                let option_c = self.c_type(&option)?;
                Ok(format!("({{ {rc} {temp} = {recv_code}; {option_c} __hir_o; memset(&__hir_o, 0, sizeof __hir_o); if ({temp}.ok) {{ __hir_o.has = true; __hir_o.value = {temp}.value; }} __hir_o; }})"))
            }
            _ => Err(()),
        }
    }

    fn lambda_expr(&mut self, e: &HirExpr, params: &[String], body: &HirBlock) -> Bail<String> {
        let Ty::Fn(param_tys, ret) = &e.ty else {
            return Err(());
        };
        if params.len() != param_tys.len() || hir_contains_lambda(body) {
            return Err(());
        }
        let captures = self.lambda_captures(params, body)?;
        let id = self.world.closure_counter.get();
        self.world.closure_counter.set(id + 1);
        let env_name = format!("OstrinHirEnv_{id}");
        let fn_name = format!("ostrin_hir_lambda_{id}");
        let ret_c = self.c_type(ret)?;
        let param_cs = param_tys
            .iter()
            .map(|ty| self.c_type(ty))
            .collect::<Bail<Vec<_>>>()?;
        let env_fields = captures
            .iter()
            .map(|(name, ty)| Ok(format!("{} {name}; ", self.c_type(ty)?)))
            .collect::<Bail<Vec<_>>>()?;
        let params_c = params
            .iter()
            .zip(&param_cs)
            .map(|(name, ty)| format!(", {ty} {name}"))
            .collect::<String>();

        let mut names = params.iter().cloned().collect::<HashSet<_>>();
        names.extend(captures.iter().map(|(name, _)| name.clone()));
        let mut lambda_emitter = Emitter {
            world: self.world,
            scopes: vec![names],
            ret: (**ret).clone(),
            temp: 0,
            owned_locals: Vec::new(),
            owned_block_locals: Vec::new(),
            ownership_control_frames: Vec::new(),
        };
        let mut body_c = String::new();
        lambda_emitter.body(body, &mut body_c)?;

        let mut prototype = Vec::new();
        if !captures.is_empty() {
            prototype.push(format!(
                "typedef struct {{ {} }} {env_name};",
                env_fields.join("")
            ));
        }
        let signature = format!("static {ret_c} {fn_name}(void* __env{params_c})");
        if !captures.is_empty() {
            body_c = format!(
                "    {env_name}* __e = __env;\n{}",
                captures
                    .iter()
                    .map(|(name, ty)| format!(
                        "    {} {name} = __e->{name};\n",
                        self.c_type(ty).unwrap()
                    ))
                    .collect::<String>()
                    + &body_c
            );
        }
        prototype.push(format!("{signature};"));
        self.world.closure_protos.borrow_mut().extend(prototype);
        self.world
            .closure_bodies
            .borrow_mut()
            .push((signature, body_c));

        let env = if captures.is_empty() {
            "NULL".to_string()
        } else {
            let assignments = captures
                .iter()
                .map(|(name, _)| format!("__ce->{name} = {name}; "))
                .collect::<String>();
            format!(
                "({{ {env_name}* __ce = malloc(sizeof *__ce); if (!__ce) OSTRIN_OOM(); {assignments} (void*)__ce; }})"
            )
        };
        Ok(format!("((OstrinClosure){{ (void*){fn_name}, {env} }})"))
    }

    fn lambda_captures(&self, params: &[String], body: &HirBlock) -> Bail<Vec<(String, Ty)>> {
        let mut used = HashMap::new();
        let mut bound = params.iter().cloned().collect::<HashSet<_>>();
        collect_hir_locals_block(body, &mut used, &mut bound);
        let mut names = used
            .into_iter()
            .filter(|(name, _)| self.declared(name) && !bound.contains(name))
            .collect::<Vec<_>>();
        names.sort_by(|a, b| a.0.cmp(&b.0));
        for (_, ty) in &names {
            self.c_type(ty)?;
        }
        Ok(names)
    }

    fn function_value(&mut self, e: &HirExpr, name: &str) -> Bail<String> {
        let Ty::Fn(params, ret) = &e.ty else {
            return Err(());
        };
        let Some((world_params, world_ret)) = self.world.functions.get(name) else {
            return Err(());
        };
        if world_params.len() != params.len()
            || world_params.iter().zip(params).any(|(actual, expected)| {
                !c_compatible(actual, &self.c_type(expected).unwrap_or_default())
            })
            || *world_ret != self.c_type(ret)?
        {
            return Err(());
        }
        let id = self.world.closure_counter.get();
        self.world.closure_counter.set(id + 1);
        let fn_name = format!("ostrin_hir_thunk_{id}");
        let params_c = world_params
            .iter()
            .enumerate()
            .map(|(index, ty)| format!(", {ty} a{index}"))
            .collect::<String>();
        let args = (0..params.len())
            .map(|index| format!("a{index}"))
            .collect::<Vec<_>>()
            .join(", ");
        let signature = format!("static {world_ret} {fn_name}(void* __env{params_c})");
        let call = format!("{}({args})", (self.world.c_name)(name));
        let body = if **ret == Ty::Void {
            format!("    (void)__env; {call};\n")
        } else {
            format!("    (void)__env; return {call};\n")
        };
        self.world
            .closure_protos
            .borrow_mut()
            .push(format!("{signature};"));
        self.world
            .closure_bodies
            .borrow_mut()
            .push((signature, body));
        Ok(format!("((OstrinClosure){{ (void*){fn_name}, NULL }})"))
    }

    fn closure_call(
        &mut self,
        e: &HirExpr,
        callee: &HirExpr,
        args: &[crate::hir::HirArg],
    ) -> Bail<String> {
        let Ty::Fn(params, ret) = &callee.ty else {
            return Err(());
        };
        if args.len() != params.len() || args.iter().any(|arg| arg.name.is_some()) {
            return Err(());
        }
        if e.ty != **ret {
            return Err(());
        }
        let callee_code = self.expr(callee)?;
        let mut arg_codes = Vec::with_capacity(args.len());
        for (arg, param) in args.iter().zip(params) {
            if !c_compatible(&self.c_type(&arg.value.ty)?, &self.c_type(param)?) {
                return Err(());
            }
            arg_codes.push(self.expr(&arg.value)?);
        }
        self.closure_call_raw(&callee_code, params, ret, &arg_codes)
    }

    fn closure_call_raw(
        &mut self,
        callee_code: &str,
        params: &[Ty],
        ret: &Ty,
        arg_codes: &[String],
    ) -> Bail<String> {
        let fn_type = self.closure_fn_type(params, ret)?;
        if arg_codes.len() != params.len() {
            return Err(());
        }
        let temp = self.next_temp();
        let rest = if arg_codes.is_empty() {
            String::new()
        } else {
            format!(", {}", arg_codes.join(", "))
        };
        Ok(format!(
            "({{ OstrinClosure {temp} = {callee_code}; (({fn_type}){temp}.fn)({temp}.env{rest}); }})"
        ))
    }

    fn closure_fn_type(&self, params: &[Ty], ret: &Ty) -> Bail<String> {
        let params = params
            .iter()
            .map(|ty| self.c_type(ty))
            .collect::<Bail<Vec<_>>>()?;
        Ok(format!(
            "{} (*)(void*{})",
            self.c_type(ret)?,
            params
                .iter()
                .map(|ty| format!(", {ty}"))
                .collect::<String>()
        ))
    }

    fn list_literal(&mut self, e: &HirExpr, values: &[HirExpr]) -> Bail<String> {
        let Ty::List(elem) = &e.ty else {
            return Err(());
        };
        let elem_c = self.c_type(elem)?;
        let mut codes = Vec::with_capacity(values.len());
        for value in values {
            if !c_compatible(&self.c_type(&value.ty)?, &elem_c) {
                return Err(());
            }
            codes.push(self.expr(value)?);
        }
        let name = self.mangle_type(&e.ty)?;
        if values.is_empty() {
            return Ok(format!("{name}_new_from_array(NULL, 0)"));
        }
        Ok(format!(
            "{name}_new_from_array(({elem_c}[]){{ {} }}, {})",
            codes.join(", "),
            values.len()
        ))
    }

    fn set_literal(&mut self, e: &HirExpr, values: &[HirExpr]) -> Bail<String> {
        let Ty::Set(elem) = &e.ty else { return Err(()) };
        let elem_c = self.c_type(elem)?;
        let name = self.mangle_type(&e.ty)?;
        let temp = self.next_temp();
        let mut body = format!("{name}* {temp} = {name}_new(); ");
        for value in values {
            if !c_compatible(&self.c_type(&value.ty)?, &elem_c) {
                return Err(());
            }
            let code = self.expr(value)?;
            body.push_str(&format!("{name}_add({temp}, {code}); "));
        }
        Ok(format!("({{ {body} {temp}; }})"))
    }

    fn map_literal(&mut self, e: &HirExpr, values: &[(HirExpr, HirExpr)]) -> Bail<String> {
        let Ty::Map(key, value) = &e.ty else {
            return Err(());
        };
        let key_c = self.c_type(key)?;
        let value_c = self.c_type(value)?;
        let name = self.mangle_type(&e.ty)?;
        let temp = self.next_temp();
        let mut body = format!("{name}* {temp} = {name}_new(); ");
        for (key_expr, value_expr) in values {
            if !c_compatible(&self.c_type(&key_expr.ty)?, &key_c)
                || !c_compatible(&self.c_type(&value_expr.ty)?, &value_c)
            {
                return Err(());
            }
            let key_code = self.expr(key_expr)?;
            let value_code = self.expr(value_expr)?;
            body.push_str(&format!("{name}_set({temp}, {key_code}, {value_code}); "));
        }
        Ok(format!("({{ {body} {temp}; }})"))
    }

    fn empty_collection(&mut self, e: &HirExpr) -> Bail<String> {
        if !matches!(e.ty, Ty::Map(..) | Ty::Set(_)) {
            return Err(());
        }
        Ok(format!("{}_new()", self.mangle_type(&e.ty)?))
    }

    fn list_method(
        &mut self,
        e: &HirExpr,
        recv: &HirExpr,
        method: &str,
        args: &[crate::hir::HirArg],
    ) -> Bail<String> {
        let Ty::List(elem) = &recv.ty else {
            return Err(());
        };
        if args.iter().any(|arg| arg.name.is_some()) {
            return Err(());
        }
        let elem_c = self.c_type(elem)?;
        let recv_c = self.c_type(&recv.ty)?;
        let name = self.mangle_type(&recv.ty)?;
        if matches!(method, "map" | "filter" | "fold" | "any" | "all" | "find") {
            return self.list_combinator(e, recv, method, args);
        }
        let recv_code = self.expr(recv)?;
        match method {
            "length" | "count" if args.is_empty() && e.ty == Ty::Int => {
                Ok(format!("{name}_length({recv_code})"))
            }
            "push" if args.len() == 1 && e.ty == Ty::Void => {
                if !c_compatible(&self.c_type(&args[0].value.ty)?, &elem_c) {
                    return Err(());
                }
                let value = self.expr(&args[0].value)?;
                Ok(format!("{name}_push({recv_code}, {value})"))
            }
            "remove_at" if args.len() == 1 && e.ty == **elem => {
                if args[0].value.ty != Ty::Int {
                    return Err(());
                }
                let index = self.expr(&args[0].value)?;
                Ok(format!("{name}_remove_at({recv_code}, {index})"))
            }
            "join" if **elem == Ty::String && args.len() == 1 && e.ty == Ty::String => {
                if args[0].value.ty != Ty::String {
                    return Err(());
                }
                let separator = self.expr(&args[0].value)?;
                let temp = self.next_temp();
                Ok(format!(
                    "({{ {recv_c} {temp} = {recv_code}; ostrin_s_join({temp}->items, {temp}->length, {separator}); }})"
                ))
            }
            _ => Err(()),
        }
    }

    fn list_combinator(
        &mut self,
        e: &HirExpr,
        recv: &HirExpr,
        method: &str,
        args: &[crate::hir::HirArg],
    ) -> Bail<String> {
        let Ty::List(elem) = &recv.ty else {
            return Err(());
        };
        if args.iter().any(|arg| arg.name.is_some()) {
            return Err(());
        }
        let recv_c = self.c_type(&recv.ty)?;
        let recv_code = self.expr(recv)?;
        let list_name = self.mangle_type(&recv.ty)?;
        let list_temp = self.next_temp();
        let closure_temp = self.next_temp();
        let index_temp = self.next_temp();
        let mut head =
            format!("({{ {recv_c} {list_temp} = {recv_code}; OstrinClosure {closure_temp} = ");

        match method {
            "map" => {
                let [closure] = args else { return Err(()) };
                let Ty::Fn(params, ret) = &closure.value.ty else {
                    return Err(());
                };
                if params.as_slice() != [(*elem.clone())] {
                    return Err(());
                }
                let Ty::List(out_elem) = &e.ty else {
                    return Err(());
                };
                if **ret != **out_elem {
                    return Err(());
                }
                let closure_code = self.expr(&closure.value)?;
                let out_c = self.c_type(out_elem)?;
                let out_ty = Ty::List(out_elem.clone());
                let out_name = self.mangle_type(&out_ty)?;
                let out_temp = self.next_temp();
                let fn_type = self.closure_fn_type(params, ret)?;
                head.push_str(&format!(
                    "{closure_code}; {out_name}* {out_temp} = {out_name}_new_from_array(NULL, 0); for (int64_t {index_temp} = 0; {index_temp} < {list_temp}->length; {index_temp}++) {{ {out_c} __hir_item = (({fn_type}){closure_temp}.fn)({closure_temp}.env, {list_temp}->items[{index_temp}]); "
                ));
                head.push_str(&format!(
                    "{out_name}_push({out_temp}, __hir_item); }} {out_temp}; }})"
                ));
                return Ok(head);
            }
            "filter" => {
                let [closure] = args else { return Err(()) };
                let Ty::Fn(params, ret) = &closure.value.ty else {
                    return Err(());
                };
                if params.as_slice() != [(*elem.clone())] || **ret != Ty::Bool || e.ty != recv.ty {
                    return Err(());
                }
                let closure_code = self.expr(&closure.value)?;
                let out_temp = self.next_temp();
                let fn_type = self.closure_fn_type(params, ret)?;
                head.push_str(&format!(
                    "{closure_code}; {list_name}* {out_temp} = {list_name}_new_from_array(NULL, 0); for (int64_t {index_temp} = 0; {index_temp} < {list_temp}->length; {index_temp}++) {{ if ((({fn_type}){closure_temp}.fn)({closure_temp}.env, {list_temp}->items[{index_temp}])) {list_name}_push({out_temp}, {list_temp}->items[{index_temp}]); }} {out_temp}; }})"
                ));
                Ok(head)
            }
            "fold" => {
                let [initial, closure] = args else {
                    return Err(());
                };
                let Ty::Fn(params, ret) = &closure.value.ty else {
                    return Err(());
                };
                if params.len() != 2 || params[1] != **elem || **ret != e.ty {
                    return Err(());
                }
                if !c_compatible(&self.c_type(&initial.value.ty)?, &self.c_type(&e.ty)?) {
                    return Err(());
                }
                let closure_code = self.expr(&closure.value)?;
                let accumulator = self.expr(&initial.value)?;
                let acc_c = self.c_type(&e.ty)?;
                let fn_type = self.closure_fn_type(params, ret)?;
                head.push_str(&format!(
                    "{closure_code}; {acc_c} __hir_acc = {accumulator}; for (int64_t {index_temp} = 0; {index_temp} < {list_temp}->length; {index_temp}++) {{ __hir_acc = (({fn_type}){closure_temp}.fn)({closure_temp}.env, __hir_acc, {list_temp}->items[{index_temp}]); }} __hir_acc; }})"
                ));
                Ok(head)
            }
            "any" | "all" => {
                let [closure] = args else { return Err(()) };
                let Ty::Fn(params, ret) = &closure.value.ty else {
                    return Err(());
                };
                if params.as_slice() != [(*elem.clone())] || **ret != Ty::Bool || e.ty != Ty::Bool {
                    return Err(());
                }
                let closure_code = self.expr(&closure.value)?;
                let initial = if method == "all" { "true" } else { "false" };
                let wanted = if method == "all" {
                    "!__hir_pred"
                } else {
                    "__hir_pred"
                };
                let fn_type = self.closure_fn_type(params, ret)?;
                head.push_str(&format!(
                    "{closure_code}; bool __hir_result = {initial}; for (int64_t {index_temp} = 0; {index_temp} < {list_temp}->length; {index_temp}++) {{ bool __hir_pred = (({fn_type}){closure_temp}.fn)({closure_temp}.env, {list_temp}->items[{index_temp}]); if ({wanted}) {{ __hir_result = __hir_pred; break; }} }} __hir_result; }})"
                ));
                Ok(head)
            }
            "find" => {
                let [closure] = args else { return Err(()) };
                let Ty::Fn(params, ret) = &closure.value.ty else {
                    return Err(());
                };
                if params.as_slice() != [(*elem.clone())] || **ret != Ty::Bool {
                    return Err(());
                }
                let expected = Ty::Applied("Option".to_string(), vec![(*elem.clone())]);
                if e.ty != expected {
                    return Err(());
                }
                let option_c = self.c_type(&e.ty)?;
                let closure_code = self.expr(&closure.value)?;
                let fn_type = self.closure_fn_type(params, ret)?;
                head.push_str(&format!(
                    "{closure_code}; {option_c} __hir_found; memset(&__hir_found, 0, sizeof __hir_found); for (int64_t {index_temp} = 0; {index_temp} < {list_temp}->length; {index_temp}++) {{ if ((({fn_type}){closure_temp}.fn)({closure_temp}.env, {list_temp}->items[{index_temp}])) {{ __hir_found.has = true; __hir_found.value = {list_temp}->items[{index_temp}]; break; }} }} __hir_found; }})"
                ));
                Ok(head)
            }
            _ => Err(()),
        }
    }

    fn map_method(
        &mut self,
        e: &HirExpr,
        recv: &HirExpr,
        method: &str,
        args: &[crate::hir::HirArg],
    ) -> Bail<String> {
        let Ty::Map(key, value) = &recv.ty else {
            return Err(());
        };
        if args.iter().any(|arg| arg.name.is_some()) {
            return Err(());
        }
        let key_c = self.c_type(key)?;
        let value_c = self.c_type(value)?;
        let recv_code = self.expr(recv)?;
        let name = self.mangle_type(&recv.ty)?;
        match method {
            "get" | "remove"
                if args.len() == 1 && c_compatible(&self.c_type(&args[0].value.ty)?, &key_c) =>
            {
                let expected = Ty::Applied("Option".to_string(), vec![(*value.clone())]);
                if e.ty != expected {
                    return Err(());
                }
                let key_code = self.expr(&args[0].value)?;
                Ok(format!("{name}_{method}({recv_code}, {key_code})"))
            }
            "contains_key"
                if args.len() == 1
                    && e.ty == Ty::Bool
                    && c_compatible(&self.c_type(&args[0].value.ty)?, &key_c) =>
            {
                let key_code = self.expr(&args[0].value)?;
                Ok(format!("{name}_contains_key({recv_code}, {key_code})"))
            }
            "count" if args.is_empty() && e.ty == Ty::Int => {
                Ok(format!("{name}_count({recv_code})"))
            }
            "set" if args.len() == 2 && e.ty == Ty::Void => {
                if !c_compatible(&self.c_type(&args[0].value.ty)?, &key_c)
                    || !c_compatible(&self.c_type(&args[1].value.ty)?, &value_c)
                {
                    return Err(());
                }
                let key_code = self.expr(&args[0].value)?;
                let value_code = self.expr(&args[1].value)?;
                Ok(format!("{name}_set({recv_code}, {key_code}, {value_code})"))
            }
            "keys" if args.is_empty() && e.ty == Ty::List(Box::new((**key).clone())) => {
                Ok(format!("{name}_keys({recv_code})"))
            }
            "values" if args.is_empty() && e.ty == Ty::List(Box::new((**value).clone())) => {
                Ok(format!("{name}_values({recv_code})"))
            }
            _ => Err(()),
        }
    }

    fn set_method(
        &mut self,
        e: &HirExpr,
        recv: &HirExpr,
        method: &str,
        args: &[crate::hir::HirArg],
    ) -> Bail<String> {
        let Ty::Set(elem) = &recv.ty else {
            return Err(());
        };
        if args.iter().any(|arg| arg.name.is_some()) {
            return Err(());
        }
        let elem_c = self.c_type(elem)?;
        let recv_code = self.expr(recv)?;
        let name = self.mangle_type(&recv.ty)?;
        match method {
            "contains" if args.len() == 1 && e.ty == Ty::Bool => {
                if !c_compatible(&self.c_type(&args[0].value.ty)?, &elem_c) {
                    return Err(());
                }
                let item = self.expr(&args[0].value)?;
                Ok(format!("{name}_contains({recv_code}, {item})"))
            }
            "add" | "remove" if args.len() == 1 && e.ty == Ty::Void => {
                if !c_compatible(&self.c_type(&args[0].value.ty)?, &elem_c) {
                    return Err(());
                }
                let item = self.expr(&args[0].value)?;
                Ok(format!("{name}_{method}({recv_code}, {item})"))
            }
            "count" if args.is_empty() && e.ty == Ty::Int => {
                Ok(format!("{name}_count({recv_code})"))
            }
            _ => Err(()),
        }
    }

    fn try_expr(&mut self, inner: &HirExpr, handler: Option<&HirExpr>) -> Bail<String> {
        if handler.is_some() {
            return Err(());
        }
        let container = self.c_type(&inner.ty)?;
        let value = self.expr(inner)?;
        let temp = self.next_temp();
        match (&inner.ty, &self.ret) {
            (Ty::Applied(n, args), Ty::Applied(ret_n, ret_args))
                if n == "Option" && ret_n == "Option" && args.len() == 1 && ret_args.len() == 1 =>
            {
                if args[0] != ret_args[0] {
                    return Err(());
                }
                let ret_c = self.c_type(&self.ret)?;
                Ok(format!("({{ {container} {temp} = {value}; if (!{temp}.has) {{ return (({ret_c}){{ .has = false }}); }} {temp}.value; }})"))
            }
            (Ty::Applied(n, args), Ty::Applied(ret_n, ret_args))
                if n == "Result" && ret_n == "Result" && args.len() == 2 && ret_args.len() == 2 =>
            {
                if args[1] != ret_args[1] {
                    return Err(());
                }
                let ret_c = self.c_type(&self.ret)?;
                Ok(format!("({{ {container} {temp} = {value}; if (!{temp}.ok) {{ return (({ret_c}){{ .ok = false, .error = {temp}.error }}); }} {temp}.value; }})"))
            }
            _ => Err(()),
        }
    }
}

fn hir_contains_lambda(block: &HirBlock) -> bool {
    block.stmts.iter().any(hir_stmt_contains_lambda)
        || block.tail.as_deref().is_some_and(hir_expr_contains_lambda)
}

fn hir_stmt_contains_lambda(stmt: &HirStmt) -> bool {
    match stmt {
        HirStmt::Let { value, .. } | HirStmt::Assign { value, .. } => {
            hir_expr_contains_lambda(value)
        }
        HirStmt::FieldAssign { target, value } => {
            hir_expr_contains_lambda(target) || hir_expr_contains_lambda(value)
        }
        HirStmt::Return(value) | HirStmt::Break(value) => {
            value.as_ref().is_some_and(hir_expr_contains_lambda)
        }
        HirStmt::Continue => false,
        HirStmt::While { cond, body }
        | HirStmt::For {
            iter: cond, body, ..
        } => hir_expr_contains_lambda(cond) || hir_contains_lambda(body),
        HirStmt::Expr(value) => hir_expr_contains_lambda(value),
    }
}

fn hir_expr_contains_lambda(expr: &HirExpr) -> bool {
    match &expr.kind {
        HirKind::Lambda(..) => true,
        HirKind::Unit(value, _)
        | HirKind::Unary(_, value)
        | HirKind::Field(value, _)
        | HirKind::As(value, _)
        | HirKind::Try(value, None) => hir_expr_contains_lambda(value),
        HirKind::Try(value, Some(handler)) => {
            hir_expr_contains_lambda(value) || hir_expr_contains_lambda(handler)
        }
        HirKind::Binary(_, left, right)
        | HirKind::Index(left, right)
        | HirKind::Within(left, right) => {
            hir_expr_contains_lambda(left) || hir_expr_contains_lambda(right)
        }
        HirKind::Approximately(a, b, tolerance) => {
            hir_expr_contains_lambda(a)
                || hir_expr_contains_lambda(b)
                || hir_expr_contains_lambda(tolerance)
        }
        HirKind::Range(start, _, end, step) => {
            hir_expr_contains_lambda(start)
                || hir_expr_contains_lambda(end)
                || step.as_deref().is_some_and(hir_expr_contains_lambda)
        }
        HirKind::Call { callee, args, .. } => {
            hir_expr_contains_lambda(callee)
                || args.iter().any(|arg| hir_expr_contains_lambda(&arg.value))
        }
        HirKind::MethodCall { recv, args, .. } => {
            hir_expr_contains_lambda(recv)
                || args.iter().any(|arg| hir_expr_contains_lambda(&arg.value))
        }
        HirKind::If(cond, then_block, else_block) => {
            hir_expr_contains_lambda(cond)
                || hir_contains_lambda(then_block)
                || else_block.as_ref().is_some_and(hir_contains_lambda)
        }
        HirKind::Block(block)
        | HirKind::Loop(block)
        | HirKind::Spawn(block)
        | HirKind::SpawnScope(block) => hir_contains_lambda(block),
        HirKind::List(values) | HirKind::Set(values) => values.iter().any(hir_expr_contains_lambda),
        HirKind::Map(values) => values
            .iter()
            .any(|(key, value)| hir_expr_contains_lambda(key) || hir_expr_contains_lambda(value)),
        HirKind::Record { fields, .. } => fields
            .iter()
            .any(|(_, value)| hir_expr_contains_lambda(value)),
        HirKind::Match(scrutinee, arms) => {
            hir_expr_contains_lambda(scrutinee)
                || arms.iter().any(|arm| {
                    arm.guard.as_ref().is_some_and(hir_expr_contains_lambda)
                        || hir_contains_lambda(&arm.body)
                })
        }
        HirKind::Channel(_, capacity) => capacity.as_deref().is_some_and(hir_expr_contains_lambda),
        HirKind::Int(_)
        | HirKind::Sized(..)
        | HirKind::Float(_)
        | HirKind::Float32(_)
        | HirKind::Str(_)
        | HirKind::Char(_)
        | HirKind::Bool(_)
        | HirKind::Local(_)
        | HirKind::Global(_)
        | HirKind::EmptyCollection(..) => false,
    }
}

fn collect_hir_locals_block(
    block: &HirBlock,
    used: &mut HashMap<String, Ty>,
    bound: &mut HashSet<String>,
) {
    for stmt in &block.stmts {
        match stmt {
            HirStmt::Let { name, value, .. } => {
                collect_hir_locals_expr(value, used, bound);
                bound.insert(name.clone());
            }
            HirStmt::Assign { value, .. } => collect_hir_locals_expr(value, used, bound),
            HirStmt::FieldAssign { target, value } => {
                collect_hir_locals_expr(target, used, bound);
                collect_hir_locals_expr(value, used, bound);
            }
            HirStmt::Return(value) | HirStmt::Break(value) => {
                if let Some(value) = value {
                    collect_hir_locals_expr(value, used, bound);
                }
            }
            HirStmt::Continue => {}
            HirStmt::While { cond, body } => {
                collect_hir_locals_expr(cond, used, bound);
                collect_hir_locals_block(body, used, bound);
            }
            HirStmt::For { var, iter, body } => {
                collect_hir_locals_expr(iter, used, bound);
                bound.insert(var.clone());
                collect_hir_locals_block(body, used, bound);
            }
            HirStmt::Expr(value) => collect_hir_locals_expr(value, used, bound),
        }
    }
    if let Some(tail) = &block.tail {
        collect_hir_locals_expr(tail, used, bound);
    }
}

fn collect_hir_locals_expr(
    expr: &HirExpr,
    used: &mut HashMap<String, Ty>,
    bound: &mut HashSet<String>,
) {
    match &expr.kind {
        HirKind::Local(name) => {
            used.entry(name.clone()).or_insert_with(|| expr.ty.clone());
        }
        HirKind::Unit(value, _)
        | HirKind::Unary(_, value)
        | HirKind::Field(value, _)
        | HirKind::As(value, _)
        | HirKind::Try(value, None) => collect_hir_locals_expr(value, used, bound),
        HirKind::Try(value, Some(handler)) => {
            collect_hir_locals_expr(value, used, bound);
            collect_hir_locals_expr(handler, used, bound);
        }
        HirKind::Binary(_, left, right)
        | HirKind::Index(left, right)
        | HirKind::Within(left, right) => {
            collect_hir_locals_expr(left, used, bound);
            collect_hir_locals_expr(right, used, bound);
        }
        HirKind::Approximately(a, b, tolerance) => {
            collect_hir_locals_expr(a, used, bound);
            collect_hir_locals_expr(b, used, bound);
            collect_hir_locals_expr(tolerance, used, bound);
        }
        HirKind::Range(start, _, end, step) => {
            collect_hir_locals_expr(start, used, bound);
            collect_hir_locals_expr(end, used, bound);
            if let Some(step) = step {
                collect_hir_locals_expr(step, used, bound);
            }
        }
        HirKind::Call { callee, args, .. } => {
            collect_hir_locals_expr(callee, used, bound);
            for arg in args {
                collect_hir_locals_expr(&arg.value, used, bound);
            }
        }
        HirKind::MethodCall { recv, args, .. } => {
            collect_hir_locals_expr(recv, used, bound);
            for arg in args {
                collect_hir_locals_expr(&arg.value, used, bound);
            }
        }
        HirKind::If(cond, then_block, else_block) => {
            collect_hir_locals_expr(cond, used, bound);
            collect_hir_locals_block(then_block, used, bound);
            if let Some(else_block) = else_block {
                collect_hir_locals_block(else_block, used, bound);
            }
        }
        HirKind::Block(block)
        | HirKind::Loop(block)
        | HirKind::Spawn(block)
        | HirKind::SpawnScope(block)
        | HirKind::Lambda(_, block) => collect_hir_locals_block(block, used, bound),
        HirKind::List(values) | HirKind::Set(values) => {
            for value in values {
                collect_hir_locals_expr(value, used, bound);
            }
        }
        HirKind::Map(values) => {
            for (key, value) in values {
                collect_hir_locals_expr(key, used, bound);
                collect_hir_locals_expr(value, used, bound);
            }
        }
        HirKind::Record { fields, .. } => {
            for (_, value) in fields {
                collect_hir_locals_expr(value, used, bound);
            }
        }
        HirKind::Match(scrutinee, arms) => {
            collect_hir_locals_expr(scrutinee, used, bound);
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    collect_hir_locals_expr(guard, used, bound);
                }
                collect_hir_locals_block(&arm.body, used, bound);
            }
        }
        HirKind::Channel(_, capacity) => {
            if let Some(capacity) = capacity {
                collect_hir_locals_expr(capacity, used, bound);
            }
        }
        HirKind::Int(_)
        | HirKind::Sized(..)
        | HirKind::Float(_)
        | HirKind::Float32(_)
        | HirKind::Str(_)
        | HirKind::Char(_)
        | HirKind::Bool(_)
        | HirKind::Global(_)
        | HirKind::EmptyCollection(..) => {}
    }
}

fn is_option(ty: &Ty) -> bool {
    matches!(ty, Ty::Applied(name, args) if name == "Option" && args.len() == 1)
}

fn is_result(ty: &Ty) -> bool {
    matches!(ty, Ty::Applied(name, args) if name == "Result" && args.len() == 2)
}

fn is_list(ty: &Ty) -> bool {
    matches!(ty, Ty::List(_))
}

fn is_map(ty: &Ty) -> bool {
    matches!(ty, Ty::Map(_, _))
}

fn is_set(ty: &Ty) -> bool {
    matches!(ty, Ty::Set(_))
}

fn is_fn(ty: &Ty) -> bool {
    matches!(ty, Ty::Fn(_, _))
}

fn option_inner(ty: &Ty) -> Bail<Ty> {
    let Ty::Applied(name, args) = ty else {
        return Err(());
    };
    (name == "Option" && args.len() == 1)
        .then(|| args[0].clone())
        .ok_or(())
}

fn result_types(ty: &Ty) -> Bail<(Ty, Ty)> {
    let Ty::Applied(name, args) = ty else {
        return Err(());
    };
    (name == "Result" && args.len() == 2)
        .then(|| (args[0].clone(), args[1].clone()))
        .ok_or(())
}

/// A `String` expression whose value the caller owns: a concatenation or the
/// result of a call (functions and string methods return a new reference).
fn fresh_string(e: &HirExpr) -> bool {
    e.ty == Ty::String
        && matches!(
            &e.kind,
            HirKind::Binary(BinOp::Add, ..) | HirKind::Call { .. } | HirKind::MethodCall { .. }
        )
}
