use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::rc::Rc;

use crate::ast::*;
use crate::types::{dim_div, dim_is_dimensionless, dim_mul, dim_pow, dim_to_string, resolve_unit_expr, Dimension};

#[derive(Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Char(char),
    String(String),
    Quantity(f64, Dimension, String),
    // Las listas tienen identidad de referencia porque sus operaciones
    // mutables (`push`/`remove_at`) deben ser visibles desde aliases del
    // mismo binding, igual que Map y Set.
    List(Rc<RefCell<Vec<Value>>>),
    Closure(Rc<Vec<String>>, Rc<Block>, Env),
    Record(String, Rc<RefCell<Vec<(String, Value)>>>),
    EnumInstance(String, String, HashMap<String, Value>, Vec<Type>),
    Task(Rc<RefCell<Value>>),
    Channel(Rc<RefCell<ChannelState>>),
    Map(Rc<RefCell<Vec<(Value, Value)>>>),
    Set(Rc<RefCell<Vec<Value>>>),
    Void,
}

struct RuntimeImpl {
    generics: Vec<GenericParam>,
    trait_name: Option<String>,
    type_args: Vec<Type>,
    methods: Vec<Rc<FunctionDecl>>,
}

pub struct ChannelState {
    queue: VecDeque<Value>,
    closed: bool,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{n}"),
            Value::Float(n) => write!(f, "{n}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Char(c) => write!(f, "{c}"),
            Value::String(s) => write!(f, "{s}"),
            Value::Quantity(v, _, unit) => write!(f, "{v} {unit}"),
            Value::List(state) => {
                write!(f, "[")?;
                for (i, item) in state.borrow().iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{item}")?;
                }
                write!(f, "]")
            }
            Value::Closure(..) => write!(f, "<function>"),
            Value::Record(name, data) => {
                write!(f, "{name} {{ ")?;
                for (i, (k, v)) in data.borrow().iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, " }}")
            }
            Value::EnumInstance(_, variant, fields, _) => {
                if fields.is_empty() {
                    write!(f, "{variant}")
                } else if let Some(positional) = positional_fields(fields) {
                    write!(f, "{variant}(")?;
                    for (i, v) in positional.iter().enumerate() {
                        if i > 0 { write!(f, ", ")?; }
                        write!(f, "{v}")?;
                    }
                    write!(f, ")")
                } else {
                    write!(f, "{variant}(")?;
                    for (i, (k, v)) in fields.iter().enumerate() {
                        if i > 0 { write!(f, ", ")?; }
                        write!(f, "{k}: {v}")?;
                    }
                    write!(f, ")")
                }
            }
            Value::Task(result) => write!(f, "Task({})", result.borrow()),
            Value::Channel(state) => write!(f, "Channel({} pending)", state.borrow().queue.len()),
            Value::Map(state) => {
                write!(f, "[")?;
                for (i, (k, v)) in state.borrow().iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "]")
            }
            Value::Set(state) => {
                write!(f, "{{")?;
                for (i, v) in state.borrow().iter().enumerate() {
                    if i > 0 { write!(f, ", ")?; }
                    write!(f, "{v}")?;
                }
                write!(f, "}}")
            }
            Value::Void => write!(f, ""),
        }
    }
}

fn fields_get<'a>(data: &'a [(String, Value)], key: &str) -> Option<&'a Value> {
    data.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn fields_set(data: &mut Vec<(String, Value)>, key: &str, value: Value) {
    match data.iter_mut().find(|(k, _)| k == key) {
        Some(entry) => entry.1 = value,
        None => data.push((key.to_string(), value)),
    }
}

fn positional_fields(fields: &HashMap<String, Value>) -> Option<Vec<&Value>> {
    let mut indexed: Vec<(usize, &Value)> = Vec::with_capacity(fields.len());
    for (k, v) in fields {
        indexed.push((k.parse::<usize>().ok()?, v));
    }
    indexed.sort_by_key(|(i, _)| *i);
    if indexed.iter().enumerate().all(|(pos, (i, _))| pos == *i) {
        Some(indexed.into_iter().map(|(_, v)| v).collect())
    } else {
        None
    }
}

pub enum RuntimeError {
    Error(String),
    Return(Value),
    Break(Option<Value>),
    Continue,
}

pub type EvalResult = Result<Value, RuntimeError>;

struct Frame {
    vars: HashMap<String, Value>,
    parent: Option<Env>,
}

#[derive(Clone)]
pub struct Env(Rc<RefCell<Frame>>);

impl Env {
    fn root() -> Self {
        Env(Rc::new(RefCell::new(Frame { vars: HashMap::new(), parent: None })))
    }

    fn child(&self) -> Self {
        Env(Rc::new(RefCell::new(Frame { vars: HashMap::new(), parent: Some(self.clone()) })))
    }

    fn define(&self, name: &str, value: Value) {
        self.0.borrow_mut().vars.insert(name.to_string(), value);
    }

    fn get(&self, name: &str) -> Option<Value> {
        if let Some(v) = self.0.borrow().vars.get(name) {
            return Some(v.clone());
        }
        self.0.borrow().parent.as_ref().and_then(|p| p.get(name))
    }

    /// Reasigna 'name' en el frame donde ya existe (subiendo por los padres);
    /// si no existe en ninguno, lo define en el frame actual (nuevo binding
    /// inmutable implícito, documento 01 §1.1 / documento 10 §5.1).
    fn assign(&self, name: &str, value: Value) {
        if self.0.borrow().vars.contains_key(name) {
            self.0.borrow_mut().vars.insert(name.to_string(), value);
            return;
        }
        let parent = self.0.borrow().parent.clone();
        match parent {
            Some(p) if p.contains(name) => p.assign(name, value),
            _ => self.define(name, value),
        }
    }

    fn contains(&self, name: &str) -> bool {
        self.0.borrow().vars.contains_key(name) || self.0.borrow().parent.as_ref().is_some_and(|p| p.contains(name))
    }
}

pub struct Interpreter {
    functions: HashMap<String, Rc<FunctionDecl>>,
    records: HashMap<String, RecordDecl>,
    enums: HashMap<String, EnumDecl>,
    variant_to_enum: HashMap<String, String>,
    impls: HashMap<String, Vec<RuntimeImpl>>,
    traits: HashMap<String, TraitDecl>,
    derives: HashMap<String, Vec<String>>,
    runtime_record_type_args: HashMap<usize, Vec<Type>>,
    moved: HashSet<usize>,
}

impl Interpreter {
    pub fn new(items: &[Item]) -> Self {
        let mut functions = HashMap::new();
        let mut records = HashMap::new();
        let mut enums = HashMap::new();
        let mut variant_to_enum = HashMap::new();
        let mut impls: HashMap<String, Vec<RuntimeImpl>> = HashMap::new();
        let mut traits: HashMap<String, TraitDecl> = HashMap::new();
        let mut derives: HashMap<String, Vec<String>> = HashMap::new();
        for item in items {
            match item {
                Item::Function(f) => { functions.insert(f.name.clone(), Rc::new(f.clone())); }
                Item::Record(r) => {
                    derives.insert(r.name.clone(), r.derives.clone());
                    records.insert(r.name.clone(), r.clone());
                }
                Item::Enum(e) => {
                    for v in &e.variants {
                        variant_to_enum.insert(v.name.clone(), e.name.clone());
                    }
                    derives.insert(e.name.clone(), e.derives.clone());
                    enums.insert(e.name.clone(), e.clone());
                }
                Item::Impl(im) => {
                    let entry = impls.entry(im.type_name.clone()).or_default();
                    entry.push(RuntimeImpl {
                        generics: im.generics.clone(),
                        trait_name: im.trait_name.clone(),
                        type_args: im.type_args.clone(),
                        methods: im.methods.iter().map(|method| Rc::new(method.clone())).collect(),
                    });
                }
                // Los 'import' ya se resolvieron y desazucararon al fusionar
                // los módulos (compiler/src/modules.rs) antes de llegar aquí.
                Item::Import(_) => {}
                // Se conservan para que find_method pueda ejecutar cuerpos por
                // defecto cuando un impl no proporciona una sobrescritura.
                Item::Trait(trait_decl) => {
                    traits.insert(trait_decl.name.clone(), trait_decl.clone());
                }
            }
        }
        // Option/Result son parte del núcleo del lenguaje (documento 04) y no
        // necesitan que el programa los declare — se registran aquí como si
        // fueran de la stdlib, salvo que el propio programa ya los redefina.
        for (variant, owner) in [
            ("Some", "Option"), ("None", "Option"),
            ("Ok", "Result"), ("Err", "Result"),
            ("Less", "Ordering"), ("Equal", "Ordering"), ("Greater", "Ordering"),
        ] {
            variant_to_enum.entry(variant.to_string()).or_insert_with(|| owner.to_string());
        }
        Interpreter {
            functions,
            records,
            enums,
            variant_to_enum,
            impls,
            traits,
            derives,
            runtime_record_type_args: HashMap::new(),
            moved: HashSet::new(),
        }
    }

    fn has_derive(&self, type_name: &str, trait_name: &str) -> bool {
        self.derives.get(type_name).is_some_and(|d| d.iter().any(|t| t == trait_name))
    }

    /// Genera 'equals' campo por campo, en el orden de declaración del
    /// 'record' (documento 12, §2.1: 'derive(Eq)') — para 'enum', compara
    /// primero la variante y luego sus campos.
    fn derived_equals(&mut self, lv: &Value, rv: &Value, env: &Env) -> Result<bool, RuntimeError> {
        match (lv, rv) {
            (Value::Record(tn1, d1), Value::Record(tn2, d2)) if tn1 == tn2 => {
                let Some(decl) = self.records.get(tn1).cloned() else { return Ok(false) };
                for field in &decl.fields {
                    let a = fields_get(&d1.borrow(), &field.name).cloned();
                    let b = fields_get(&d2.borrow(), &field.name).cloned();
                    match (a, b) {
                        (Some(a), Some(b)) => { if !truthy(&self.eval_binary(BinOp::Eq, a, b, env)?) { return Ok(false); } }
                        _ => return Ok(false),
                    }
                }
                Ok(true)
            }
            (Value::EnumInstance(tn1, v1, f1, _), Value::EnumInstance(tn2, v2, f2, _)) if tn1 == tn2 => {
                if v1 != v2 { return Ok(false); }
                for (k, a) in f1 {
                    let Some(b) = f2.get(k) else { return Ok(false) };
                    if !truthy(&self.eval_binary(BinOp::Eq, a.clone(), b.clone(), env)?) { return Ok(false); }
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// Genera 'compare' para 'record' con 'derive(Ord)': orden lexicográfico
    /// por campo, en el orden de declaración (documento 12, §2.2). Los
    /// 'enum' con 'derive(Ord)' distintos de 'Ordering' no están cubiertos
    /// todavía (necesitarían el orden de declaración de variantes).
    fn compare_values(&mut self, a: &Value, b: &Value, env: &Env) -> Result<i32, RuntimeError> {
        if matches!(a, Value::Record(..) | Value::EnumInstance(..)) {
            let type_name = value_type_name(a);
            if let Some(f) = self.find_method_for_value(a, "compare") {
                let ordering = self.call_user_function(&f, vec![a.clone(), b.clone()], env.clone())?;
                return Ok(ordering_to_i32(&ordering));
            }
            if self.has_derive(&type_name, "Ord") {
                if let (Value::Record(tn, d1), Value::Record(_, d2)) = (a, b) {
                    let Some(decl) = self.records.get(tn).cloned() else {
                        return Err(RuntimeError::Error(format!("'{type_name}' has no field declaration to compare")));
                    };
                    for field in &decl.fields {
                        let fa = fields_get(&d1.borrow(), &field.name).cloned();
                        let fb = fields_get(&d2.borrow(), &field.name).cloned();
                        if let (Some(fa), Some(fb)) = (fa, fb) {
                            let c = self.compare_values(&fa, &fb, env)?;
                            if c != 0 { return Ok(c); }
                        }
                    }
                    return Ok(0);
                }
            }
            return Err(RuntimeError::Error(format!("'{type_name}' does not implement 'Ord'")));
        }
        compare(a, b)
    }

    /// Puntero identidad de un Record, usado para el seguimiento de "movido"
    /// tras enviarlo por un canal (documento 09, §2.1 — error E1101). Es la
    /// única forma de seguimiento de propiedad de todo el lenguaje, y solo
    /// aplica a valores con campos 'mut' (los únicos con identidad real,
    /// documento 11).
    fn record_ptr(v: &Value) -> Option<usize> {
        match v {
            Value::Record(_, data) => Some(Rc::as_ptr(data) as usize),
            _ => None,
        }
    }

    /// Despacha un operador binario a través de 'impl Trait for Type' cuando
    /// el operando izquierdo es un tipo definido por el usuario (documento 03,
    /// §4: los operadores no son un caso especial, se habilitan implementando
    /// el trait estándar correspondiente). Los tipos incorporados (Int, Float,
    /// Quantity, String, Bool) siguen resolviéndose por la vía rápida existente.
    fn eval_binary(&mut self, op: BinOp, lv: Value, rv: Value, env: &Env) -> EvalResult {
        if matches!(lv, Value::Record(..) | Value::EnumInstance(..)) {
            let type_name = value_type_name(&lv);
            if let Some(method) = operator_method_name(op) {
                if let Some(f) = self.find_method_for_value(&lv, method) {
                    return self.call_user_function(&f, vec![lv, rv], env.clone());
                }
                if op == BinOp::Eq && self.has_derive(&type_name, "Eq") {
                    return Ok(Value::Bool(self.derived_equals(&lv, &rv, env)?));
                }
            }
            if matches!(op, BinOp::Lt | BinOp::Gt | BinOp::LtEq | BinOp::GtEq) {
                if let Some(f) = self.find_method_for_value(&lv, "compare") {
                    let ordering = self.call_user_function(&f, vec![lv, rv], env.clone())?;
                    return ordering_matches(op, &ordering).map(Value::Bool);
                }
                if self.has_derive(&type_name, "Ord") {
                    let c = self.compare_values(&lv, &rv, env)?;
                    return Ok(Value::Bool(match op {
                        BinOp::Lt => c < 0,
                        BinOp::Gt => c > 0,
                        BinOp::LtEq => c <= 0,
                        BinOp::GtEq => c >= 0,
                        _ => unreachable!(),
                    }));
                }
            }
            if op == BinOp::NotEq {
                if let Some(f) = self.find_method_for_value(&lv, "equals") {
                    let eq = self.call_user_function(&f, vec![lv, rv], env.clone())?;
                    return Ok(Value::Bool(!truthy(&eq)));
                }
                if self.has_derive(&type_name, "Eq") {
                    return Ok(Value::Bool(!self.derived_equals(&lv, &rv, env)?));
                }
            }
            return Err(RuntimeError::Error(format!(
                "'{type_name}' does not implement the trait needed for this operator"
            )));
        }
        eval_binary_builtin(op, lv, rv)
    }

    fn call_callable(&mut self, f: Value, args: Vec<Value>, _env: &Env) -> EvalResult {
        match f {
            Value::Closure(params, body, captured) => {
                let call_env = captured.child();
                for (p, a) in params.iter().zip(args.into_iter()) {
                    call_env.define(p, a);
                }
                match self.eval_block(&body, &call_env) {
                    Ok(v) => Ok(v),
                    Err(RuntimeError::Return(v)) => Ok(v),
                    other => other,
                }
            }
            other => Err(RuntimeError::Error(format!("'{other}' is not callable"))),
        }
    }

    fn call_method_on_value(&mut self, receiver: Value, method: &str, args: Vec<Value>, env: &Env) -> EvalResult {
        let type_name = value_type_name(&receiver);
        let Some(f) = self.find_method_for_value(&receiver, method) else {
            return Err(RuntimeError::Error(format!("no method '{method}' for type '{type_name}'")));
        };
        let mut values = vec![receiver];
        values.extend(args);
        self.call_user_function(&f, values, env.clone())
    }

    fn find_method_for_value(&self, receiver: &Value, method: &str) -> Option<Rc<FunctionDecl>> {
        let type_name = value_type_name(receiver);
        let actual_type_args = self.runtime_type_args(receiver);
        let implementations = self.impls.get(&type_name)?;
        for implementation in implementations {
            if !applied_type_args_match(&actual_type_args, implementation) {
                continue;
            }
            if let Some(explicit) = implementation.methods.iter().find(|f| f.name == method) {
                return Some(explicit.clone());
            }
        }
        for implementation in implementations {
            if !applied_type_args_match(&actual_type_args, implementation) {
                continue;
            }
            let Some(trait_name) = &implementation.trait_name else { continue };
            let Some(trait_decl) = self.traits.get(trait_name) else { continue };
            let Some(default) = trait_decl.methods.iter().find(|candidate| candidate.name == method) else { continue };
            let Some(body) = &default.default_body else { continue };
            return Some(Rc::new(FunctionDecl {
                name: default.name.clone(),
                is_pub: false,
                generics: default.generics.clone(),
                params: default.params.clone(),
                return_type: default.return_type.clone(),
                body: body.clone(),
            }));
        }
        None
    }

    fn runtime_type_args(&self, value: &Value) -> Vec<Type> {
        match value {
            Value::Record(_, data) => self
                .runtime_record_type_args
                .get(&(Rc::as_ptr(data) as usize))
                .cloned()
                .unwrap_or_default(),
            Value::EnumInstance(_, _, _, type_args) => type_args.clone(),
            Value::Quantity(_, dimension, _) => vec![dimension_to_type(dimension)],
            _ => Vec::new(),
        }
    }

    fn infer_record_type_args(&self, name: &str, fields: &[(String, Value)]) -> Vec<Type> {
        let Some(decl) = self.records.get(name) else { return Vec::new() };
        if decl.generics.is_empty() {
            return Vec::new();
        }
        let generic_names: HashSet<String> = decl.generics.iter().map(|generic| generic.name.clone()).collect();
        let mut substitutions = HashMap::new();
        for field in &decl.fields {
            let Some((_, value)) = fields.iter().find(|(field_name, _)| field_name == &field.name) else { continue };
            self.infer_runtime_type(&field.ty, value, &generic_names, &mut substitutions);
        }
        decl.generics
            .iter()
            .map(|generic| substitutions.get(&generic.name).cloned())
            .collect::<Option<Vec<_>>>()
            .unwrap_or_default()
    }

    fn infer_enum_type_args(
        &self,
        enum_name: &str,
        variant_name: &str,
        fields: &HashMap<String, Value>,
    ) -> Vec<Type> {
        let Some(decl) = self.enums.get(enum_name) else { return Vec::new() };
        if decl.generics.is_empty() {
            return Vec::new();
        }
        let Some(variant) = decl.variants.iter().find(|variant| variant.name == variant_name) else {
            return Vec::new();
        };
        let generic_names: HashSet<String> = decl.generics.iter().map(|generic| generic.name.clone()).collect();
        let mut substitutions = HashMap::new();
        for (index, field) in variant.fields.iter().enumerate() {
            let key = field.name.clone().unwrap_or_else(|| index.to_string());
            let Some(value) = fields.get(&key) else { continue };
            let actual = self.runtime_type_of_value(value);
            type_pattern_matches(&field.ty, &actual, &generic_names, &mut substitutions);
        }
        decl.generics
            .iter()
            .map(|generic| substitutions.get(&generic.name).cloned())
            .collect::<Option<Vec<_>>>()
            .unwrap_or_default()
    }

    fn infer_runtime_type(
        &self,
        pattern: &Type,
        value: &Value,
        generic_names: &HashSet<String>,
        substitutions: &mut HashMap<String, Type>,
    ) {
        let actual = self.runtime_type_of_value(value);
        type_pattern_matches(pattern, &actual, generic_names, substitutions);
    }

    fn runtime_type_of_value(&self, value: &Value) -> Type {
        match value {
            Value::List(state) => {
                let element = state
                    .borrow()
                    .first()
                    .map(|value| self.runtime_type_of_value(value))
                    .unwrap_or_else(|| Type::Named("Unknown".to_string(), Vec::new()));
                Type::Named("List".to_string(), vec![element])
            }
            Value::Map(state) => {
                let (key, value) = state
                    .borrow()
                    .first()
                    .map(|(key, value)| (self.runtime_type_of_value(key), self.runtime_type_of_value(value)))
                    .unwrap_or_else(|| {
                        (
                            Type::Named("Unknown".to_string(), Vec::new()),
                            Type::Named("Unknown".to_string(), Vec::new()),
                        )
                    });
                Type::Named("Map".to_string(), vec![key, value])
            }
            Value::Set(state) => {
                let element = state
                    .borrow()
                    .first()
                    .map(|value| self.runtime_type_of_value(value))
                    .unwrap_or_else(|| Type::Named("Unknown".to_string(), Vec::new()));
                Type::Named("Set".to_string(), vec![element])
            }
            Value::Record(name, _) => Type::Named(name.clone(), self.runtime_type_args(value)),
            Value::EnumInstance(name, _, _, type_args) => Type::Named(name.clone(), type_args.clone()),
            Value::Int(_) => Type::Named("Int".to_string(), Vec::new()),
            Value::Float(_) => Type::Named("Float".to_string(), Vec::new()),
            Value::Bool(_) => Type::Named("Bool".to_string(), Vec::new()),
            Value::Char(_) => Type::Named("Char".to_string(), Vec::new()),
            Value::String(_) => Type::Named("String".to_string(), Vec::new()),
            Value::Quantity(..) => Type::Named("Quantity".to_string(), self.runtime_type_args(value)),
            Value::Closure(..) => Type::Named("Function".to_string(), Vec::new()),
            Value::Task(_) => Type::Named("Task".to_string(), Vec::new()),
            Value::Channel(_) => Type::Named("Channel".to_string(), Vec::new()),
            Value::Void => Type::Named("Void".to_string(), Vec::new()),
        }
    }

    pub fn run_main(&mut self) -> Result<Value, String> {
        let Some(main_fn) = self.functions.get("main").cloned() else {
            return Err("no 'main' function found".to_string());
        };
        let env = Env::root();
        match self.call_user_function(&main_fn, vec![], env) {
            Ok(v) => Ok(v),
            Err(RuntimeError::Error(msg)) => Err(msg),
            Err(RuntimeError::Return(v)) => Ok(v),
            Err(RuntimeError::Break(_)) => Err("'break' outside a loop".to_string()),
            Err(RuntimeError::Continue) => Err("'continue' outside a loop".to_string()),
        }
    }

    fn call_user_function(&mut self, f: &FunctionDecl, args: Vec<Value>, closure_env: Env) -> EvalResult {
        let call_env = closure_env.child();
        for (param, arg) in f.params.iter().zip(args.into_iter()) {
            call_env.define(&param.name, arg);
        }
        match self.eval_block(&f.body, &call_env) {
            Ok(v) => Ok(v),
            Err(RuntimeError::Return(v)) => Ok(v),
            other => other,
        }
    }

    fn eval_block(&mut self, block: &Block, env: &Env) -> EvalResult {
        let inner = env.child();
        for stmt in &block.stmts {
            self.eval_stmt(stmt, &inner)?;
        }
        match &block.tail {
            Some(e) => self.eval_expr(e, &inner),
            None => Ok(Value::Void),
        }
    }

    fn eval_stmt(&mut self, stmt: &Stmt, env: &Env) -> EvalResult {
        match stmt {
            Stmt::Binding { name, value, .. } => {
                let v = self.eval_expr(value, env)?;
                env.define(name, v);
                Ok(Value::Void)
            }
            Stmt::Assign { name, value } => {
                let v = self.eval_expr(value, env)?;
                env.assign(name, v);
                Ok(Value::Void)
            }
            Stmt::Return(e) => {
                let v = match e { Some(e) => self.eval_expr(e, env)?, None => Value::Void };
                Err(RuntimeError::Return(v))
            }
            Stmt::Break(e) => {
                let v = match e { Some(e) => Some(self.eval_expr(e, env)?), None => None };
                Err(RuntimeError::Break(v))
            }
            Stmt::Continue => Err(RuntimeError::Continue),
            Stmt::For { pattern, iter, body } => {
                if matches!(iter, Expr::Range(..)) {
                    let items = self.eval_iterable(iter, env)?;
                    for item in items {
                        let loop_env = env.child();
                        loop_env.define(pattern, item);
                        match self.eval_block(body, &loop_env) {
                            Ok(_) => {}
                            Err(RuntimeError::Break(_)) => break,
                            Err(RuntimeError::Continue) => continue,
                            Err(other) => return Err(other),
                        }
                    }
                    return Ok(Value::Void);
                }
                let source = self.eval_expr(iter, env)?;
                match source {
                    Value::Channel(state) => loop {
                        let popped = state.borrow_mut().queue.pop_front();
                        match popped {
                            Some(item) => {
                                let loop_env = env.child();
                                loop_env.define(pattern, item);
                                match self.eval_block(body, &loop_env) {
                                    Ok(_) => {}
                                    Err(RuntimeError::Break(_)) => break,
                                    Err(RuntimeError::Continue) => continue,
                                    Err(other) => return Err(other),
                                }
                            }
                            None => break,
                        }
                    },
                    Value::List(state) => {
                        for item in state.borrow().clone() {
                            let loop_env = env.child();
                            loop_env.define(pattern, item);
                            match self.eval_block(body, &loop_env) {
                                Ok(_) => {}
                                Err(RuntimeError::Break(_)) => break,
                                Err(RuntimeError::Continue) => continue,
                                Err(other) => return Err(other),
                            }
                        }
                    }
                    // Protocolo Iterator (documento 06 §3): se llama '.next()'
                    // repetidamente sobre la propia fuente hasta que devuelva
                    // 'None'. Es perezoso (no colecciona nada por adelantado),
                    // a diferencia del caso List/Range de arriba.
                    other if self.find_method_for_value(&other, "next").is_some() => loop {
                        match self.call_method_on_value(other.clone(), "next", vec![], env)? {
                            Value::EnumInstance(_, variant, fields, _) if variant == "Some" => {
                                let item = fields.get("0").cloned().unwrap_or(Value::Void);
                                let loop_env = env.child();
                                loop_env.define(pattern, item);
                                match self.eval_block(body, &loop_env) {
                                    Ok(_) => {}
                                    Err(RuntimeError::Break(_)) => break,
                                    Err(RuntimeError::Continue) => continue,
                                    Err(other) => return Err(other),
                                }
                            }
                            Value::EnumInstance(_, variant, _, _) if variant == "None" => break,
                            other => {
                                return Err(RuntimeError::Error(format!(
                                    "'next()' must return Option, got '{other}'"
                                )));
                            }
                        }
                    },
                    other => {
                        return Err(RuntimeError::Error(format!("'{other}' is not iterable")));
                    }
                }
                Ok(Value::Void)
            }
            Stmt::While { cond, body } => {
                loop {
                    let c = self.eval_expr(cond, env)?;
                    if !truthy(&c) { break; }
                    match self.eval_block(body, env) {
                        Ok(_) => {}
                        Err(RuntimeError::Break(_)) => break,
                        Err(RuntimeError::Continue) => continue,
                        Err(other) => return Err(other),
                    }
                }
                Ok(Value::Void)
            }
            Stmt::FieldAssign { target, value } => {
                let v = self.eval_expr(value, env)?;
                match target {
                    Expr::FieldAccess(obj, field) => match self.eval_expr(obj, env)? {
                        Value::Record(_, data) => {
                            fields_set(&mut data.borrow_mut(), field, v);
                            Ok(Value::Void)
                        }
                        other => Err(RuntimeError::Error(format!("cannot assign a field on '{other}'"))),
                    },
                    _ => Err(RuntimeError::Error("invalid assignment target".to_string())),
                }
            }
            Stmt::Expr(e) => self.eval_expr(e, env),
        }
    }

    fn eval_iterable(&mut self, expr: &Expr, env: &Env) -> Result<Vec<Value>, RuntimeError> {
        match expr {
            Expr::Range(start, kind, end, step) => {
                let sv = self.eval_expr(start, env)?;
                let ev = self.eval_expr(end, env)?;
                let step_v = match step {
                    Some(s) => as_i64(&self.eval_expr(s, env)?)?,
                    None => 1,
                };
                let (s, e) = (as_i64(&sv)?, as_i64(&ev)?);
                let mut values = Vec::new();
                if step_v > 0 {
                    let mut i = s;
                    while (*kind == RangeKind::To && i <= e) || (*kind == RangeKind::Until && i < e) {
                        values.push(Value::Int(i));
                        i += step_v;
                    }
                } else if step_v < 0 {
                    let mut i = s;
                    while (*kind == RangeKind::To && i >= e) || (*kind == RangeKind::Until && i > e) {
                        values.push(Value::Int(i));
                        i += step_v;
                    }
                }
                Ok(values)
            }
            other => match self.eval_expr(other, env)? {
                Value::List(state) => Ok(state.borrow().clone()),
                v => Err(RuntimeError::Error(format!("'{v}' is not iterable"))),
            },
        }
    }

    fn eval_expr(&mut self, expr: &Expr, env: &Env) -> EvalResult {
        match expr {
            Expr::IntLiteral(n) => Ok(Value::Int(*n)),
            Expr::FloatLiteral(n) => Ok(Value::Float(*n)),
            Expr::StringLiteral(s) => Ok(Value::String(s.clone())),
            Expr::CharLiteral(c) => Ok(Value::Char(*c)),
            Expr::BoolLiteral(b) => Ok(Value::Bool(*b)),
            Expr::UnitLiteral(num, unit) => {
                let n = as_f64(&self.eval_expr(num, env)?)?;
                let dim = resolve_unit_expr(unit).map_err(|u| RuntimeError::Error(format!("unknown unit '{u}'")))?;
                Ok(Value::Quantity(n, dim, unit.clone()))
            }
            Expr::Ident(name) => {
                if let Some(v) = env.get(name) {
                    if let Some(ptr) = Self::record_ptr(&v) {
                        if self.moved.contains(&ptr) {
                            return Err(RuntimeError::Error(format!(
                                "'{name}' was moved into a channel send earlier and cannot be used afterwards."
                            )));
                        }
                    }
                    return Ok(v);
                }
                if let Some(enum_name) = self.variant_to_enum.get(name).cloned() {
                    return Ok(Value::EnumInstance(enum_name, name.clone(), HashMap::new(), Vec::new()));
                }
                Err(RuntimeError::Error(format!("undefined name '{name}'")))
            }
            Expr::Unary(op, e) => {
                let v = self.eval_expr(e, env)?;
                match (op, &v) {
                    (UnaryOp::Neg, Value::Int(n)) => Ok(Value::Int(-n)),
                    (UnaryOp::Neg, Value::Float(n)) => Ok(Value::Float(-n)),
                    (UnaryOp::Neg, Value::Quantity(n, d, u)) => Ok(Value::Quantity(-n, d.clone(), u.clone())),
                    (UnaryOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
                    _ => Err(RuntimeError::Error(format!("cannot apply unary operator to '{v}'"))),
                }
            }
            Expr::Binary(op, l, r) => {
                let lv = self.eval_expr(l, env)?;
                let rv = self.eval_expr(r, env)?;
                self.eval_binary(*op, lv, rv, env)
            }
            Expr::Range(start, _, end, _) => {
                let s = self.eval_expr(start, env)?;
                let e = self.eval_expr(end, env)?;
                Ok(Value::String(format!("Range({s}, {e})")))
            }
            Expr::Call(callee, args) => self.eval_call(callee, args, env, None),
            Expr::GenericCall(callee, type_args, args) => self.eval_call(callee, args, env, Some(type_args)),
            Expr::FieldAccess(obj, field) => self.eval_method(obj, field, &[], env),
            Expr::Index(obj, idx) => {
                let ov = self.eval_expr(obj, env)?;
                let iv = as_i64(&self.eval_expr(idx, env)?)?;
                match ov {
                    Value::List(state) => state
                        .borrow()
                        .get(iv as usize)
                        .cloned()
                        .ok_or_else(|| RuntimeError::Error(format!("index out of bounds: {iv}"))),
                    other => Err(RuntimeError::Error(format!("cannot index '{other}'"))),
                }
            }
            Expr::If(cond, then_block, else_block) => {
                let c = self.eval_expr(cond, env)?;
                if truthy(&c) {
                    self.eval_block(then_block, env)
                } else {
                    match else_block {
                        Some(b) => self.eval_block(b, env),
                        None => Ok(Value::Void),
                    }
                }
            }
            Expr::Block(b) => self.eval_block(b, env),
            Expr::Lambda(params, body) => {
                Ok(Value::Closure(Rc::new(params.clone()), Rc::new(body.clone()), env.clone()))
            }
            Expr::ListLiteral(items) => {
                let mut values = Vec::with_capacity(items.len());
                for it in items { values.push(self.eval_expr(it, env)?); }
                Ok(Value::List(Rc::new(RefCell::new(values))))
            }
            Expr::SetLiteral(items) => {
                let mut values: Vec<Value> = Vec::with_capacity(items.len());
                for it in items {
                    let v = self.eval_expr(it, env)?;
                    let mut dup = false;
                    for existing in &values {
                        if truthy(&self.eval_binary(BinOp::Eq, existing.clone(), v.clone(), env)?) { dup = true; break; }
                    }
                    if !dup { values.push(v); }
                }
                Ok(Value::Set(Rc::new(RefCell::new(values))))
            }
            Expr::MapLiteral(pairs) => {
                let mut values: Vec<(Value, Value)> = Vec::with_capacity(pairs.len());
                for (k, v) in pairs {
                    let kv = self.eval_expr(k, env)?;
                    let vv = self.eval_expr(v, env)?;
                    let mut replaced = false;
                    for entry in values.iter_mut() {
                        if truthy(&self.eval_binary(BinOp::Eq, entry.0.clone(), kv.clone(), env)?) {
                            entry.1 = vv.clone();
                            replaced = true;
                            break;
                        }
                    }
                    if !replaced { values.push((kv, vv)); }
                }
                Ok(Value::Map(Rc::new(RefCell::new(values))))
            }
            Expr::Try(inner, _catch) => self.eval_expr(inner, env),
            Expr::Within(a, r) => {
                let av = self.eval_expr(a, env)?;
                if let Expr::Range(start, kind, end, _) = r.as_ref() {
                    let sv = self.eval_expr(start, env)?;
                    let ev = self.eval_expr(end, env)?;
                    let a_f = as_f64(&av)?;
                    let s_f = as_f64(&sv)?;
                    let e_f = as_f64(&ev)?;
                    let inside = match kind {
                        RangeKind::To => a_f >= s_f && a_f <= e_f,
                        RangeKind::Until => a_f >= s_f && a_f < e_f,
                    };
                    Ok(Value::Bool(inside))
                } else {
                    Err(RuntimeError::Error("'within' expects a range on the right-hand side".to_string()))
                }
            }
            Expr::Approximately(a, b, tol) => {
                let av = as_f64(&self.eval_expr(a, env)?)?;
                let bv = as_f64(&self.eval_expr(b, env)?)?;
                let tv = as_f64(&self.eval_expr(tol, env)?)?;
                Ok(Value::Bool((av - bv).abs() <= tv))
            }
            Expr::As(e, unit_expr) => {
                let v = as_f64(&self.eval_expr(e, env)?)?;
                if let Expr::Ident(sym) = unit_expr.as_ref() {
                    let dim = resolve_unit_expr(sym).map_err(|u| RuntimeError::Error(format!("unknown unit '{u}'")))?;
                    Ok(Value::Quantity(v, dim, sym.clone()))
                } else {
                    Err(RuntimeError::Error("'as' expects a unit identifier".to_string()))
                }
            }
            Expr::Loop(block) => loop {
                match self.eval_block(block, env) {
                    Ok(_) => {}
                    Err(RuntimeError::Break(v)) => return Ok(v.unwrap_or(Value::Void)),
                    Err(RuntimeError::Continue) => continue,
                    Err(other) => return Err(other),
                }
            },
            Expr::RecordLiteral(name, fields) => {
                let mut data: Vec<(String, Value)> = Vec::new();
                if let Some(decl) = self.records.get(name).cloned() {
                    for field in &decl.fields {
                        if let Some(default) = &field.default {
                            let v = self.eval_expr(default, &Env::root())?;
                            data.push((field.name.clone(), v));
                        }
                    }
                }
                for (fname, fexpr) in fields {
                    let v = self.eval_expr(fexpr, env)?;
                    fields_set(&mut data, fname, v);
                }
                let type_args = self.infer_record_type_args(name, &data);
                let storage = Rc::new(RefCell::new(data));
                self.runtime_record_type_args
                    .insert(Rc::as_ptr(&storage) as usize, type_args);
                Ok(Value::Record(name.clone(), storage))
            }
            Expr::GenericRecordLiteral(name, type_args, fields) => {
                let mut data: Vec<(String, Value)> = Vec::new();
                if let Some(decl) = self.records.get(name).cloned() {
                    for field in &decl.fields {
                        if let Some(default) = &field.default {
                            let v = self.eval_expr(default, &Env::root())?;
                            data.push((field.name.clone(), v));
                        }
                    }
                }
                for (fname, fexpr) in fields {
                    let v = self.eval_expr(fexpr, env)?;
                    fields_set(&mut data, fname, v);
                }
                let storage = Rc::new(RefCell::new(data));
                self.runtime_record_type_args
                    .insert(Rc::as_ptr(&storage) as usize, type_args.clone());
                Ok(Value::Record(name.clone(), storage))
            }
            Expr::Match(scrutinee, arms) => {
                let v = self.eval_expr(scrutinee, env)?;
                for arm in arms {
                    let arm_env = env.child();
                    if self.try_match(&arm.pattern, &v, &arm_env)? {
                        if let Some(guard) = &arm.guard {
                            let g = self.eval_expr(guard, &arm_env)?;
                            if !truthy(&g) { continue; }
                        }
                        return self.eval_block(&arm.body, &arm_env);
                    }
                }
                Err(RuntimeError::Error("no 'match' arm matched the value".to_string()))
            }
            // 'spawn' se ejecuta de forma síncrona e inmediata (documento 10 §7:
            // simulación de una tarea, sin hilos de SO reales todavía) — lo que
            // sí se verifica de verdad son las reglas de seguridad del diseño
            // (E1100 en el verificador de tipos; movido-tras-enviar aquí abajo).
            Expr::Spawn(block) => {
                let result = self.eval_block(block, env)?;
                Ok(Value::Task(Rc::new(RefCell::new(result))))
            }
            Expr::SpawnScope(block) => self.eval_block(block, env),
            Expr::Channel(_, _capacity) => {
                Ok(Value::Channel(Rc::new(RefCell::new(ChannelState { queue: VecDeque::new(), closed: false }))))
            }
        }
    }

    fn try_match(&mut self, pattern: &Pattern, value: &Value, env: &Env) -> Result<bool, RuntimeError> {
        match pattern {
            Pattern::Wildcard => Ok(true),
            Pattern::Literal(lit) => {
                let lit_v = self.eval_expr(lit, env)?;
                Ok(values_equal(&lit_v, value))
            }
            Pattern::Range(start, kind, end) => {
                let s = as_f64(&self.eval_expr(start, env)?)?;
                let e = as_f64(&self.eval_expr(end, env)?)?;
                let v = as_f64(value)?;
                Ok(match kind {
                    RangeKind::To => v >= s && v <= e,
                    RangeKind::Until => v >= s && v < e,
                })
            }
            Pattern::Ident(name) => {
                // Un identificador que nombra una variante conocida es un
                // patrón de constructor, no un binding. Sin esta distinción,
                // `Red => ...` capturaría también `Yellow` y `Green`.
                if self.variant_to_enum.contains_key(name) {
                    return Ok(matches!(
                        value,
                        Value::EnumInstance(_, variant_name, _, _) if variant_name == name
                    ));
                }
                if let Value::EnumInstance(_, variant_name, fields, _) = value {
                    if variant_name == name && fields.is_empty() {
                        return Ok(true);
                    }
                }
                env.define(name, value.clone());
                Ok(true)
            }
            Pattern::Variant(vname, field_pats) => match value {
                Value::EnumInstance(_, variant_name, fields, _) if variant_name == vname => {
                    for (position, (fname, sub)) in field_pats.iter().enumerate() {
                        let Some(fv) = pattern_field_value(fields, fname, position) else { return Ok(false) };
                        if !self.try_match(sub, &fv.clone(), env)? { return Ok(false); }
                    }
                    Ok(true)
                }
                Value::Record(type_name, data) if type_name == vname => {
                    let snapshot: Vec<(String, Value)> = data.borrow().iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                    for (position, (fname, sub)) in field_pats.iter().enumerate() {
                        let field = if let Some(index) = fname.strip_prefix('@').and_then(|index| index.parse::<usize>().ok()) {
                            snapshot.get(index)
                        } else {
                            snapshot.iter().find(|(key, _)| key == fname).or_else(|| snapshot.get(position))
                        };
                        let Some((_, fv)) = field else { return Ok(false) };
                        if !self.try_match(sub, fv, env)? { return Ok(false); }
                    }
                    Ok(true)
                }
                _ => Ok(false),
            },
        }
    }

    fn eval_call(
        &mut self,
        callee: &Expr,
        args: &[Arg],
        env: &Env,
        explicit_type_args: Option<&[Type]>,
    ) -> EvalResult {
        if let Expr::Ident(name) = callee {
            match name.as_str() {
                "print" => {
                    let v = self.eval_arg(&args[0], env)?;
                    println!("{v}");
                    return Ok(Value::Void);
                }
                "sum" => {
                    let v = self.eval_arg(&args[0], env)?;
                    return match v {
                        Value::List(state) => sum_values(&state.borrow()),
                        other => Err(RuntimeError::Error(format!("'sum' expects a List, got '{other}'"))),
                    };
                }
                _ => {}
            }
            if let Some(enum_name) = self.variant_to_enum.get(name).cloned() {
                return self.construct_variant(&enum_name, name, args, env, explicit_type_args);
            }
            if let Some(f) = self.functions.get(name).cloned() {
                let mut values = Vec::with_capacity(args.len());
                for a in args { values.push(self.eval_arg(a, env)?); }
                return self.call_user_function(&f, values, env.clone());
            }
        }
        if let Expr::FieldAccess(obj, method) = callee {
            let receiver = self.eval_expr(obj, env)?;
            if method == "to_string" {
                return Ok(Value::String(receiver.to_string()));
            }
            if method == "length" {
                if let Value::List(state) = &receiver {
                    return Ok(Value::Int(state.borrow().len() as i64));
                }
            }
            if let Value::List(state) = &receiver {
                match method.as_str() {
                    "push" => {
                        let item = self.eval_arg(&args[0], env)?;
                        state.borrow_mut().push(item);
                        return Ok(Value::Void);
                    }
                    "remove_at" => {
                        let index = as_i64(&self.eval_arg(&args[0], env)?)?;
                        let index = usize::try_from(index).map_err(|_| {
                            RuntimeError::Error(format!("index out of bounds: {index}"))
                        })?;
                        let mut items = state.borrow_mut();
                        if index >= items.len() {
                            return Err(RuntimeError::Error(format!("index out of bounds: {index}")));
                        }
                        return Ok(items.remove(index));
                    }
                    "map" => {
                        let f = self.eval_arg(&args[0], env)?;
                        let items = state.borrow().clone();
                        let mut out = Vec::with_capacity(items.len());
                        for it in items { out.push(self.call_callable(f.clone(), vec![it], env)?); }
                        return Ok(Value::List(Rc::new(RefCell::new(out))));
                    }
                    "filter" => {
                        let f = self.eval_arg(&args[0], env)?;
                        let items = state.borrow().clone();
                        let mut out = Vec::new();
                        for it in items {
                            if truthy(&self.call_callable(f.clone(), vec![it.clone()], env)?) { out.push(it); }
                        }
                        return Ok(Value::List(Rc::new(RefCell::new(out))));
                    }
                    "fold" => {
                        let mut acc = self.eval_arg(&args[0], env)?;
                        let f = self.eval_arg(&args[1], env)?;
                        for it in state.borrow().clone() { acc = self.call_callable(f.clone(), vec![acc, it], env)?; }
                        return Ok(acc);
                    }
                    "find" => {
                        let f = self.eval_arg(&args[0], env)?;
                        for it in state.borrow().clone() {
                            if truthy(&self.call_callable(f.clone(), vec![it.clone()], env)?) {
                                return Ok(some_value(it));
                            }
                        }
                        return Ok(none_value());
                    }
                    "any" => {
                        let f = self.eval_arg(&args[0], env)?;
                        for it in state.borrow().clone() {
                            if truthy(&self.call_callable(f.clone(), vec![it], env)?) { return Ok(Value::Bool(true)); }
                        }
                        return Ok(Value::Bool(false));
                    }
                    "all" => {
                        let f = self.eval_arg(&args[0], env)?;
                        for it in state.borrow().clone() {
                            if !truthy(&self.call_callable(f.clone(), vec![it], env)?) { return Ok(Value::Bool(false)); }
                        }
                        return Ok(Value::Bool(true));
                    }
                    "count" => return Ok(Value::Int(state.borrow().len() as i64)),
                    _ => {}
                }
            }
            if let Value::Map(state) = &receiver {
                match method.as_str() {
                    "get" => {
                        let k = self.eval_arg(&args[0], env)?;
                        let snapshot = state.borrow().clone();
                        for (mk, mv) in snapshot {
                            if truthy(&self.eval_binary(BinOp::Eq, mk, k.clone(), env)?) { return Ok(some_value(mv)); }
                        }
                        return Ok(none_value());
                    }
                    "contains_key" => {
                        let k = self.eval_arg(&args[0], env)?;
                        let snapshot = state.borrow().clone();
                        for (mk, _) in snapshot {
                            if truthy(&self.eval_binary(BinOp::Eq, mk, k.clone(), env)?) { return Ok(Value::Bool(true)); }
                        }
                        return Ok(Value::Bool(false));
                    }
                    "keys" => return Ok(Value::List(Rc::new(RefCell::new(state.borrow().iter().map(|(k, _)| k.clone()).collect())))),
                    "values" => return Ok(Value::List(Rc::new(RefCell::new(state.borrow().iter().map(|(_, v)| v.clone()).collect())))),
                    "count" => return Ok(Value::Int(state.borrow().len() as i64)),
                    "set" => {
                        let k = self.eval_arg(&args[0], env)?;
                        let v = self.eval_arg(&args[1], env)?;
                        let snapshot = state.borrow().clone();
                        let mut found = None;
                        for (i, (mk, _)) in snapshot.iter().enumerate() {
                            if truthy(&self.eval_binary(BinOp::Eq, mk.clone(), k.clone(), env)?) { found = Some(i); break; }
                        }
                        match found {
                            Some(i) => state.borrow_mut()[i] = (k, v),
                            None => state.borrow_mut().push((k, v)),
                        }
                        return Ok(Value::Void);
                    }
                    "remove" => {
                        let k = self.eval_arg(&args[0], env)?;
                        let snapshot = state.borrow().clone();
                        let mut found = None;
                        for (i, (mk, mv)) in snapshot.iter().enumerate() {
                            if truthy(&self.eval_binary(BinOp::Eq, mk.clone(), k.clone(), env)?) { found = Some((i, mv.clone())); break; }
                        }
                        return Ok(match found {
                            Some((i, v)) => { state.borrow_mut().remove(i); some_value(v) }
                            None => none_value(),
                        });
                    }
                    _ => {}
                }
            }
            if let Value::Set(state) = &receiver {
                match method.as_str() {
                    "contains" => {
                        let x = self.eval_arg(&args[0], env)?;
                        let snapshot = state.borrow().clone();
                        for item in snapshot {
                            if truthy(&self.eval_binary(BinOp::Eq, item, x.clone(), env)?) { return Ok(Value::Bool(true)); }
                        }
                        return Ok(Value::Bool(false));
                    }
                    "count" => return Ok(Value::Int(state.borrow().len() as i64)),
                    "add" => {
                        let x = self.eval_arg(&args[0], env)?;
                        let snapshot = state.borrow().clone();
                        let exists = {
                            let mut found = false;
                            for item in snapshot {
                                if truthy(&self.eval_binary(BinOp::Eq, item, x.clone(), env)?) { found = true; break; }
                            }
                            found
                        };
                        if !exists { state.borrow_mut().push(x); }
                        return Ok(Value::Void);
                    }
                    "remove" => {
                        let x = self.eval_arg(&args[0], env)?;
                        let snapshot = state.borrow().clone();
                        let mut found = None;
                        for (i, item) in snapshot.iter().enumerate() {
                            if truthy(&self.eval_binary(BinOp::Eq, item.clone(), x.clone(), env)?) { found = Some(i); break; }
                        }
                        if let Some(i) = found { state.borrow_mut().remove(i); }
                        return Ok(Value::Void);
                    }
                    _ => {}
                }
            }
            if let Value::Task(result) = &receiver {
                if method == "join" {
                    return Ok(result.borrow().clone());
                }
            }
            if let Value::Channel(state) = &receiver {
                match method.as_str() {
                    "send" => {
                        let v = self.eval_arg(&args[0], env)?;
                        if let Some(ptr) = Self::record_ptr(&v) {
                            self.moved.insert(ptr);
                        }
                        state.borrow_mut().queue.push_back(v);
                        return Ok(Value::Void);
                    }
                    "receive" => {
                        let popped = state.borrow_mut().queue.pop_front();
                        return Ok(match popped {
                            Some(v) => Value::EnumInstance("Option".to_string(), "Some".to_string(), HashMap::from([("0".to_string(), v)]), Vec::new()),
                            None => Value::EnumInstance("Option".to_string(), "None".to_string(), HashMap::new(), Vec::new()),
                        });
                    }
                    "close" => {
                        state.borrow_mut().closed = true;
                        return Ok(Value::Void);
                    }
                    _ => {}
                }
            }
            let type_name = value_type_name(&receiver);
            if let Some(f) = self.find_method_for_value(&receiver, method) {
                let mut values = vec![receiver];
                for a in args { values.push(self.eval_arg(a, env)?); }
                return self.call_user_function(&f, values, env.clone());
            }
            return Err(RuntimeError::Error(format!("no method '{method}' for type '{type_name}'")));
        }
        let callee_v = self.eval_expr(callee, env)?;
        match callee_v {
            Value::Closure(params, body, captured) => {
                let call_env = captured.child();
                for (p, a) in params.iter().zip(args.iter()) {
                    call_env.define(p, self.eval_arg(a, env)?);
                }
                match self.eval_block(&body, &call_env) {
                    Ok(v) => Ok(v),
                    Err(RuntimeError::Return(v)) => Ok(v),
                    other => other,
                }
            }
            other => Err(RuntimeError::Error(format!("'{other}' is not callable"))),
        }
    }

    fn construct_variant(
        &mut self,
        enum_name: &str,
        variant_name: &str,
        args: &[Arg],
        env: &Env,
        explicit_type_args: Option<&[Type]>,
    ) -> EvalResult {
        let field_names: Vec<Option<String>> = self
            .enums
            .get(enum_name)
            .and_then(|e| e.variants.iter().find(|v| v.name == variant_name))
            .map(|v| v.fields.iter().map(|f| f.name.clone()).collect())
            .unwrap_or_default();
        let mut fields = HashMap::new();
        for (i, arg) in args.iter().enumerate() {
            match arg {
                Arg::Named(fname, e) => { fields.insert(fname.clone(), self.eval_expr(e, env)?); }
                Arg::Positional(e) => {
                    let key = field_names.get(i).cloned().flatten().unwrap_or_else(|| i.to_string());
                    fields.insert(key, self.eval_expr(e, env)?);
                }
            }
        }
        let type_args = explicit_type_args
            .map(|args| args.to_vec())
            .unwrap_or_else(|| self.infer_enum_type_args(enum_name, variant_name, &fields));
        Ok(Value::EnumInstance(
            enum_name.to_string(),
            variant_name.to_string(),
            fields,
            type_args,
        ))
    }

    fn eval_arg(&mut self, arg: &Arg, env: &Env) -> EvalResult {
        match arg {
            Arg::Positional(e) | Arg::Named(_, e) => self.eval_expr(e, env),
        }
    }

    fn eval_method(&mut self, obj: &Expr, field: &str, _args: &[Expr], env: &Env) -> EvalResult {
        let v = self.eval_expr(obj, env)?;
        match (field, &v) {
            ("length", Value::List(state)) => Ok(Value::Int(state.borrow().len() as i64)),
            (_, Value::Record(_, data)) => fields_get(&data.borrow(), field)
                .cloned()
                .ok_or_else(|| RuntimeError::Error(format!("'{v}' has no field '{field}'"))),
            (_, Value::EnumInstance(_, _, fields, _)) => fields
                .get(field)
                .cloned()
                .ok_or_else(|| RuntimeError::Error(format!("'{v}' has no field '{field}'"))),
            _ => Err(RuntimeError::Error(format!("'{v}' has no field/method '{field}'"))),
        }
    }
}

fn pattern_field_value<'a>(fields: &'a HashMap<String, Value>, name: &str, position: usize) -> Option<&'a Value> {
    if let Some(index) = name.strip_prefix('@').and_then(|index| index.parse::<usize>().ok()) {
        return fields.get(&index.to_string());
    }
    fields.get(name).or_else(|| fields.get(&position.to_string()))
}

fn value_type_name(v: &Value) -> String {
    match v {
        Value::Record(name, _) => name.clone(),
        Value::EnumInstance(name, _, _, _) => name.clone(),
        Value::List(_) => "List".to_string(),
        Value::Int(_) => "Int".to_string(),
        Value::Float(_) => "Float".to_string(),
        Value::Bool(_) => "Bool".to_string(),
        Value::Char(_) => "Char".to_string(),
        Value::String(_) => "String".to_string(),
        Value::Quantity(..) => "Quantity".to_string(),
        Value::Closure(..) => "Function".to_string(),
        Value::Task(_) => "Task".to_string(),
        Value::Channel(_) => "Channel".to_string(),
        Value::Map(_) => "Map".to_string(),
        Value::Set(_) => "Set".to_string(),
        Value::Void => "Void".to_string(),
    }
}

fn dimension_to_type(dimension: &Dimension) -> Type {
    if dimension.len() == 1 {
        if let Some((name, exponent)) = dimension.iter().next() {
            if *exponent == 1 {
                return Type::Named(name.clone(), Vec::new());
            }
        }
    }
    Type::Named(dim_to_string(dimension), Vec::new())
}

fn applied_type_args_match(actual: &[Type], implementation: &RuntimeImpl) -> bool {
    let generic_names: HashSet<String> = implementation.generics.iter().map(|generic| generic.name.clone()).collect();
    if actual.is_empty()
        && !implementation.type_args.is_empty()
        && implementation
            .type_args
            .iter()
            .all(|pattern| type_pattern_is_generic(pattern, &generic_names))
    {
        return true;
    }
    if actual.len() != implementation.type_args.len() {
        return false;
    }
    let mut substitutions = HashMap::new();
    implementation
        .type_args
        .iter()
        .zip(actual.iter())
        .all(|(pattern, actual)| type_pattern_matches(pattern, actual, &generic_names, &mut substitutions))
}

fn type_pattern_is_generic(pattern: &Type, generic_names: &HashSet<String>) -> bool {
    match pattern {
        Type::Named(name, args) if args.is_empty() => generic_names.contains(name),
        Type::Named(_, args) => args.iter().all(|arg| type_pattern_is_generic(arg, generic_names)),
        Type::Mul(left, right) | Type::Div(left, right) => {
            type_pattern_is_generic(left, generic_names) && type_pattern_is_generic(right, generic_names)
        }
        Type::Pow(base, _) => type_pattern_is_generic(base, generic_names),
        Type::Fn(params, return_type) => {
            params.iter().all(|param| type_pattern_is_generic(param, generic_names))
                && type_pattern_is_generic(return_type, generic_names)
        }
        Type::Dyn(_) => false,
    }
}

fn type_pattern_matches(
    pattern: &Type,
    actual: &Type,
    generic_names: &HashSet<String>,
    substitutions: &mut HashMap<String, Type>,
) -> bool {
    match (pattern, actual) {
        (Type::Named(name, args), actual) if args.is_empty() && generic_names.contains(name) => {
            match substitutions.get(name) {
                Some(previous) => previous == actual,
                None => {
                    substitutions.insert(name.clone(), actual.clone());
                    true
                }
            }
        }
        (Type::Named(pattern_name, pattern_args), Type::Named(actual_name, actual_args)) => {
            pattern_name == actual_name
                && pattern_args.len() == actual_args.len()
                && pattern_args.iter().zip(actual_args).all(|(pattern, actual)| {
                    type_pattern_matches(pattern, actual, generic_names, substitutions)
                })
        }
        (Type::Mul(pattern_left, pattern_right), Type::Mul(actual_left, actual_right))
        | (Type::Div(pattern_left, pattern_right), Type::Div(actual_left, actual_right)) => {
            type_pattern_matches(pattern_left, actual_left, generic_names, substitutions)
                && type_pattern_matches(pattern_right, actual_right, generic_names, substitutions)
        }
        (Type::Pow(pattern_base, pattern_exponent), Type::Pow(actual_base, actual_exponent)) => {
            pattern_exponent == actual_exponent
                && type_pattern_matches(pattern_base, actual_base, generic_names, substitutions)
        }
        (Type::Fn(pattern_params, pattern_return), Type::Fn(actual_params, actual_return)) => {
            pattern_params.len() == actual_params.len()
                && pattern_params.iter().zip(actual_params).all(|(pattern, actual)| {
                    type_pattern_matches(pattern, actual, generic_names, substitutions)
                })
                && type_pattern_matches(pattern_return, actual_return, generic_names, substitutions)
        }
        (Type::Dyn(pattern_traits), Type::Dyn(actual_traits)) => pattern_traits == actual_traits,
        _ => false,
    }
}

fn operator_method_name(op: BinOp) -> Option<&'static str> {
    match op {
        BinOp::Add => Some("add"),
        BinOp::Sub => Some("sub"),
        BinOp::Mul => Some("mul"),
        BinOp::Div => Some("div"),
        BinOp::Eq => Some("equals"),
        _ => None,
    }
}

fn ordering_to_i32(v: &Value) -> i32 {
    match v {
        Value::EnumInstance(_, variant, _, _) if variant == "Less" => -1,
        Value::EnumInstance(_, variant, _, _) if variant == "Greater" => 1,
        _ => 0,
    }
}

fn ordering_matches(op: BinOp, ordering: &Value) -> Result<bool, RuntimeError> {
    let Value::EnumInstance(_, variant, _, _) = ordering else {
        return Err(RuntimeError::Error(format!("'compare' must return an Ordering, got '{ordering}'")));
    };
    Ok(matches!(
        (op, variant.as_str()),
        (BinOp::Lt, "Less") | (BinOp::Gt, "Greater") | (BinOp::LtEq, "Less") | (BinOp::LtEq, "Equal") | (BinOp::GtEq, "Greater") | (BinOp::GtEq, "Equal")
    ))
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => x == y,
        (Value::Int(x), Value::Float(y)) | (Value::Float(y), Value::Int(x)) => *x as f64 == *y,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Char(x), Value::Char(y)) => x == y,
        (Value::String(x), Value::String(y)) => x == y,
        _ => false,
    }
}

fn some_value(v: Value) -> Value {
    Value::EnumInstance("Option".to_string(), "Some".to_string(), HashMap::from([("0".to_string(), v)]), Vec::new())
}

fn none_value() -> Value {
    Value::EnumInstance("Option".to_string(), "None".to_string(), HashMap::new(), Vec::new())
}

fn sum_values(items: &[Value]) -> EvalResult {
    let mut acc: Option<Value> = None;
    for item in items {
        acc = Some(match acc {
            None => item.clone(),
            Some(a) => eval_binary_builtin(BinOp::Add, a, item.clone())?,
        });
    }
    Ok(acc.unwrap_or(Value::Int(0)))
}

fn truthy(v: &Value) -> bool {
    matches!(v, Value::Bool(true))
}

fn as_i64(v: &Value) -> Result<i64, RuntimeError> {
    match v {
        Value::Int(n) => Ok(*n),
        Value::Float(n) => Ok(*n as i64),
        other => Err(RuntimeError::Error(format!("expected a number, got '{other}'"))),
    }
}

fn as_f64(v: &Value) -> Result<f64, RuntimeError> {
    match v {
        Value::Int(n) => Ok(*n as f64),
        Value::Float(n) => Ok(*n),
        Value::Quantity(n, _, _) => Ok(*n),
        other => Err(RuntimeError::Error(format!("expected a number, got '{other}'"))),
    }
}

fn eval_binary_builtin(op: BinOp, lv: Value, rv: Value) -> EvalResult {
    use BinOp::*;
    match op {
        Add | Sub => match (&lv, &rv) {
            (Value::Quantity(a, d1, u1), Value::Quantity(b, d2, u2)) => {
                if d1 != d2 {
                    return Err(RuntimeError::Error(format!(
                        "Invalid dimensional operation: {} vs {}",
                        dim_to_string(d1),
                        dim_to_string(d2)
                    )));
                }
                let converted_b = convert(*b, u2, u1)?;
                let result = if op == Add { a + converted_b } else { a - converted_b };
                Ok(Value::Quantity(result, d1.clone(), u1.clone()))
            }
            (Value::Quantity(..), _) | (_, Value::Quantity(..)) => Err(RuntimeError::Error(
                "cannot combine a Quantity with a plain scalar without an explicit unit ('as <unit>')".to_string(),
            )),
            (Value::String(a), Value::String(b)) if op == Add => Ok(Value::String(format!("{a}{b}"))),
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(if op == Add { a + b } else { a - b })),
            _ => {
                let a = as_f64(&lv)?;
                let b = as_f64(&rv)?;
                Ok(Value::Float(if op == Add { a + b } else { a - b }))
            }
        },
        Mul | Div => match (&lv, &rv) {
            (Value::Quantity(a, d1, u1), Value::Quantity(b, d2, u2)) => {
                let combined_dim = if op == Mul { dim_mul(d1, d2) } else { dim_div(d1, d2) };
                let value = if op == Mul { a * b } else { a / b };
                if op == Div && dim_is_dimensionless(&combined_dim) {
                    let converted_b = convert(*b, u2, u1)?;
                    Ok(Value::Float(a / converted_b))
                } else {
                    let unit = if op == Mul { format!("{u1}*{u2}") } else { format!("{u1}/{u2}") };
                    Ok(Value::Quantity(value, combined_dim, unit))
                }
            }
            (Value::Quantity(a, d, u), scalar) if op == Mul || op == Div => {
                let s = as_f64(scalar)?;
                let value = if op == Mul { a * s } else { a / s };
                Ok(Value::Quantity(value, d.clone(), u.clone()))
            }
            (scalar, Value::Quantity(a, d, u)) if op == Mul => {
                let s = as_f64(scalar)?;
                Ok(Value::Quantity(s * a, d.clone(), u.clone()))
            }
            (scalar, Value::Quantity(a, d, u)) if op == Div => {
                let s = as_f64(scalar)?;
                Ok(Value::Quantity(s / a, dim_pow(d, -1), format!("1/{u}")))
            }
            (Value::Int(a), Value::Int(b)) => {
                Ok(if op == Mul { Value::Int(a * b) } else { Value::Float(*a as f64 / *b as f64) })
            }
            _ => {
                let a = as_f64(&lv)?;
                let b = as_f64(&rv)?;
                Ok(Value::Float(if op == Mul { a * b } else { a / b }))
            }
        },
        Eq | NotEq | Lt | Gt | LtEq | GtEq => {
            let ordering = compare(&lv, &rv)?;
            Ok(Value::Bool(match op {
                Eq => ordering == 0,
                NotEq => ordering != 0,
                Lt => ordering < 0,
                Gt => ordering > 0,
                LtEq => ordering <= 0,
                GtEq => ordering >= 0,
                _ => unreachable!(),
            }))
        }
        And => Ok(Value::Bool(truthy(&lv) && truthy(&rv))),
        Or => Ok(Value::Bool(truthy(&lv) || truthy(&rv))),
    }
}

fn compare(lv: &Value, rv: &Value) -> Result<i32, RuntimeError> {
    match (lv, rv) {
        (Value::Quantity(a, d1, u1), Value::Quantity(b, d2, u2)) => {
            if d1 != d2 {
                return Err(RuntimeError::Error("cannot compare quantities of different dimensions".to_string()));
            }
            let converted_b = convert(*b, u2, u1)?;
            Ok(cmp_f64(*a, converted_b))
        }
        (Value::String(a), Value::String(b)) => Ok(if a == b { 0 } else if a < b { -1 } else { 1 }),
        (Value::Bool(a), Value::Bool(b)) => Ok(if a == b { 0 } else if !*a { -1 } else { 1 }),
        _ => Ok(cmp_f64(as_f64(lv)?, as_f64(rv)?)),
    }
}

fn cmp_f64(a: f64, b: f64) -> i32 {
    if a < b { -1 } else if a > b { 1 } else { 0 }
}

fn unit_factor(symbol: &str) -> Option<f64> {
    Some(match symbol {
        "m" | "s" | "kg" | "K" | "A" | "mol" | "cd" | "USD" | "bit" | "C" | "atm" | "Pa" => 1.0,
        "nm" => 1e-9,
        "km" => 1000.0,
        "cm" => 0.01,
        "mm" => 0.001,
        "ms" => 0.001,
        "min" => 60.0,
        "h" => 3600.0,
        "g" => 0.001,
        "mg" => 1e-6,
        "mmol" => 0.001,
        "L" => 0.001,
        "EUR" => 1.0,
        "byte" => 8.0,
        _ => return None,
    })
}

fn resolve_unit_factor(expr: &str) -> Result<f64, RuntimeError> {
    let mut result = 1.0;
    let mut op = '*';
    let mut chars = expr.chars().peekable();
    loop {
        let mut atom = String::new();
        while let Some(&c) = chars.peek() {
            if c == '*' || c == '/' || c == '^' { break; }
            atom.push(c);
            chars.next();
        }
        if atom.is_empty() {
            return Err(RuntimeError::Error(format!("malformed unit expression '{expr}'")));
        }
        let mut factor = unit_factor(&atom).ok_or_else(|| RuntimeError::Error(format!("unknown unit '{atom}'")))?;
        if chars.peek() == Some(&'^') {
            chars.next();
            let mut exp_str = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_ascii_digit() || c == '-' { exp_str.push(c); chars.next(); } else { break; }
            }
            let exp: i32 = exp_str.parse().map_err(|_| RuntimeError::Error(format!("invalid exponent in '{expr}'")))?;
            factor = factor.powi(exp);
        }
        result = if op == '*' { result * factor } else { result / factor };
        match chars.next() {
            Some(c @ ('*' | '/')) => op = c,
            None => break,
            _ => return Err(RuntimeError::Error(format!("malformed unit expression '{expr}'"))),
        }
    }
    Ok(result)
}

fn convert(value: f64, from_unit: &str, to_unit: &str) -> Result<f64, RuntimeError> {
    if from_unit == to_unit {
        return Ok(value);
    }
    let f_from = resolve_unit_factor(from_unit)?;
    let f_to = resolve_unit_factor(to_unit)?;
    Ok(value * f_from / f_to)
}
