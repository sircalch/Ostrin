use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;
use std::fs;
use std::io::{BufRead, Write};
use std::rc::Rc;

use serde_json::{json, Value as JsonValue};

use crate::ast::*;
use crate::protocol;

mod array;
mod detmath;
mod regress;
mod math;
mod rng;
mod strings;
use crate::types::{dim_div, dim_is_dimensionless, dim_mul, dim_pow, dim_to_string, resolve_unit_expr, Dimension};

#[derive(Clone)]
pub(crate) struct MapState {
    entries: Vec<(Value, Value)>,
    /// Hash buckets store entry indexes so iteration remains insertion-ordered;
    /// scalar-key lookups stay O(1), while mutable reference-backed keys are
    /// reindexed immediately before lookup.
    index: HashMap<u64, Vec<usize>>,
    refresh_before_lookup: bool,
}

impl MapState {
    fn new(entries: Vec<(Value, Value)>) -> Self {
        let mut state = Self { entries, index: HashMap::new(), refresh_before_lookup: false };
        state.rebuild_index();
        state
    }

    fn rebuild_index(&mut self) {
        self.rebuild_index_with(map_key_hash);
    }

    fn rebuild_index_with<F>(&mut self, hash_value: F)
    where
        F: Fn(&Value) -> Option<u64>,
    {
        self.index.clear();
        for (position, (key, _)) in self.entries.iter().enumerate() {
            if let Some(hash) = hash_value(key) {
                self.index.entry(hash).or_default().push(position);
            }
        }
        self.refresh_before_lookup = self.entries.iter().any(|(key, _)| value_may_mutate(key));
    }

    fn candidates_with_hash(&self, hash: Option<u64>) -> Vec<usize> {
        match hash.and_then(|hash| self.index.get(&hash)) {
            Some(indexes) => indexes.clone(),
            None => (0..self.entries.len()).collect(),
        }
    }
}

impl std::ops::Deref for MapState {
    type Target = Vec<(Value, Value)>;

    fn deref(&self) -> &Self::Target { &self.entries }
}

impl std::ops::DerefMut for MapState {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.entries }
}

impl IntoIterator for MapState {
    type Item = (Value, Value);
    type IntoIter = std::vec::IntoIter<(Value, Value)>;

    fn into_iter(self) -> Self::IntoIter { self.entries.into_iter() }
}

#[derive(Clone)]
pub(crate) struct SetState {
    entries: Vec<Value>,
    index: HashMap<u64, Vec<usize>>,
    refresh_before_lookup: bool,
}

impl SetState {
    fn new(entries: Vec<Value>) -> Self {
        let mut state = Self { entries, index: HashMap::new(), refresh_before_lookup: false };
        state.rebuild_index();
        state
    }

    fn rebuild_index(&mut self) {
        self.rebuild_index_with(map_key_hash);
    }

    fn rebuild_index_with<F>(&mut self, hash_value: F)
    where
        F: Fn(&Value) -> Option<u64>,
    {
        self.index.clear();
        for (position, value) in self.entries.iter().enumerate() {
            if let Some(hash) = hash_value(value) {
                self.index.entry(hash).or_default().push(position);
            }
        }
        self.refresh_before_lookup = self.entries.iter().any(value_may_mutate);
    }

    fn candidates_with_hash(&self, hash: Option<u64>) -> Vec<usize> {
        match hash.and_then(|hash| self.index.get(&hash)) {
            Some(indexes) => indexes.clone(),
            None => (0..self.entries.len()).collect(),
        }
    }
}

impl std::ops::Deref for SetState {
    type Target = Vec<Value>;

    fn deref(&self) -> &Self::Target { &self.entries }
}

impl std::ops::DerefMut for SetState {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.entries }
}

impl IntoIterator for SetState {
    type Item = Value;
    type IntoIter = std::vec::IntoIter<Value>;

    fn into_iter(self) -> Self::IntoIter { self.entries.into_iter() }
}

#[derive(Clone)]
pub enum Value {
    Int(i64),
    /// A fixed-width integer (`UInt8`, `Int32`, …), stored widened.
    Sized(i128, IntKind),
    /// A single-precision float (`Float32`).
    F32(f32),
    /// A dense N-dimensional numeric array (`Array<T>`), by reference like `List`.
    Array(Rc<RefCell<array::ArrayData>>),
    /// A reproducible random generator (`Rng`), by reference.
    Rng(Rc<RefCell<rng::RngState>>),
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
    Task(Rc<RefCell<TaskState>>),
    Channel(Rc<RefCell<ChannelState>>),
    Map(Rc<RefCell<MapState>>),
    Set(Rc<RefCell<SetState>>),
    Void,
}

fn map_key_hash(value: &Value) -> Option<u64> {
    stable_hash_value(value)
}

fn value_may_mutate(value: &Value) -> bool {
    match value {
        Value::List(_) | Value::Map(_) | Value::Set(_) | Value::Record(_, _) => true,
        Value::EnumInstance(_, _, fields, _) => fields.values().any(value_may_mutate),
        _ => false,
    }
}

fn stable_hash_u64(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58476d1ce4e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}

fn stable_hash_string(value: &str) -> u64 {
    let mut hash = 1469598103934665603u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(1099511628211);
    }
    hash
}

fn stable_hash_combine(left: u64, right: u64) -> u64 {
    stable_hash_u64(left ^ right.rotate_left(17))
}

fn stable_hash_value(value: &Value) -> Option<u64> {
    match value {
        Value::Int(n) => Some(stable_hash_u64(*n as u64)),
        Value::Sized(n, _) => Some(stable_hash_u64(*n as u64)),
        Value::F32(n) => Some(stable_hash_u64(u64::from(n.to_bits()))),
        Value::Float(n) => Some(stable_hash_u64(n.to_bits())),
        Value::Bool(value) => Some(stable_hash_u64(u64::from(*value))),
        Value::String(value) => Some(stable_hash_string(value)),
        Value::EnumInstance(type_name, variant, fields, _)
            if type_name == "Option" || type_name == "Result" =>
        {
            let tag = stable_hash_string(&format!("{type_name}::{variant}"));
            if let Some(inner) = fields.get("0") {
                Some(stable_hash_combine(tag, stable_hash_value(inner)?))
            } else {
                Some(tag)
            }
        }
        _ => None,
    }
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

pub(crate) struct TaskState {
    body: Rc<Block>,
    env: Env,
    status: TaskStatus,
    result: Option<Result<Value, String>>,
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{n}"),
            Value::Sized(n, _) => write!(f, "{n}"),
            Value::F32(n) => write!(f, "{n}"),
            Value::Array(a) => write!(f, "{}", array::display(&a.borrow())),
            Value::Rng(_) => write!(f, "Rng"),
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
            Value::Task(task) => {
                let state = task.borrow();
                match (&state.status, &state.result) {
                    (TaskStatus::Completed, Some(Ok(value))) => write!(f, "Task({value})"),
                    (TaskStatus::Failed, Some(Err(error))) => write!(f, "Task(error: {error})"),
                    (TaskStatus::Running, _) => write!(f, "Task(running)"),
                    _ => write!(f, "Task(pending)"),
                }
            }
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
    /// Unwinds the whole program when a connected debugger sends
    /// `disconnect`/`terminate` while execution is paused (see `dap.rs`).
    Terminated,
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

/// One entry in the call stack the debugger reports through
/// `stackTrace`/`scopes`/`variables`. Pushed and popped only at real function
/// call boundaries (`call_user_function*`, closure calls); a nested block
/// (`if`/`while`/`for`/`match`/`spawn`) just updates `current_env`/`line` on
/// the frame that is already on top, since none of those introduce a new
/// logical stack frame.
struct CallFrame {
    name: String,
    file: Option<String>,
    line: usize,
    /// The environment `call_user_function` created for this call (holding
    /// its parameters). `variables` walks from `current_env` up to and
    /// including this one, then stops — showing only this frame's locals,
    /// not whatever lexically encloses it.
    base_env: Env,
    current_env: Env,
}

#[derive(PartialEq)]
enum StepMode {
    None,
    Into,
    /// Also covers step-out: both just mean "run until the call stack is at
    /// or shallower than `step_depth`", they only differ in what `step_depth`
    /// was set to when the request came in.
    UntilDepth,
}

/// Debug Adapter Protocol session state, owned by the `Interpreter` while a
/// program runs under `ostrinc --dap` (see `dap.rs`). Reading/writing this
/// struct's transport happens entirely within `interpreter/mod.rs` because
/// answering `stackTrace`/`variables`/`evaluate` needs direct access to the
/// interpreter's live state at the exact moment execution is paused — there
/// is no separate thread or coroutine involved, pausing just means "block on
/// a read from stdin instead of returning", using the interpreter's own
/// existing call stack as the only stack that matters.
pub struct Debugger {
    reader: Box<dyn BufRead>,
    writer: Box<dyn Write>,
    breakpoints: HashMap<String, HashSet<usize>>,
    seq: i64,
    pending_entry_stop: bool,
    step: StepMode,
    step_depth: usize,
}

impl Debugger {
    pub fn new(
        reader: Box<dyn BufRead>,
        writer: Box<dyn Write>,
        breakpoints: HashMap<String, HashSet<usize>>,
        stop_on_entry: bool,
    ) -> Self {
        Debugger {
            reader,
            writer,
            breakpoints,
            seq: 0,
            pending_entry_stop: stop_on_entry,
            step: StepMode::None,
            step_depth: 0,
        }
    }

    pub fn set_breakpoints(&mut self, file: String, lines: HashSet<usize>) {
        self.breakpoints.insert(file, lines);
    }

    fn next_seq(&mut self) -> i64 {
        self.seq += 1;
        self.seq
    }

    pub fn send_event(&mut self, event: &str, body: JsonValue) {
        let seq = self.next_seq();
        let _ = protocol::write_message(
            &mut *self.writer,
            &json!({ "seq": seq, "type": "event", "event": event, "body": body }),
        );
    }

    pub fn send_output(&mut self, text: &str) {
        self.send_event("output", json!({ "category": "stdout", "output": text }));
    }

    fn send_response(&mut self, request_seq: i64, command: &str, body: JsonValue) {
        let seq = self.next_seq();
        let _ = protocol::write_message(
            &mut *self.writer,
            &json!({
                "seq": seq, "type": "response", "request_seq": request_seq,
                "success": true, "command": command, "body": body
            }),
        );
    }
}

fn dap_scopes(arguments: &JsonValue) -> JsonValue {
    let frame_id = arguments.get("frameId").and_then(JsonValue::as_i64).unwrap_or(0);
    json!({ "scopes": [{ "name": "Locals", "variablesReference": frame_id + 1, "expensive": false }] })
}

fn dap_set_breakpoints(dbg: &mut Debugger, arguments: &JsonValue) -> JsonValue {
    let path = arguments
        .get("source")
        .and_then(|source| source.get("path"))
        .and_then(JsonValue::as_str)
        .map(|path| std::fs::canonicalize(path).map(|p| p.display().to_string()).unwrap_or_else(|_| path.to_string()))
        .unwrap_or_default();
    let requested = arguments.get("breakpoints").and_then(JsonValue::as_array).cloned().unwrap_or_default();
    let lines: HashSet<usize> = requested
        .iter()
        .filter_map(|entry| entry.get("line").and_then(JsonValue::as_u64))
        .map(|line| line as usize)
        .collect();
    dbg.set_breakpoints(path, lines);
    let verified: Vec<JsonValue> = requested
        .iter()
        .map(|entry| json!({ "verified": true, "line": entry.get("line").cloned().unwrap_or(JsonValue::Null) }))
        .collect();
    json!({ "breakpoints": verified })
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
    /// Names of records/enums whose representation contains mutable state.
    /// Immutable records are copy/share-safe when sent through a channel.
    movable_types: HashSet<String>,
    moved: HashSet<usize>,
    call_stack: Vec<CallFrame>,
    debugger: Option<Debugger>,
    terminated: bool,
    /// Cooperative tasks created by `spawn`. The interpreter owns the queue;
    /// a Task value is only a handle to one of these states. This keeps the
    /// interpreter deterministic while giving `spawn` real deferred
    /// semantics before the native thread backend is introduced.
    tasks: Vec<Rc<RefCell<TaskState>>>,
    /// Integer literals the checker typed as fixed-width (see `TypedProgram::literal_kinds`).
    literal_kinds: HashMap<crate::typeck::ExprKey, LitKind>,
}

impl Interpreter {
    /// Supplies the checker's fixed-width literal choices.
    pub fn with_literal_kinds(mut self, kinds: HashMap<crate::typeck::ExprKey, LitKind>) -> Self {
        self.literal_kinds = kinds;
        self
    }

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
        let movable_types = crate::ownership::movable_types(items);
        Interpreter {
            functions,
            records,
            enums,
            variant_to_enum,
            impls,
            traits,
            derives,
            runtime_record_type_args: HashMap::new(),
            movable_types,
            moved: HashSet::new(),
            call_stack: Vec::new(),
            debugger: None,
            literal_kinds: HashMap::new(),
            terminated: false,
            tasks: Vec::new(),
        }
    }

    /// Attaches a debugger before `run_main` executes; every statement
    /// boundary will now check breakpoints/step state and, when this
    /// program's own `print()` runs, its output is relayed as a DAP `output`
    /// event instead of going straight to the real stdout (see `dap.rs`).
    pub fn attach_debugger(&mut self, debugger: Debugger) {
        self.debugger = Some(debugger);
    }

    /// Reclaims the debugger transport after `run_main` returns, so the
    /// caller (`dap.rs`) can send the final `exited`/`terminated` events over
    /// the same stdio stream. `None` only when a pause loop's stdin read
    /// failed outright (client vanished without a clean `disconnect`).
    pub fn take_debugger(&mut self) -> Option<Debugger> {
        self.debugger.take()
    }

    fn task_error(error: RuntimeError) -> String {
        match error {
            RuntimeError::Error(message) => message,
            RuntimeError::Return(_) => "task returned through an invalid control-flow path".to_string(),
            RuntimeError::Break(_) => "task escaped with break".to_string(),
            RuntimeError::Continue => "task escaped with continue".to_string(),
            RuntimeError::Terminated => "task was terminated by the debugger".to_string(),
        }
    }

    /// Runs one task to completion on the interpreter's cooperative scheduler.
    /// The task is deliberately removed from the pending set while executing,
    /// so a cyclic join is diagnosed instead of recursing forever.
    fn run_task(&mut self, task: Rc<RefCell<TaskState>>) -> EvalResult {
        let (body, env) = {
            let mut state = task.borrow_mut();
            match state.status {
                TaskStatus::Pending => {
                    state.status = TaskStatus::Running;
                    (state.body.clone(), state.env.clone())
                }
                TaskStatus::Running => {
                    return Err(RuntimeError::Error("cyclic task join would deadlock".to_string()));
                }
                TaskStatus::Completed => {
                    return match state.result.clone() {
                        Some(Ok(value)) => Ok(value),
                        Some(Err(error)) => Err(RuntimeError::Error(format!("task failed: {error}"))),
                        None => Err(RuntimeError::Error("completed task has no result".to_string())),
                    };
                }
                TaskStatus::Failed => {
                    return match state.result.clone() {
                        Some(Err(error)) => Err(RuntimeError::Error(format!("task failed: {error}"))),
                        _ => Err(RuntimeError::Error("failed task has no error".to_string())),
                    };
                }
            }
        };

        let outcome = match self.eval_block(&body, &env) {
            Ok(value) => Ok(value),
            Err(RuntimeError::Return(value)) => Ok(value),
            Err(error) => Err(Self::task_error(error)),
        };
        let status = if outcome.is_ok() { TaskStatus::Completed } else { TaskStatus::Failed };
        {
            let mut state = task.borrow_mut();
            state.status = status;
            state.result = Some(outcome.clone());
        }
        match outcome {
            Ok(value) => Ok(value),
            Err(error) => Err(RuntimeError::Error(format!("task failed: {error}"))),
        }
    }

    /// Makes one pending task make progress. Returning `false` means the
    /// scheduler has no runnable task left, which is how the interpreter
    /// reports an unfinished channel wait instead of hanging the process.
    fn run_one_pending_task(&mut self) -> Result<bool, RuntimeError> {
        let task = self.tasks.iter().find_map(|candidate| {
            (candidate.borrow().status == TaskStatus::Pending).then(|| candidate.clone())
        });
        let Some(task) = task else { return Ok(false) };
        self.run_task(task)?;
        Ok(true)
    }

    fn drain_tasks_from(&mut self, start: usize) -> Result<(), RuntimeError> {
        loop {
            let task = self.tasks.iter().skip(start).find_map(|candidate| {
                (candidate.borrow().status == TaskStatus::Pending).then(|| candidate.clone())
            });
            let Some(task) = task else { return Ok(()) };
            self.run_task(task)?;
        }
    }

    fn has_derive(&self, type_name: &str, trait_name: &str) -> bool {
        self.derives.get(type_name).is_some_and(|d| d.iter().any(|t| t == trait_name))
    }

    fn hash_value(&self, value: &Value) -> Option<u64> {
        match value {
            Value::List(state) => {
                let mut hash = stable_hash_string("List");
                let items = state.borrow().clone();
                for item in &items {
                    hash = stable_hash_combine(hash, self.hash_value(item)?);
                }
                Some(hash)
            }
            Value::Map(state) => {
                let entries = state.borrow().entries.clone();
                let mut sum = 0u64;
                for (key, value) in &entries {
                    let entry = stable_hash_combine(self.hash_value(key)?, self.hash_value(value)?);
                    sum = sum.wrapping_add(entry.rotate_left(17));
                }
                Some(stable_hash_combine(
                    stable_hash_string("Map"),
                    stable_hash_u64(sum ^ entries.len() as u64),
                ))
            }
            Value::Set(state) => {
                let entries = state.borrow().entries.clone();
                let mut sum = 0u64;
                for item in &entries {
                    sum = sum.wrapping_add(self.hash_value(item)?.rotate_left(17));
                }
                Some(stable_hash_combine(
                    stable_hash_string("Set"),
                    stable_hash_u64(sum ^ entries.len() as u64),
                ))
            }
            Value::Record(type_name, data) if self.has_derive(type_name, "Hash") => {
                let declaration = self.records.get(type_name)?;
                let mut hash = stable_hash_string(&format!("Record::{type_name}"));
                let fields = data.borrow();
                for field in &declaration.fields {
                    let value = fields_get(&fields, &field.name)?;
                    hash = stable_hash_combine(hash, self.hash_value(value)?);
                }
                Some(hash)
            }
            Value::EnumInstance(type_name, variant, fields, _)
                if type_name == "Option" || type_name == "Result" =>
            {
                let tag = stable_hash_string(&format!("{type_name}::{variant}"));
                if let Some(inner) = fields.get("0") {
                    Some(stable_hash_combine(tag, self.hash_value(inner)?))
                } else {
                    Some(tag)
                }
            }
            Value::EnumInstance(type_name, variant_name, fields, _)
                if self.has_derive(type_name, "Hash") =>
            {
                let declaration = self.enums.get(type_name)?;
                let variant = declaration.variants.iter().find(|candidate| candidate.name == *variant_name)?;
                let mut hash = stable_hash_string(&format!("Enum::{type_name}::{variant_name}"));
                for (index, field) in variant.fields.iter().enumerate() {
                    let key = field.name.clone().unwrap_or_else(|| index.to_string());
                    hash = stable_hash_combine(hash, self.hash_value(fields.get(&key)?)?);
                }
                Some(hash)
            }
            _ => stable_hash_value(value),
        }
    }

    /// Hashes values for collection indexes only when the equality contract
    /// is known to match. Built-in collections and sum types use their
    /// structural equality; user types require both derives and no custom
    /// `equals` method, otherwise lookup safely falls back to a linear scan.
    fn index_hash(&self, value: &Value) -> Option<u64> {
        match value {
            Value::List(state) => {
                let mut hash = stable_hash_string("List");
                for item in &state.borrow().clone() {
                    hash = stable_hash_combine(hash, self.index_hash(item)?);
                }
                Some(hash)
            }
            Value::Map(state) => {
                let entries = state.borrow().entries.clone();
                let mut sum = 0u64;
                for (key, value) in &entries {
                    let entry = stable_hash_combine(self.index_hash(key)?, self.index_hash(value)?);
                    sum = sum.wrapping_add(entry.rotate_left(17));
                }
                Some(stable_hash_combine(
                    stable_hash_string("Map"),
                    stable_hash_u64(sum ^ entries.len() as u64),
                ))
            }
            Value::Set(state) => {
                let entries = state.borrow().entries.clone();
                let mut sum = 0u64;
                for item in &entries {
                    sum = sum.wrapping_add(self.index_hash(item)?.rotate_left(17));
                }
                Some(stable_hash_combine(
                    stable_hash_string("Set"),
                    stable_hash_u64(sum ^ entries.len() as u64),
                ))
            }
            Value::EnumInstance(type_name, variant, fields, _)
                if type_name == "Option" || type_name == "Result" =>
            {
                let tag = stable_hash_string(&format!("{type_name}::{variant}"));
                if let Some(inner) = fields.get("0") {
                    Some(stable_hash_combine(tag, self.index_hash(inner)?))
                } else {
                    Some(tag)
                }
            }
            Value::Record(type_name, data)
                if self.has_derive(type_name, "Hash")
                    && self.has_derive(type_name, "Eq")
                    && self.find_method_for_value(value, "equals").is_none() =>
            {
                let declaration = self.records.get(type_name)?;
                let mut hash = stable_hash_string(&format!("Record::{type_name}"));
                let fields = data.borrow();
                for field in &declaration.fields {
                    let value = fields_get(&fields, &field.name)?;
                    hash = stable_hash_combine(hash, self.index_hash(value)?);
                }
                Some(hash)
            }
            Value::EnumInstance(type_name, variant_name, fields, _)
                if self.has_derive(type_name, "Hash")
                    && self.has_derive(type_name, "Eq")
                    && self.find_method_for_value(value, "equals").is_none() =>
            {
                let declaration = self.enums.get(type_name)?;
                let variant = declaration.variants.iter().find(|candidate| candidate.name == *variant_name)?;
                let mut hash = stable_hash_string(&format!("Enum::{type_name}::{variant_name}"));
                for (index, field) in variant.fields.iter().enumerate() {
                    let key = field.name.clone().unwrap_or_else(|| index.to_string());
                    hash = stable_hash_combine(hash, self.index_hash(fields.get(&key)?)?);
                }
                Some(hash)
            }
            _ => stable_hash_value(value),
        }
    }

    fn reindex_map(&self, state: &Rc<RefCell<MapState>>) {
        state.borrow_mut().rebuild_index_with(|value| self.index_hash(value));
    }

    fn reindex_set(&self, state: &Rc<RefCell<SetState>>) {
        state.borrow_mut().rebuild_index_with(|value| self.index_hash(value));
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

    /// Structural equality for the collection and sum types supplied by the
    /// language core. Records/enums still use `derive(Eq)` or an explicit
    /// `equals` method; these built-ins have a stable value semantics even
    /// though their storage is reference counted internally.
    fn structural_equals(&mut self, lv: &Value, rv: &Value, env: &Env) -> Result<bool, RuntimeError> {
        match (lv, rv) {
            (Value::List(left), Value::List(right)) => {
                let left = left.borrow().clone();
                let right = right.borrow().clone();
                if left.len() != right.len() { return Ok(false); }
                for (a, b) in left.into_iter().zip(right) {
                    if !truthy(&self.eval_binary(BinOp::Eq, a, b, env)?) { return Ok(false); }
                }
                Ok(true)
            }
            (Value::Map(left), Value::Map(right)) => {
                let left = left.borrow().clone();
                let right = right.borrow().clone();
                if left.len() != right.len() { return Ok(false); }
                for (key, value) in left {
                    let mut found = false;
                    for (other_key, other_value) in right.entries.iter() {
                        if truthy(&self.eval_binary(BinOp::Eq, key.clone(), other_key.clone(), env)?) {
                            if !truthy(&self.eval_binary(BinOp::Eq, value.clone(), other_value.clone(), env)?) {
                                return Ok(false);
                            }
                            found = true;
                            break;
                        }
                    }
                    if !found { return Ok(false); }
                }
                Ok(true)
            }
            (Value::Set(left), Value::Set(right)) => {
                let left = left.borrow().clone();
                let right = right.borrow().clone();
                if left.len() != right.len() { return Ok(false); }
                for value in left {
                    let mut found = false;
                    for other in right.entries.iter() {
                        if truthy(&self.eval_binary(BinOp::Eq, value.clone(), other.clone(), env)?) {
                            found = true;
                            break;
                        }
                    }
                    if !found { return Ok(false); }
                }
                Ok(true)
            }
            (Value::EnumInstance(left_type, left_variant, left_fields, _), Value::EnumInstance(right_type, right_variant, right_fields, _))
                if left_type == right_type && matches!(left_type.as_str(), "Option" | "Result") =>
            {
                if left_variant != right_variant { return Ok(false); }
                match (left_fields.get("0"), right_fields.get("0")) {
                    (Some(a), Some(b)) => Ok(truthy(&self.eval_binary(BinOp::Eq, a.clone(), b.clone(), env)?)),
                    (None, None) => Ok(true),
                    _ => Ok(false),
                }
            }
            _ => Ok(false),
        }
    }

    fn map_find(&mut self, state: &Rc<RefCell<MapState>>, key: &Value, env: &Env) -> Result<Option<usize>, RuntimeError> {
        if state.borrow().refresh_before_lookup {
            self.reindex_map(state);
        }
        let candidates = state.borrow().candidates_with_hash(self.index_hash(key));
        for index in candidates {
            let existing = state.borrow().entries[index].0.clone();
            if truthy(&self.eval_binary(BinOp::Eq, existing, key.clone(), env)?) {
                return Ok(Some(index));
            }
        }
        Ok(None)
    }

    fn set_find(&mut self, state: &Rc<RefCell<SetState>>, value: &Value, env: &Env) -> Result<Option<usize>, RuntimeError> {
        if state.borrow().refresh_before_lookup {
            self.reindex_set(state);
        }
        let candidates = state.borrow().candidates_with_hash(self.index_hash(value));
        for index in candidates {
            let existing = state.borrow().entries[index].clone();
            if truthy(&self.eval_binary(BinOp::Eq, existing, value.clone(), env)?) {
                return Ok(Some(index));
            }
        }
        Ok(None)
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
        // `2.0 * x` with a user type on the right: the reflected method (`rmul`, `radd`, …) of that type.
        if matches!(rv, Value::Record(..) | Value::EnumInstance(..)) && !matches!(lv, Value::Record(..) | Value::EnumInstance(..)) {
            if let Some(method) = operator_method_name(op).filter(|_| op != BinOp::Eq).map(|m| format!("r{m}")) {
                if let Some(f) = self.find_method_for_value(&rv, &method) {
                    return self.call_user_function(&f, vec![rv, lv], env.clone());
                }
            }
        }
        if matches!(lv, Value::List(..) | Value::Map(..) | Value::Set(..))
            || matches!(rv, Value::List(..) | Value::Map(..) | Value::Set(..))
            || matches!((&lv, &rv), (Value::EnumInstance(a, ..), Value::EnumInstance(b, ..)) if a == "Option" && b == "Option" || a == "Result" && b == "Result")
        {
            return match op {
                BinOp::Eq => Ok(Value::Bool(self.structural_equals(&lv, &rv, env)?)),
                BinOp::NotEq => Ok(Value::Bool(!self.structural_equals(&lv, &rv, env)?)),
                _ => Err(RuntimeError::Error("only '==' and '!=' are defined for collections, Option and Result".to_string())),
            };
        }
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
                span: Span::default(),
                source_file: None,
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
            Value::Sized(_, kind) => Type::Named(kind.name().to_string(), Vec::new()),
            Value::F32(_) => Type::Named("Float32".to_string(), Vec::new()),
            Value::Rng(_) => Type::Named("Rng".to_string(), Vec::new()),
            Value::Array(a) => {
                let element = a.borrow().data.first().map(|v| self.runtime_type_of_value(v)).unwrap_or_else(|| Type::Named("Unknown".to_string(), Vec::new()));
                Type::Named("Array".to_string(), vec![element])
            }
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

    /// The zero-argument functions named `test_*`, in source order: what
    /// `ostrinc --test` runs.
    pub fn test_function_names(&self) -> Vec<String> {
        let mut tests: Vec<(usize, String)> = self
            .functions
            .values()
            .filter(|f| f.name.starts_with("test_") && f.params.is_empty())
            .map(|f| (f.span.line, f.name.clone()))
            .collect();
        tests.sort();
        tests.into_iter().map(|(_, name)| name).collect()
    }

    pub fn run_main(&mut self) -> Result<Value, String> {
        self.run_function("main")
    }

    pub fn run_function(&mut self, name: &str) -> Result<Value, String> {
        let Some(main_fn) = self.functions.get(name).cloned() else {
            return Err(format!("no '{name}' function found"));
        };
        let env = Env::root();
        match self.call_user_function(&main_fn, vec![], env) {
            Ok(v) => Ok(v),
            Err(RuntimeError::Error(msg)) => Err(msg),
            Err(RuntimeError::Return(v)) => Ok(v),
            Err(RuntimeError::Break(_)) => Err("'break' outside a loop".to_string()),
            Err(RuntimeError::Continue) => Err("'continue' outside a loop".to_string()),
            Err(RuntimeError::Terminated) => Err("debug session terminated".to_string()),
        }
    }

    /// Pushes/pops the `CallFrame` a debugger session reports through
    /// `stackTrace`, wrapping the `Return`-unwinding both call paths below
    /// already needed. Not used by `call_callable` (lambdas don't carry a
    /// declared name/source span worth showing as their own frame; their
    /// statements are attributed to whichever named frame called them).
    fn run_function_body(&mut self, f: &FunctionDecl, call_env: &Env) -> EvalResult {
        self.call_stack.push(CallFrame {
            name: f.name.clone(),
            file: f.source_file.clone(),
            line: f.span.line,
            base_env: call_env.clone(),
            current_env: call_env.clone(),
        });
        let result = match self.eval_block(&f.body, call_env) {
            Ok(v) => Ok(v),
            Err(RuntimeError::Return(v)) => Ok(v),
            other => other,
        };
        self.call_stack.pop();
        result
    }

    fn call_user_function(&mut self, f: &FunctionDecl, args: Vec<Value>, closure_env: Env) -> EvalResult {
        let call_env = closure_env.child();
        for (param, arg) in f.params.iter().zip(args.into_iter()) {
            call_env.define(&param.name, arg);
        }
        self.run_function_body(f, &call_env)
    }

    fn call_user_function_with_args(
        &mut self,
        f: &FunctionDecl,
        args: &[Arg],
        closure_env: Env,
    ) -> EvalResult {
        let mut values: Vec<Option<Value>> = (0..f.params.len()).map(|_| None).collect();
        let mut next_positional = 0usize;
        let mut saw_named = false;

        for arg in args {
            match arg {
                Arg::Positional(_) => {
                    if saw_named {
                        return Err(RuntimeError::Error(
                            "positional arguments must come before named arguments".to_string(),
                        ));
                    }
                    if next_positional >= f.params.len() {
                        return Err(RuntimeError::Error(format!(
                            "function '{}' expects at most {} argument(s), got {}",
                            f.name,
                            f.params.len(),
                            args.len()
                        )));
                    }
                    values[next_positional] = Some(self.eval_arg(arg, &closure_env)?);
                    next_positional += 1;
                }
                Arg::Named(name, _) => {
                    saw_named = true;
                    let Some(index) = f.params.iter().position(|param| param.name == *name) else {
                        return Err(RuntimeError::Error(format!(
                            "function '{}' has no parameter named '{}'",
                            f.name, name
                        )));
                    };
                    if values[index].is_some() {
                        return Err(RuntimeError::Error(format!(
                            "parameter '{}' was supplied more than once",
                            name
                        )));
                    }
                    values[index] = Some(self.eval_arg(arg, &closure_env)?);
                }
            }
        }

        let call_env = closure_env.child();
        for (index, param) in f.params.iter().enumerate() {
            let value = match values[index].take() {
                Some(value) => value,
                None => match &param.default {
                    Some(default) => self.eval_expr(default, &call_env)?,
                    None => {
                        return Err(RuntimeError::Error(format!(
                            "missing required argument '{}' when calling '{}'",
                            param.name, f.name
                        )));
                    }
                },
            };
            call_env.define(&param.name, value);
        }

        self.run_function_body(f, &call_env)
    }

    fn eval_block(&mut self, block: &Block, env: &Env) -> EvalResult {
        let inner = env.child();
        for stmt in &block.stmts {
            self.before_statement(&inner, stmt.span.line)?;
            self.eval_stmt(&stmt.stmt, &inner)?;
        }
        match &block.tail {
            Some(e) => {
                // A block's last expression (no trailing statement after it)
                // is parsed as `tail`, not pushed onto `stmts` — without this,
                // a breakpoint on e.g. the single-line body of a `for` loop
                // would never fire. `parse_expr` always wraps its result in
                // `Expr::Located`, so the source line is right here.
                if let Expr::Located(_, range) = e.as_ref() {
                    self.before_statement(&inner, range.start.line)?;
                }
                self.eval_expr(e, &inner)
            }
            None => Ok(Value::Void),
        }
    }

    /// Runs right before every statement executes: keeps the top call frame's
    /// reported line/scope current for the debugger, and — only when a
    /// debugger is attached — checks whether this is a breakpoint or the
    /// target of an in-flight step request and, if so, blocks until the
    /// debugger sends a command that lets execution continue.
    fn before_statement(&mut self, env: &Env, line: usize) -> Result<(), RuntimeError> {
        if let Some(frame) = self.call_stack.last_mut() {
            frame.line = line;
            frame.current_env = env.clone();
        }
        if self.debugger.is_some() {
            self.maybe_pause(line);
        }
        if self.terminated {
            return Err(RuntimeError::Terminated);
        }
        Ok(())
    }

    fn maybe_pause(&mut self, line: usize) {
        let reason = {
            let Some(dbg) = self.debugger.as_ref() else { return };
            if dbg.pending_entry_stop {
                Some("entry")
            } else if self
                .call_stack
                .last()
                .and_then(|frame| frame.file.as_deref())
                .is_some_and(|file| dbg.breakpoints.get(file).is_some_and(|lines| lines.contains(&line)))
            {
                Some("breakpoint")
            } else {
                let depth = self.call_stack.len();
                match dbg.step {
                    StepMode::Into => Some("step"),
                    StepMode::UntilDepth if depth <= dbg.step_depth => Some("step"),
                    _ => None,
                }
            }
        };
        if let Some(reason) = reason {
            self.enter_pause(reason);
        }
    }

    /// Hands control to the debugger: sends `stopped`, then blocks reading
    /// DAP requests from stdin and answering them directly from the live
    /// interpreter state until a `continue`/step/`disconnect` command tells
    /// it to let this statement actually run.
    fn enter_pause(&mut self, reason: &str) {
        let Some(mut dbg) = self.debugger.take() else { return };
        dbg.pending_entry_stop = false;
        dbg.step = StepMode::None;
        dbg.send_event(
            "stopped",
            json!({ "reason": reason, "threadId": 1, "allThreadsStopped": true }),
        );
        loop {
            let message = match protocol::read_message(&mut *dbg.reader) {
                Ok(Some(bytes)) => bytes,
                _ => {
                    // stdin closed without a clean `disconnect` — stop the
                    // program the same way an explicit disconnect would.
                    self.terminated = true;
                    return;
                }
            };
            let Ok(value) = serde_json::from_slice::<JsonValue>(&message) else { continue };
            let command = value.get("command").and_then(JsonValue::as_str).unwrap_or_default().to_string();
            let request_seq = value.get("seq").and_then(JsonValue::as_i64).unwrap_or(0);
            let arguments = value.get("arguments").cloned().unwrap_or(JsonValue::Null);
            match command.as_str() {
                "threads" => dbg.send_response(request_seq, &command, json!({ "threads": [{ "id": 1, "name": "main" }] })),
                "stackTrace" => {
                    let body = self.dap_stack_trace();
                    dbg.send_response(request_seq, &command, body);
                }
                "scopes" => {
                    let body = dap_scopes(&arguments);
                    dbg.send_response(request_seq, &command, body);
                }
                "variables" => {
                    let body = self.dap_variables(&arguments);
                    dbg.send_response(request_seq, &command, body);
                }
                "evaluate" => {
                    let body = self.dap_evaluate(&arguments);
                    dbg.send_response(request_seq, &command, body);
                }
                "setBreakpoints" => {
                    let body = dap_set_breakpoints(&mut dbg, &arguments);
                    dbg.send_response(request_seq, &command, body);
                }
                "continue" => {
                    dbg.step = StepMode::None;
                    dbg.send_response(request_seq, &command, json!({ "allThreadsContinued": true }));
                    self.debugger = Some(dbg);
                    return;
                }
                "next" => {
                    dbg.step = StepMode::UntilDepth;
                    dbg.step_depth = self.call_stack.len();
                    dbg.send_response(request_seq, &command, json!({}));
                    self.debugger = Some(dbg);
                    return;
                }
                "stepIn" => {
                    dbg.step = StepMode::Into;
                    dbg.send_response(request_seq, &command, json!({}));
                    self.debugger = Some(dbg);
                    return;
                }
                "stepOut" => {
                    dbg.step = StepMode::UntilDepth;
                    dbg.step_depth = self.call_stack.len().saturating_sub(1);
                    dbg.send_response(request_seq, &command, json!({}));
                    self.debugger = Some(dbg);
                    return;
                }
                "pause" => dbg.send_response(request_seq, &command, json!({})),
                "disconnect" | "terminate" => {
                    dbg.send_response(request_seq, &command, json!({}));
                    self.terminated = true;
                    self.debugger = Some(dbg);
                    return;
                }
                _ => dbg.send_response(request_seq, &command, json!({})),
            }
        }
    }

    fn dap_stack_trace(&self) -> JsonValue {
        let frames: Vec<JsonValue> = self
            .call_stack
            .iter()
            .enumerate()
            .rev()
            .map(|(id, frame)| {
                json!({
                    "id": id,
                    "name": frame.name,
                    "line": frame.line,
                    "column": 1,
                    "source": frame.file.as_ref().map(|file| json!({
                        "path": file,
                        "name": std::path::Path::new(file).file_name().and_then(|n| n.to_str()).unwrap_or(file)
                    }))
                })
            })
            .collect();
        json!({ "stackFrames": frames, "totalFrames": frames.len() })
    }

    fn dap_variables(&self, arguments: &JsonValue) -> JsonValue {
        let reference = arguments.get("variablesReference").and_then(JsonValue::as_i64).unwrap_or(0);
        let frame_id = reference.saturating_sub(1).max(0) as usize;
        let Some(frame) = self.call_stack.get(frame_id) else {
            return json!({ "variables": [] });
        };
        let base_ptr = Rc::as_ptr(&frame.base_env.0) as usize;
        let mut seen = HashSet::new();
        let mut variables = Vec::new();
        let mut current = Some(frame.current_env.clone());
        while let Some(env) = current {
            let is_base = Rc::as_ptr(&env.0) as usize == base_ptr;
            let parent = {
                let borrowed = env.0.borrow();
                for (name, value) in &borrowed.vars {
                    if seen.insert(name.clone()) {
                        variables.push((name.clone(), format!("{value}")));
                    }
                }
                borrowed.parent.clone()
            };
            if is_base {
                break;
            }
            current = parent;
        }
        variables.sort_by(|a, b| a.0.cmp(&b.0));
        let variables: Vec<JsonValue> = variables
            .into_iter()
            .map(|(name, value)| json!({ "name": name, "value": value, "variablesReference": 0 }))
            .collect();
        json!({ "variables": variables })
    }

    /// Evaluates a watch/REPL expression against the paused frame's live
    /// environment — the same parser and evaluator the program itself runs
    /// on, not a separate mini-language. `self.debugger` is `None` for the
    /// duration of this call (it lives in `enter_pause`'s local `dbg`
    /// instead), so a breakpoint can't recursively trigger while evaluating
    /// a watch expression.
    fn dap_evaluate(&mut self, arguments: &JsonValue) -> JsonValue {
        let expression = arguments.get("expression").and_then(JsonValue::as_str).unwrap_or_default();
        let frame_id = arguments.get("frameId").and_then(JsonValue::as_i64);
        let env = match frame_id {
            Some(id) => self.call_stack.get(id as usize).map(|frame| frame.current_env.clone()),
            None => self.call_stack.last().map(|frame| frame.current_env.clone()),
        };
        let Some(env) = env else {
            return json!({ "result": "<no active frame>", "variablesReference": 0 });
        };
        let tokens = match crate::lexer::Lexer::new(expression).tokenize() {
            Ok(tokens) => tokens,
            Err(error) => return json!({ "result": format!("lex error: {}", error.message), "variablesReference": 0 }),
        };
        let expr = match crate::parser::Parser::new(tokens).parse_expr() {
            Ok(expr) => expr,
            Err(error) => return json!({ "result": format!("parse error: {}", error.message), "variablesReference": 0 }),
        };
        match self.eval_expr(&expr, &env) {
            Ok(value) => json!({ "result": format!("{value}"), "variablesReference": 0 }),
            Err(RuntimeError::Error(message)) => json!({ "result": format!("error: {message}"), "variablesReference": 0 }),
            Err(_) => json!({ "result": "error: control flow escaped the expression", "variablesReference": 0 }),
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
                if matches!(iter.unlocated(), Expr::Range(..)) {
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
                            None if state.borrow().closed => break,
                            None => {
                                if !self.run_one_pending_task()? {
                                    return Err(RuntimeError::Error(
                                        "channel receive would block: no runnable task remains".to_string(),
                                    ));
                                }
                            }
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
                match target.unlocated() {
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
        match expr.unlocated() {
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
            Expr::Located(inner, range) => {
                if !self.literal_kinds.is_empty() {
                    let key = crate::typeck::ExprKey {
                        file: self.call_stack.last().and_then(|frame| frame.file.clone()),
                        start: range.start,
                        end: range.end,
                    };
                    if let Some(kind) = self.literal_kinds.get(&key).copied() {
                        match (inner.as_ref(), kind) {
                            (Expr::IntLiteral(n), LitKind::Int(kind)) => return Ok(Value::Sized(*n as i128, kind)),
                            (Expr::FloatLiteral(f), LitKind::F32) => return Ok(Value::F32(*f as f32)),
                            _ => {}
                        }
                    }
                }
                self.eval_expr(inner, env)
            }
            Expr::SizedIntLiteral(n, kind) => Ok(Value::Sized(*n, *kind)),
            Expr::Float32Literal(f) => Ok(Value::F32(*f)),
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
                // A named function used as a value: a closure over its own parameters.
                if let Some(decl) = self.functions.get(name).cloned() {
                    let params: Vec<String> = decl.params.iter().map(|p| p.name.clone()).collect();
                    return Ok(Value::Closure(Rc::new(params), Rc::new(decl.body.clone()), env.clone()));
                }
                Err(RuntimeError::Error(format!("undefined name '{name}'")))
            }
            Expr::Unary(UnaryOp::Neg, e)
                if matches!(e.unlocated(), Expr::SizedIntLiteral(v, k) if k.is_signed() && *v == -k.min()) =>
            {
                let Expr::SizedIntLiteral(_, kind) = e.unlocated() else { unreachable!() };
                Ok(Value::Sized(kind.min(), *kind))
            }
            Expr::Unary(op, e) => {
                let v = self.eval_expr(e, env)?;
                match (op, &v) {
                    (UnaryOp::Neg, Value::Int(n)) => Ok(Value::Int(-n)),
                    (UnaryOp::Neg, Value::Sized(n, kind)) if kind.is_signed() && kind.fits(-n) => Ok(Value::Sized(-n, *kind)),
                    (UnaryOp::Neg, Value::Sized(_, kind)) => Err(RuntimeError::Error(format!("integer overflow: cannot negate this {}", kind.name()))),
                    (UnaryOp::Neg, Value::Float(n)) => Ok(Value::Float(-n)),
                    (UnaryOp::Neg, Value::F32(n)) => Ok(Value::F32(-n)),
                    (UnaryOp::Neg, Value::Record(..) | Value::EnumInstance(..)) => match self.find_method_for_value(&v, "neg") {
                        Some(f) => self.call_user_function(&f, vec![v.clone()], env.clone()),
                        None => Err(RuntimeError::Error(format!("'{}' has no 'neg' method", value_type_name(&v)))),
                    },
                    (UnaryOp::Neg, Value::Array(_)) => array::negate(&v),
                    (UnaryOp::Not, Value::Array(_)) => array::not_array(&v),
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
            Expr::Index(obj, idx) if matches!(idx.unlocated(), Expr::Range(_, _, _, None)) => {
                // `a[lo until hi]` / `a[lo to hi]` on an array: a slice, not a list of indices.
                let ov = self.eval_expr(obj, env)?;
                let Expr::Range(start, kind, end, _) = idx.unlocated() else { unreachable!() };
                let lo = as_i64(&self.eval_expr(start, env)?)?;
                let hi = as_i64(&self.eval_expr(end, env)?)? + if *kind == RangeKind::To { 1 } else { 0 };
                match ov {
                    Value::Array(a) => array::slice(&a, lo, hi),
                    other => Err(RuntimeError::Error(format!("cannot slice '{other}'"))),
                }
            }
            Expr::Index(obj, idx) => {
                let ov = self.eval_expr(obj, env)?;
                let index_value = self.eval_expr(idx, env)?;
                if let (Value::Array(a), Value::Array(_)) = (&ov, &index_value) {
                    return array::index_mask(a, &index_value);
                }
                let iv = as_i64(&index_value)?;
                match ov {
                    Value::List(state) => state
                        .borrow()
                        .get(iv as usize)
                        .cloned()
                        .ok_or_else(|| RuntimeError::Error(format!("index out of bounds: {iv}"))),
                    Value::Array(a) => array::index1(&a, iv),
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
                let state = Rc::new(RefCell::new(SetState::new(values)));
                self.reindex_set(&state);
                Ok(Value::Set(state))
            }
            Expr::EmptyCollection(name, _) => {
                if name == "Map" {
                    let state = Rc::new(RefCell::new(MapState::new(Vec::new())));
                    self.reindex_map(&state);
                    Ok(Value::Map(state))
                } else {
                    let state = Rc::new(RefCell::new(SetState::new(Vec::new())));
                    self.reindex_set(&state);
                    Ok(Value::Set(state))
                }
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
                let state = Rc::new(RefCell::new(MapState::new(values)));
                self.reindex_map(&state);
                Ok(Value::Map(state))
            }
            Expr::Try(inner, catch) => {
                let value = self.eval_expr(inner, env)?;
                match value {
                    Value::EnumInstance(enum_name, variant, fields, type_args)
                        if enum_name == "Option" =>
                    {
                        match variant.as_str() {
                            "Some" => Ok(fields.get("0").cloned().unwrap_or(Value::Void)),
                            "None" => Err(RuntimeError::Return(Value::EnumInstance(
                                enum_name,
                                variant,
                                fields,
                                type_args,
                            ))),
                            _ => Err(RuntimeError::Error("invalid Option variant".to_string())),
                        }
                    }
                    Value::EnumInstance(enum_name, variant, fields, _type_args)
                        if enum_name == "Result" =>
                    {
                        match variant.as_str() {
                            "Ok" => Ok(fields.get("0").cloned().unwrap_or(Value::Void)),
                            "Err" => {
                                let error = fields.get("0").cloned().unwrap_or(Value::Void);
                                let propagated = if let Some(catch_expr) = catch {
                                    let handler = self.eval_expr(catch_expr, env)?;
                                    self.call_callable(handler, vec![error], env)?
                                } else {
                                    error
                                };
                                Err(RuntimeError::Return(err_value(propagated)))
                            }
                            _ => Err(RuntimeError::Error("invalid Result variant".to_string())),
                        }
                    }
                    other => Err(RuntimeError::Error(format!(
                        "'try' expects Option or Result, got '{other}'"
                    ))),
                }
            }
            Expr::Within(a, r) => {
                let av = self.eval_expr(a, env)?;
                if let Expr::Range(start, kind, end, _) = r.as_ref().unlocated() {
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
            Expr::As(e, unit_expr)
                if matches!(unit_expr.as_ref().unlocated(), Expr::Ident(sym) if matches!(sym.as_str(), "Int" | "Int64" | "Float" | "Float64" | "Float32") || IntKind::from_name(sym).is_some()) =>
            {
                let value = self.eval_expr(e, env)?;
                let Expr::Ident(target) = unit_expr.as_ref().unlocated() else { unreachable!() };
                convert_numeric(value, target)
            }
            Expr::As(e, unit_expr) => {
                let v = as_f64(&self.eval_expr(e, env)?)?;
                if let Expr::Ident(sym) = unit_expr.as_ref().unlocated() {
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
            Expr::Spawn(block) => {
                let task = Rc::new(RefCell::new(TaskState {
                    body: Rc::new(block.clone()),
                    env: env.clone(),
                    status: TaskStatus::Pending,
                    result: None,
                }));
                self.tasks.push(task.clone());
                Ok(Value::Task(task))
            }
            Expr::SpawnScope(block) => {
                let first_task = self.tasks.len();
                let body_result = self.eval_block(block, env);
                let drain_result = self.drain_tasks_from(first_task);
                match body_result {
                    Ok(value) => {
                        drain_result?;
                        Ok(value)
                    }
                    Err(error) => {
                        // A structured scope still gives its children a chance
                        // to finish before propagating the body's control flow.
                        let _ = drain_result;
                        Err(error)
                    }
                }
            }
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
                    match &mut self.debugger {
                        Some(dbg) => dbg.send_output(&format!("{v}\n")),
                        None => println!("{v}"),
                    }
                    return Ok(Value::Void);
                }
                "args" => {
                    let values = std::env::args()
                        .skip_while(|value| value != "--")
                        .skip(1)
                        .map(Value::String)
                        .collect();
                    return Ok(Value::List(Rc::new(RefCell::new(values))));
                }
                "env" => {
                    let key = self.eval_arg(&args[0], env)?;
                    let Value::String(key) = key else {
                        return Err(RuntimeError::Error("'env' expects a String name".to_string()));
                    };
                    return Ok(match std::env::var(key) {
                        Ok(value) => some_value(Value::String(value)),
                        Err(_) => none_value(),
                    });
                }
                "path_join" => {
                    let left = self.eval_arg(&args[0], env)?;
                    let right = self.eval_arg(&args[1], env)?;
                    let (Value::String(left), Value::String(right)) = (left, right) else {
                        return Err(RuntimeError::Error("'path_join' expects two String arguments".to_string()));
                    };
                    let left = left.trim_end_matches(|c| c == '/' || c == '\\');
                    let right = right.trim_start_matches(|c| c == '/' || c == '\\');
                    return Ok(Value::String(match (left.is_empty(), right.is_empty()) {
                        (true, _) => right.to_string(),
                        (_, true) => left.to_string(),
                        _ => format!("{left}/{right}"),
                    }));
                }
                "cwd" => {
                    return std::env::current_dir()
                        .map(|path| Value::String(path.to_string_lossy().into_owned()))
                        .map_err(|error| RuntimeError::Error(format!("'cwd' failed: {error}")));
                }
                "file_exists" => {
                    let path = self.eval_arg(&args[0], env)?;
                    let Value::String(path) = path else {
                        return Err(RuntimeError::Error("'file_exists' expects a String path".to_string()));
                    };
                    return Ok(Value::Bool(std::path::Path::new(&path).is_file()));
                }
                "hash" => {
                    let value = self.eval_arg(&args[0], env)?;
                    let hash = self.hash_value(&value).ok_or_else(|| {
                        RuntimeError::Error("'hash' supports scalar values, hashable Option/Result/collection values, or records/enums with derive(Hash)".to_string())
                    })?;
                    return Ok(Value::Int(hash as i64));
                }
                "format" => {
                    let template = self.eval_arg(&args[0], env)?;
                    let values = self.eval_arg(&args[1], env)?;
                    let (Value::String(template), Value::List(values)) = (template, values) else {
                        return Err(RuntimeError::Error("'format' expects a String and a List<String>".to_string()));
                    };
                    let values = values.borrow().clone();
                    let mut rendered = String::with_capacity(template.len());
                    let mut chars = template.chars().peekable();
                    let mut index = 0usize;
                    while let Some(ch) = chars.next() {
                        if ch == '{' && chars.peek() == Some(&'}') {
                            chars.next();
                            let Some(Value::String(value)) = values.get(index) else {
                                return Err(RuntimeError::Error(format!("'format' needs a value for placeholder {index}")));
                            };
                            rendered.push_str(value);
                            index += 1;
                        } else {
                            rendered.push(ch);
                        }
                    }
                    return Ok(Value::String(rendered));
                }
                "select" => {
                    let channels = self.eval_arg(&args[0], env)?;
                    let Value::List(channels) = channels else {
                        return Err(RuntimeError::Error("'select' expects a List<Channel<T>>".to_string()));
                    };
                    let channels = channels.borrow().clone();
                    if channels.is_empty() {
                        return Err(RuntimeError::Error("'select' expects at least one channel".to_string()));
                    }
                    loop {
                        for channel in &channels {
                            let Value::Channel(state) = channel else {
                                return Err(RuntimeError::Error("'select' expects a List<Channel<T>>".to_string()));
                            };
                            let mut state = state.borrow_mut();
                            if let Some(value) = state.queue.pop_front() {
                                return Ok(some_value(value));
                            }
                            if state.closed {
                                return Ok(none_value());
                            }
                        }
                        if !self.run_one_pending_task()? {
                            return Err(RuntimeError::Error(
                                "select would block: no runnable task remains".to_string(),
                            ));
                        }
                    }
                }
                // The interpreter already uses Rc-backed identity for
                // records and collections. Cloning a Value therefore creates
                // the same logical alias as native retain; drop is a
                // deliberate no-op here because Rust releases the temporary
                // Value at the end of this call.
                "clone" => {
                    return Ok(self.eval_arg(&args[0], env)?);
                }
                "drop" => {
                    let _ = self.eval_arg(&args[0], env)?;
                    return Ok(Value::Void);
                }
                "sum" => {
                    let v = self.eval_arg(&args[0], env)?;
                    return match v {
                        Value::List(state) => sum_values(&state.borrow()),
                        other => Err(RuntimeError::Error(format!("'sum' expects a List, got '{other}'"))),
                    };
                }
                "read_file" => {
                    let path = self.eval_arg(&args[0], env)?;
                    let Value::String(path) = path else {
                        return Err(RuntimeError::Error("'read_file' expects a String path".to_string()));
                    };
                    return Ok(match fs::read_to_string(&path) {
                        Ok(contents) => ok_value(Value::String(contents)),
                        Err(error) => err_value(Value::String(error.to_string())),
                    });
                }
                "write_file" => {
                    let path = self.eval_arg(&args[0], env)?;
                    let contents = self.eval_arg(&args[1], env)?;
                    let (Value::String(path), Value::String(contents)) = (path, contents) else {
                        return Err(RuntimeError::Error(
                            "'write_file' expects a String path and String contents".to_string(),
                        ));
                    };
                    return Ok(match fs::write(&path, contents) {
                        Ok(()) => ok_value(Value::Void),
                        Err(error) => err_value(Value::String(error.to_string())),
                    });
                }
                "parse_csv" => {
                    let text = self.eval_arg(&args[0], env)?;
                    let Value::String(text) = text else {
                        return Err(RuntimeError::Error("'parse_csv' expects a String".to_string()));
                    };
                    return Ok(strings::csv_value(&text));
                }
                "parse_int" => {
                    let text = self.eval_arg(&args[0], env)?;
                    let Value::String(text) = text else {
                        return Err(RuntimeError::Error("'parse_int' expects a String".to_string()));
                    };
                    return Ok(match text.parse::<i64>() {
                        Ok(value) => ok_value(Value::Int(value)),
                        Err(error) => err_value(Value::String(error.to_string())),
                    });
                }
                "panic" => {
                    let message = self.eval_arg(&args[0], env)?;
                    return Err(RuntimeError::Error(format!("panic: {message}")));
                }
                "assert" => {
                    let condition = self.eval_arg(&args[0], env)?;
                    if !truthy(&condition) {
                        return Err(RuntimeError::Error("assertion failed".to_string()));
                    }
                    return Ok(Value::Void);
                }
                "assert_eq" => {
                    let left = self.eval_arg(&args[0], env)?;
                    let right = self.eval_arg(&args[1], env)?;
                    let equal = self.eval_binary(BinOp::Eq, left.clone(), right.clone(), env)?;
                    if !truthy(&equal) {
                        return Err(RuntimeError::Error(format!("assertion failed: left = {left}, right = {right}")));
                    }
                    return Ok(Value::Void);
                }
                _ => {}
            }
            if let Some(enum_name) = self.variant_to_enum.get(name).cloned() {
                return self.construct_variant(&enum_name, name, args, env, explicit_type_args);
            }
            if let Some(f) = self.functions.get(name).cloned() {
                return self.call_user_function_with_args(&f, args, env.clone());
            }
            // Regression / distributions on Array<Float> (a user function of the same name wins, above).
            if regress::is_regress(name, args.len()) {
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    values.push(self.eval_arg(arg, env)?);
                }
                return regress::call(name, &values);
            }
            // Math builtins (a user function of the same name wins, above).
            if math::is_math(name, args.len()) {
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    values.push(self.eval_arg(arg, env)?);
                }
                return math::call(name, &values);
            }
            // Array constructors (a user function of the same name wins, above).
            match (name.as_str(), args.len()) {
                ("rng", 1) => {
                    let seed = as_i64(&self.eval_arg(&args[0], env)?)?;
                    return Ok(Value::Rng(Rc::new(RefCell::new(rng::RngState::new(seed)))));
                }
                ("where", 3) => {
                    let mask = self.eval_arg(&args[0], env)?;
                    let a = self.eval_arg(&args[1], env)?;
                    let b = self.eval_arg(&args[2], env)?;
                    return array::where_select(&mask, &a, &b);
                }
                ("cov", 2) | ("corr", 2) => {
                    let a = self.eval_arg(&args[0], env)?;
                    let b = self.eval_arg(&args[1], env)?;
                    return array::cov_corr(name, &a, &b);
                }
                ("array", 1) => {
                    let list = self.eval_arg(&args[0], env)?;
                    return array::from_list(&list);
                }
                ("zeros", 1) | ("ones", 1) => {
                    let shape = self.eval_arg(&args[0], env)?;
                    return array::full(&shape, Value::Float(if name == "ones" { 1.0 } else { 0.0 }));
                }
                ("full", 2) => {
                    let shape = self.eval_arg(&args[0], env)?;
                    let value = self.eval_arg(&args[1], env)?;
                    return array::full(&shape, value);
                }
                ("arange", 2) => {
                    let start = as_i64(&self.eval_arg(&args[0], env)?)?;
                    let stop = as_i64(&self.eval_arg(&args[1], env)?)?;
                    return array::arange(start, stop);
                }
                ("linspace", 3) => {
                    let a = as_f64(&self.eval_arg(&args[0], env)?)?;
                    let b = as_f64(&self.eval_arg(&args[1], env)?)?;
                    let n = as_i64(&self.eval_arg(&args[2], env)?)?;
                    return array::linspace(a, b, n);
                }
                _ => {}
            }
        }
        if let Expr::FieldAccess(obj, method) = callee.unlocated() {
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
                    "join" => {
                        let sep = self.eval_arg(&args[0], env)?;
                        let Value::String(sep) = sep else {
                            return Err(RuntimeError::Error("'join' expects a String separator".to_string()));
                        };
                        return strings::join(&state.borrow(), &sep);
                    }
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
                        if let Some(index) = self.map_find(state, &k, env)? {
                            return Ok(some_value(state.borrow().entries[index].1.clone()));
                        }
                        return Ok(none_value());
                    }
                    "contains_key" => {
                        let k = self.eval_arg(&args[0], env)?;
                        return Ok(Value::Bool(self.map_find(state, &k, env)?.is_some()));
                    }
                    "keys" => return Ok(Value::List(Rc::new(RefCell::new(state.borrow().iter().map(|(k, _)| k.clone()).collect())))),
                    "values" => return Ok(Value::List(Rc::new(RefCell::new(state.borrow().iter().map(|(_, v)| v.clone()).collect())))),
                    "count" => return Ok(Value::Int(state.borrow().len() as i64)),
                    "set" => {
                        let k = self.eval_arg(&args[0], env)?;
                        let v = self.eval_arg(&args[1], env)?;
                        match self.map_find(state, &k, env)? {
                            Some(i) => state.borrow_mut().entries[i] = (k, v),
                            None => state.borrow_mut().entries.push((k, v)),
                        }
                        self.reindex_map(state);
                        return Ok(Value::Void);
                    }
                    "remove" => {
                        let k = self.eval_arg(&args[0], env)?;
                        let Some(index) = self.map_find(state, &k, env)? else { return Ok(none_value()) };
                        let value = state.borrow().entries[index].1.clone();
                        state.borrow_mut().entries.remove(index);
                        self.reindex_map(state);
                        return Ok(some_value(value));
                    }
                    _ => {}
                }
            }
            if let Value::String(text) = &receiver {
                if strings::STRING_METHODS.contains(&method.as_str()) {
                    let mut values = Vec::with_capacity(args.len());
                    for arg in args {
                        values.push(self.eval_arg(arg, env)?);
                    }
                    if let Some(result) = strings::call_method(text, method, &values) {
                        return result;
                    }
                }
            }
            if let Value::Rng(state) = &receiver {
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    values.push(self.eval_arg(arg, env)?);
                }
                return rng::call_method(&mut state.borrow_mut(), method, &values);
            }
            if let Value::Array(state) = &receiver {
                let mut values = Vec::with_capacity(args.len());
                for arg in args {
                    values.push(self.eval_arg(arg, env)?);
                }
                return array::call_method(state, method, values);
            }
            if let Value::Set(state) = &receiver {
                match method.as_str() {
                    "contains" => {
                        let x = self.eval_arg(&args[0], env)?;
                        return Ok(Value::Bool(self.set_find(state, &x, env)?.is_some()));
                    }
                    "count" => return Ok(Value::Int(state.borrow().len() as i64)),
                    "add" => {
                        let x = self.eval_arg(&args[0], env)?;
                        if self.set_find(state, &x, env)?.is_none() {
                            state.borrow_mut().entries.push(x);
                            self.reindex_set(state);
                        }
                        return Ok(Value::Void);
                    }
                    "remove" => {
                        let x = self.eval_arg(&args[0], env)?;
                        if let Some(index) = self.set_find(state, &x, env)? {
                            state.borrow_mut().entries.remove(index);
                            self.reindex_set(state);
                        }
                        return Ok(Value::Void);
                    }
                    _ => {}
                }
            }
            if let Value::EnumInstance(enum_name, variant, fields, _) = &receiver {
                if enum_name == "Option" {
                    match method.as_str() {
                        "is_some" => return Ok(Value::Bool(variant == "Some")),
                        "is_none" => return Ok(Value::Bool(variant == "None")),
                        "unwrap" => {
                            return fields.get("0").cloned().ok_or_else(|| {
                                RuntimeError::Error("called Option.unwrap() on None".to_string())
                            });
                        }
                        "unwrap_or" => {
                            return match fields.get("0").cloned() {
                                Some(value) => Ok(value),
                                None => self.eval_arg(&args[0], env),
                            };
                        }
                        "ok_or" => {
                            return match fields.get("0").cloned() {
                                Some(value) => Ok(ok_value(value)),
                                None => Ok(err_value(self.eval_arg(&args[0], env)?)),
                            };
                        }
                        "map" => {
                            if let Some(value) = fields.get("0").cloned() {
                                let f = self.eval_arg(&args[0], env)?;
                                return Ok(some_value(self.call_callable(f, vec![value], env)?));
                            }
                            return Ok(none_value());
                        }
                        "then" => {
                            if let Some(value) = fields.get("0").cloned() {
                                let f = self.eval_arg(&args[0], env)?;
                                return self.call_callable(f, vec![value], env);
                            }
                            return Ok(none_value());
                        }
                        _ => {}
                    }
                }
                if enum_name == "Result" {
                    match method.as_str() {
                        "is_ok" => return Ok(Value::Bool(variant == "Ok")),
                        "is_err" => return Ok(Value::Bool(variant == "Err")),
                        "unwrap" => {
                            return fields.get("0").cloned().ok_or_else(|| {
                                RuntimeError::Error("called Result.unwrap() on Err".to_string())
                            });
                        }
                        "unwrap_or" => {
                            return match variant.as_str() {
                                "Ok" => Ok(fields.get("0").cloned().unwrap_or(Value::Void)),
                                _ => self.eval_arg(&args[0], env),
                            };
                        }
                        "ok" => {
                            return match variant.as_str() {
                                "Ok" => Ok(some_value(fields.get("0").cloned().unwrap_or(Value::Void))),
                                _ => Ok(none_value()),
                            };
                        }
                        "map" => {
                            if variant == "Ok" {
                                let f = self.eval_arg(&args[0], env)?;
                                let value = fields.get("0").cloned().unwrap_or(Value::Void);
                                return Ok(ok_value(self.call_callable(f, vec![value], env)?));
                            }
                            return Ok(receiver.clone());
                        }
                        "map_err" => {
                            if variant == "Err" {
                                let f = self.eval_arg(&args[0], env)?;
                                let error = fields.get("0").cloned().unwrap_or(Value::Void);
                                return Ok(err_value(self.call_callable(f, vec![error], env)?));
                            }
                            return Ok(receiver.clone());
                        }
                        "then" => {
                            if variant == "Ok" {
                                let f = self.eval_arg(&args[0], env)?;
                                let value = fields.get("0").cloned().unwrap_or(Value::Void);
                                return self.call_callable(f, vec![value], env);
                            }
                            return Ok(receiver.clone());
                        }
                        _ => {}
                    }
                }
            }
            if let Value::Task(result) = &receiver {
                if method == "join" {
                    return self.run_task(result.clone());
                }
            }
            if let Value::Channel(state) = &receiver {
                match method.as_str() {
                    "send" => {
                        if state.borrow().closed {
                            return Err(RuntimeError::Error("cannot send on a closed channel".to_string()));
                        }
                        let v = self.eval_arg(&args[0], env)?;
                        if let Value::Record(name, data) = &v {
                            if self.movable_types.contains(name) {
                                self.moved.insert(Rc::as_ptr(data) as usize);
                            }
                        }
                        state.borrow_mut().queue.push_back(v);
                        return Ok(Value::Void);
                    }
                    "receive" => {
                        loop {
                            let popped = state.borrow_mut().queue.pop_front();
                            if let Some(v) = popped {
                                return Ok(Value::EnumInstance(
                                    "Option".to_string(),
                                    "Some".to_string(),
                                    HashMap::from([("0".to_string(), v)]),
                                    Vec::new(),
                                ));
                            }
                            if state.borrow().closed {
                                return Ok(Value::EnumInstance(
                                    "Option".to_string(),
                                    "None".to_string(),
                                    HashMap::new(),
                                    Vec::new(),
                                ));
                            }
                            if !self.run_one_pending_task()? {
                                return Err(RuntimeError::Error(
                                    "channel receive would block: no runnable task remains".to_string(),
                                ));
                            }
                        }
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
        Value::Sized(_, kind) => kind.name().to_string(),
        Value::F32(_) => "Float32".to_string(),
        Value::Array(_) => "Array".to_string(),
        Value::Rng(_) => "Rng".to_string(),
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

fn ok_value(v: Value) -> Value {
    Value::EnumInstance("Result".to_string(), "Ok".to_string(), HashMap::from([("0".to_string(), v)]), Vec::new())
}

fn err_value(v: Value) -> Value {
    Value::EnumInstance("Result".to_string(), "Err".to_string(), HashMap::from([("0".to_string(), v)]), Vec::new())
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
        Value::F32(n) => Ok(*n as i64),
        Value::Sized(n, _) => i64::try_from(*n).map_err(|_| RuntimeError::Error(format!("integer {n} does not fit in Int"))),
        Value::Float(n) => Ok(*n as i64),
        other => Err(RuntimeError::Error(format!("expected a number, got '{other}'"))),
    }
}

fn as_f64(v: &Value) -> Result<f64, RuntimeError> {
    match v {
        Value::Int(n) => Ok(*n as f64),
        Value::Sized(n, _) => Ok(*n as f64),
        Value::F32(n) => Ok(*n as f64),
        Value::Float(n) => Ok(*n),
        Value::Quantity(n, _, _) => Ok(*n),
        other => Err(RuntimeError::Error(format!("expected a number, got '{other}'"))),
    }
}

/// Arithmetic and comparison on fixed-width integers: never mixes kinds,
/// and every result must fit its type (overflow is a runtime error, not wrap-around).
fn sized_binary(op: BinOp, lv: Value, rv: Value) -> EvalResult {
    use BinOp::*;
    if matches!(op, And | Or) {
        return Ok(Value::Bool(if op == And { truthy(&lv) && truthy(&rv) } else { truthy(&lv) || truthy(&rv) }));
    }
    let (a, b, kind) = match (&lv, &rv) {
        (Value::Sized(a, k1), Value::Sized(b, k2)) if k1 == k2 => (*a, *b, *k1),
        (Value::Sized(a, k), Value::Int(b)) => (*a, *b as i128, *k),
        (Value::Int(a), Value::Sized(b, k)) => (*a as i128, *b, *k),
        _ => return Err(RuntimeError::Error(format!("mismatched integer types: '{lv}' and '{rv}'"))),
    };
    let overflow = |what: &str| RuntimeError::Error(format!("integer overflow: {a} {what} {b} does not fit in {}", kind.name()));
    let checked = |value: Option<i128>, what: &str| match value {
        Some(v) if kind.fits(v) => Ok(Value::Sized(v, kind)),
        _ => Err(overflow(what)),
    };
    match op {
        Add => checked(a.checked_add(b), "+"),
        Sub => checked(a.checked_sub(b), "-"),
        Mul => checked(a.checked_mul(b), "*"),
        Div => {
            if b == 0 {
                Err(RuntimeError::Error("division by zero".to_string()))
            } else {
                checked(a.checked_div(b), "/")
            }
        }
        Eq => Ok(Value::Bool(a == b)),
        NotEq => Ok(Value::Bool(a != b)),
        Lt => Ok(Value::Bool(a < b)),
        Gt => Ok(Value::Bool(a > b)),
        LtEq => Ok(Value::Bool(a <= b)),
        GtEq => Ok(Value::Bool(a >= b)),
        And | Or => unreachable!(),
    }
}

/// `x as UInt8` / `as Int` / `as Float`: an explicit, range-checked conversion.
fn convert_numeric(value: Value, target: &str) -> EvalResult {
    // A `Float32` converts exactly like the `Float` holding the same value.
    let value = match value {
        Value::F32(f) if target != "Float32" => Value::Float(f as f64),
        other => other,
    };
    if target == "Float32" {
        return match &value {
            Value::Int(n) => Ok(Value::F32(*n as f32)),
            Value::Sized(n, _) => Ok(Value::F32(*n as f32)),
            Value::Float(f) => Ok(Value::F32(*f as f32)),
            Value::F32(f) => Ok(Value::F32(*f)),
            other => Err(RuntimeError::Error(format!("cannot convert '{other}' to Float32"))),
        };
    }
    let integer: Option<i128> = match &value {
        Value::Int(n) => Some(*n as i128),
        Value::Sized(n, _) => Some(*n),
        Value::Float(f) => {
            if !f.is_finite() {
                return Err(RuntimeError::Error(format!("cannot convert {f} to {target}")));
            }
            if target == "Float" {
                None
            } else {
                Some(f.trunc() as i128)
            }
        }
        other => return Err(RuntimeError::Error(format!("cannot convert '{other}' to {target}"))),
    };
    match target {
        "Float" | "Float64" => Ok(Value::Float(match &value {
            Value::Float(f) => *f,
            _ => integer.unwrap() as f64,
        })),
        "Int" | "Int64" => {
            let n = integer.unwrap();
            i64::try_from(n).map(Value::Int).map_err(|_| RuntimeError::Error(format!("value {n} does not fit in Int")))
        }
        other => {
            let kind = IntKind::from_name(other).expect("checked by the caller");
            let n = integer.unwrap();
            if kind.fits(n) {
                Ok(Value::Sized(n, kind))
            } else {
                Err(RuntimeError::Error(format!("value {n} does not fit in {}", kind.name())))
            }
        }
    }
}

/// Single-precision arithmetic: both operands must be `Float32` (the checker
/// guarantees it), and every operation rounds to `f32`.
fn f32_binary(op: BinOp, lv: Value, rv: Value) -> EvalResult {
    use BinOp::*;
    if matches!(op, And | Or) {
        return Ok(Value::Bool(if op == And { truthy(&lv) && truthy(&rv) } else { truthy(&lv) || truthy(&rv) }));
    }
    let (Value::F32(a), Value::F32(b)) = (&lv, &rv) else {
        return Err(RuntimeError::Error(format!("mismatched float types: '{lv}' and '{rv}'")));
    };
    let (a, b) = (*a, *b);
    Ok(match op {
        Add => Value::F32(a + b),
        Sub => Value::F32(a - b),
        Mul => Value::F32(a * b),
        Div => Value::F32(a / b),
        Eq => Value::Bool(a == b),
        NotEq => Value::Bool(a != b),
        Lt => Value::Bool(a < b),
        Gt => Value::Bool(a > b),
        LtEq => Value::Bool(a <= b),
        GtEq => Value::Bool(a >= b),
        And | Or => unreachable!(),
    })
}

fn eval_binary_builtin(op: BinOp, lv: Value, rv: Value) -> EvalResult {
    use BinOp::*;
    if matches!(lv, Value::Array(_)) || matches!(rv, Value::Array(_)) {
        return array::binary(op, lv, rv);
    }
    if matches!(lv, Value::Sized(..)) || matches!(rv, Value::Sized(..)) {
        return sized_binary(op, lv, rv);
    }
    if matches!(lv, Value::F32(_)) || matches!(rv, Value::F32(_)) {
        return f32_binary(op, lv, rv);
    }
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
            // `Int / Int` is integer division (truncating), matching the type
            // checker, which types it `Int` — the runtime used to return a
            // `Float` here, contradicting the static type.
            (Value::Int(a), Value::Int(b)) => {
                if op == Mul {
                    Ok(Value::Int(a * b))
                } else if *b == 0 {
                    Err(RuntimeError::Error("division by zero".to_string()))
                } else {
                    Ok(Value::Int(a.wrapping_div(*b)))
                }
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
