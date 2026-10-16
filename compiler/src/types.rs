use std::collections::HashMap;

pub type Dimension = HashMap<String, i32>;

/// Conversion metadata for a unit. `offset` maps an absolute point to
/// its coherent value; linear quantities such as intervals ignore it.
#[derive(Debug, Clone, PartialEq)]
pub struct UnitDefinition {
    pub dimension: Dimension,
    pub scale: f64,
    pub offset: f64,
    /// Affine space for units with an absolute-point origin, such as a
    /// temperature scale. Kelvin carries a Temperature-space marker while
    /// retaining zero offset and linear arithmetic for existing quantities.
    pub affine_space: Option<String>,
}

impl UnitDefinition {
    pub fn linear(dimension: Dimension, scale: f64) -> Self {
        Self {
            dimension,
            scale,
            offset: 0.0,
            affine_space: None,
        }
    }

    #[allow(dead_code)] // Activated when the parser and backends expose affine units.
    pub fn affine(
        dimension: Dimension,
        scale: f64,
        offset: f64,
        affine_space: impl Into<String>,
    ) -> Result<Self, String> {
        let affine_space = affine_space.into();
        if !scale.is_finite() || scale <= 0.0 || !offset.is_finite() || affine_space.is_empty() {
            return Err(
                "an affine unit needs a finite positive scale, finite offset and named space"
                    .to_string(),
            );
        }
        Ok(Self {
            dimension,
            scale,
            offset,
            affine_space: Some(affine_space),
        })
    }
}

/// Semantic role of a quantity during conversion. `Linear` also covers
/// temperature intervals; `Point` is used only for explicitly typed absolute
/// readings. This role is independent of the unit's dimension and spelling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuantityRole {
    Linear,
    #[allow(dead_code)] // Constructed by typed absolute-temperature lowering.
    Point(String),
}

pub fn dim_single(name: &str) -> Dimension {
    let mut d = HashMap::new();
    d.insert(name.to_string(), 1);
    d
}

pub fn dim_mul(a: &Dimension, b: &Dimension) -> Dimension {
    let mut result = a.clone();
    for (k, v) in b {
        *result.entry(k.clone()).or_insert(0) += v;
    }
    result.retain(|_, v| *v != 0);
    result
}

pub fn dim_div(a: &Dimension, b: &Dimension) -> Dimension {
    let mut result = a.clone();
    for (k, v) in b {
        *result.entry(k.clone()).or_insert(0) -= v;
    }
    result.retain(|_, v| *v != 0);
    result
}

pub fn dim_pow(a: &Dimension, exp: i32) -> Dimension {
    let mut result = HashMap::new();
    for (k, v) in a {
        result.insert(k.clone(), v * exp);
    }
    result.retain(|_, v| *v != 0);
    result
}

pub fn dim_is_dimensionless(d: &Dimension) -> bool {
    d.values().all(|v| *v == 0)
}

pub fn dim_to_string(d: &Dimension) -> String {
    if d.is_empty() {
        return "Dimensionless".to_string();
    }
    let mut parts: Vec<String> = d
        .iter()
        .map(|(k, v)| {
            if *v == 1 {
                k.clone()
            } else {
                format!("{k}^{v}")
            }
        })
        .collect();
    parts.sort();
    parts.join("*")
}

fn dim_of(parts: &[(&str, i32)]) -> Dimension {
    parts
        .iter()
        .filter(|(_, e)| *e != 0)
        .map(|(k, e)| (k.to_string(), *e))
        .collect()
}

// Units and dimensions declared by the program (`unit`, `dimension`, `define`,
// document 01 §3.5). They are registered while parsing, before any expression
// uses them, and live for one compilation (`reset_user_units`).
thread_local! {
    static USER_UNITS: std::cell::RefCell<Vec<(String, Dimension, f64)>> = const { std::cell::RefCell::new(Vec::new()) };
    static USER_DIMENSIONS: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

pub fn reset_user_units() {
    USER_UNITS.with(|u| u.borrow_mut().clear());
    USER_DIMENSIONS.with(|d| d.borrow_mut().clear());
}

pub fn is_user_dimension(name: &str) -> bool {
    USER_DIMENSIONS.with(|d| d.borrow().iter().any(|n| n == name))
}

/// `dimension Money`. Declaring the same dimension twice is harmless.
pub fn declare_dimension(name: &str) -> Result<(), String> {
    if named_dimension(name).is_some()
        || matches!(
            name,
            "Length"
                | "Mass"
                | "Time"
                | "Temperature"
                | "ElectricCurrent"
                | "AmountOfSubstance"
                | "LuminousIntensity"
                | "Currency"
                | "Information"
        )
    {
        return Err(format!("dimension '{name}' already exists"));
    }
    USER_DIMENSIONS.with(|d| {
        let mut d = d.borrow_mut();
        if !d.iter().any(|n| n == name) {
            d.push(name.to_string());
        }
    });
    Ok(())
}

/// `unit mmHg : Pressure`: a new unit symbol of that dimension, with factor 1
/// towards the coherent unit until a `define` gives it another.
pub fn declare_unit(symbol: &str, dim: Dimension) -> Result<(), String> {
    if builtin_unit_info(symbol).is_some() {
        return Err(format!(
            "unit '{symbol}' is already defined by the standard catalog"
        ));
    }
    USER_UNITS.with(|u| {
        let mut u = u.borrow_mut();
        match u.iter().find(|(s, _, _)| s == symbol) {
            Some((_, existing, _)) if *existing != dim => Err(format!(
                "unit '{symbol}' was already declared with another dimension"
            )),
            Some(_) => Ok(()),
            None => {
                u.push((symbol.to_string(), dim, 1.0));
                Ok(())
            }
        }
    })
}

/// `define 1 EUR = 1.08 USD`: the factor of a declared unit.
pub fn define_unit(symbol: &str, factor: f64) -> Result<(), String> {
    USER_UNITS.with(|u| {
        let mut u = u.borrow_mut();
        match u.iter_mut().find(|(s, _, _)| s == symbol) {
            Some(entry) => {
                entry.2 = factor;
                Ok(())
            }
            None if builtin_unit_info(symbol).is_some() => Err(format!(
                "'{symbol}' is a standard unit; 'define' only sets units declared with 'unit'"
            )),
            None => Err(format!(
                "unknown unit '{symbol}' in 'define' (declare it with 'unit {symbol} : Dimension')"
            )),
        }
    })
}

/// Declared units in declaration order, for the native runtime's table.
pub fn user_units() -> Vec<(String, Dimension, f64)> {
    USER_UNITS.with(|u| u.borrow().clone())
}

/// Full unit conversion metadata. Existing built-in and user-defined units
/// remain linear; an affine unit must be registered with its offset and space
/// before source syntax can expose it.
pub fn unit_definition(symbol: &str) -> Option<UnitDefinition> {
    builtin_unit_info(symbol)
        .map(|(dimension, scale)| {
            let mut definition = UnitDefinition::linear(dimension, scale);
            if symbol == "K" {
                definition.affine_space = Some("Temperature".to_string());
            }
            definition
        })
        .or_else(|| {
            USER_UNITS.with(|u| {
                u.borrow()
                    .iter()
                    .find(|(s, _, _)| s == symbol)
                    .map(|(_, dimension, scale)| UnitDefinition::linear(dimension.clone(), *scale))
            })
        })
}

pub fn unit_info(symbol: &str) -> Option<(Dimension, f64)> {
    unit_definition(symbol).map(|unit| (unit.dimension, unit.scale))
}

/// Convert a numerical value while keeping its semantic role explicit.
/// Linear values use only scales. Absolute points also apply origins, after
/// confirming that both units belong to the requested affine space.
pub fn convert_value_with_role(
    value: f64,
    from: &UnitDefinition,
    to: &UnitDefinition,
    role: &QuantityRole,
) -> Result<f64, String> {
    if from.dimension != to.dimension {
        return Err(format!(
            "cannot convert dimensions {} and {}",
            dim_to_string(&from.dimension),
            dim_to_string(&to.dimension)
        ));
    }
    for unit in [from, to] {
        if !unit.scale.is_finite() || unit.scale <= 0.0 || !unit.offset.is_finite() {
            return Err(
                "unit conversion metadata must have a finite positive scale and finite offset"
                    .to_string(),
            );
        }
    }
    match role {
        QuantityRole::Linear => Ok(value * from.scale / to.scale),
        QuantityRole::Point(space) => {
            let expected_dimension = dimension_from_name(space);
            if from.dimension != expected_dimension {
                return Err(format!(
                    "affine space '{space}' does not match unit dimension {}",
                    dim_to_string(&from.dimension)
                ));
            }
            for unit in [from, to] {
                if unit.affine_space.as_deref() != Some(space.as_str()) {
                    return Err(format!(
                        "unit affine space '{}' does not match '{space}'",
                        unit.affine_space.as_deref().unwrap_or_default()
                    ));
                }
            }
            let coherent = value * from.scale + from.offset;
            Ok((coherent - to.offset) / to.scale)
        }
    }
}

/// Catálogo de unidades conocidas por la stdlib (documento 01, §3.1/§3.5):
/// símbolo → (dimensión, factor hacia la unidad coherente del SI). El runtime C
/// (`qty_runtime.c`) replica la misma tabla; `native_units`/`unit_algebra`
/// comprueban la paridad.
fn builtin_unit_info(symbol: &str) -> Option<(Dimension, f64)> {
    const L: &str = "Length";
    const M: &str = "Mass";
    const T: &str = "Time";
    const I: &str = "ElectricCurrent";
    let (dim, factor) = match symbol {
        "m" => (dim_of(&[(L, 1)]), 1.0),
        "nm" => (dim_of(&[(L, 1)]), 1e-9),
        "um" => (dim_of(&[(L, 1)]), 1e-6),
        "mm" => (dim_of(&[(L, 1)]), 1e-3),
        "cm" => (dim_of(&[(L, 1)]), 0.01),
        "km" => (dim_of(&[(L, 1)]), 1000.0),
        "s" => (dim_of(&[(T, 1)]), 1.0),
        "ns" => (dim_of(&[(T, 1)]), 1e-9),
        "us" => (dim_of(&[(T, 1)]), 1e-6),
        "ms" => (dim_of(&[(T, 1)]), 1e-3),
        "min" => (dim_of(&[(T, 1)]), 60.0),
        "h" => (dim_of(&[(T, 1)]), 3600.0),
        "day" => (dim_of(&[(T, 1)]), 86400.0),
        "kg" => (dim_of(&[(M, 1)]), 1.0),
        "g" => (dim_of(&[(M, 1)]), 1e-3),
        "mg" => (dim_of(&[(M, 1)]), 1e-6),
        "ug" => (dim_of(&[(M, 1)]), 1e-9),
        "K" => (dim_of(&[("Temperature", 1)]), 1.0),
        "A" => (dim_of(&[(I, 1)]), 1.0),
        "mA" => (dim_of(&[(I, 1)]), 1e-3),
        "mol" => (dim_of(&[("AmountOfSubstance", 1)]), 1.0),
        "mmol" => (dim_of(&[("AmountOfSubstance", 1)]), 1e-3),
        "umol" => (dim_of(&[("AmountOfSubstance", 1)]), 1e-6),
        "cd" => (dim_of(&[("LuminousIntensity", 1)]), 1.0),
        "USD" | "EUR" => (dim_of(&[("Currency", 1)]), 1.0),
        "bit" => (dim_of(&[("Information", 1)]), 1.0),
        "byte" => (dim_of(&[("Information", 1)]), 8.0),
        "L" => (dim_of(&[(L, 3)]), 1e-3),
        "mL" => (dim_of(&[(L, 3)]), 1e-6),
        "Hz" => (dim_of(&[(T, -1)]), 1.0),
        "kHz" => (dim_of(&[(T, -1)]), 1e3),
        "N" => (dim_of(&[(M, 1), (L, 1), (T, -2)]), 1.0),
        "kN" => (dim_of(&[(M, 1), (L, 1), (T, -2)]), 1e3),
        "J" => (dim_of(&[(M, 1), (L, 2), (T, -2)]), 1.0),
        "kJ" => (dim_of(&[(M, 1), (L, 2), (T, -2)]), 1e3),
        "cal" => (dim_of(&[(M, 1), (L, 2), (T, -2)]), 4.184),
        "kcal" => (dim_of(&[(M, 1), (L, 2), (T, -2)]), 4184.0),
        "W" => (dim_of(&[(M, 1), (L, 2), (T, -3)]), 1.0),
        "kW" => (dim_of(&[(M, 1), (L, 2), (T, -3)]), 1e3),
        "Pa" => (dim_of(&[(M, 1), (L, -1), (T, -2)]), 1.0),
        "kPa" => (dim_of(&[(M, 1), (L, -1), (T, -2)]), 1e3),
        "bar" => (dim_of(&[(M, 1), (L, -1), (T, -2)]), 1e5),
        "atm" => (dim_of(&[(M, 1), (L, -1), (T, -2)]), 101325.0),
        "mmHg" => (dim_of(&[(M, 1), (L, -1), (T, -2)]), 133.322387415),
        "C" => (dim_of(&[(I, 1), (T, 1)]), 1.0),
        "V" => (dim_of(&[(M, 1), (L, 2), (T, -3), (I, -1)]), 1.0),
        "mV" => (dim_of(&[(M, 1), (L, 2), (T, -3), (I, -1)]), 1e-3),
        "ohm" => (dim_of(&[(M, 1), (L, 2), (T, -3), (I, -2)]), 1.0),
        _ => return None,
    };
    Some((dim, factor))
}

pub fn unit_dimension(symbol: &str) -> Option<Dimension> {
    unit_info(symbol).map(|(dim, _)| dim)
}

/// Dimensiones derivadas con nombre (`Quantity<Energy>`, `Quantity<Velocity>`…),
/// que se expanden a dimensiones base al resolver tipos.
pub fn named_dimension(name: &str) -> Option<Dimension> {
    const L: &str = "Length";
    const M: &str = "Mass";
    const T: &str = "Time";
    const I: &str = "ElectricCurrent";
    Some(match name {
        "Area" => dim_of(&[(L, 2)]),
        "Volume" => dim_of(&[(L, 3)]),
        "Velocity" | "Speed" => dim_of(&[(L, 1), (T, -1)]),
        "Acceleration" => dim_of(&[(L, 1), (T, -2)]),
        "Frequency" => dim_of(&[(T, -1)]),
        "Force" => dim_of(&[(M, 1), (L, 1), (T, -2)]),
        "Momentum" => dim_of(&[(M, 1), (L, 1), (T, -1)]),
        "Energy" => dim_of(&[(M, 1), (L, 2), (T, -2)]),
        "Power" => dim_of(&[(M, 1), (L, 2), (T, -3)]),
        "Pressure" => dim_of(&[(M, 1), (L, -1), (T, -2)]),
        "Density" => dim_of(&[(M, 1), (L, -3)]),
        "Charge" => dim_of(&[(I, 1), (T, 1)]),
        "Voltage" => dim_of(&[(M, 1), (L, 2), (T, -3), (I, -1)]),
        "Resistance" => dim_of(&[(M, 1), (L, 2), (T, -3), (I, -2)]),
        "Concentration" => dim_of(&[("AmountOfSubstance", 1), (L, -3)]),
        _ => return None,
    })
}

/// Una dimensión escrita por el usuario: derivada con nombre o base.
pub fn dimension_from_name(name: &str) -> Dimension {
    named_dimension(name).unwrap_or_else(|| dim_single(name))
}

/// Descripción legible de una dimensión para diagnósticos:
/// `Mass*Length^2/Time^2 (Energy)`.
pub fn dim_describe(d: &Dimension) -> String {
    if d.is_empty() {
        return "Dimensionless".to_string();
    }
    let mut num: Vec<(&String, &i32)> = d.iter().filter(|(_, e)| **e > 0).collect();
    let mut den: Vec<(&String, &i32)> = d.iter().filter(|(_, e)| **e < 0).collect();
    let rank = |k: &str| {
        [
            "Mass",
            "Length",
            "Time",
            "ElectricCurrent",
            "Temperature",
            "AmountOfSubstance",
            "LuminousIntensity",
        ]
        .iter()
        .position(|b| *b == k)
        .unwrap_or(99)
    };
    num.sort_by(|a, b| (rank(a.0), a.0).cmp(&(rank(b.0), b.0)));
    den.sort_by(|a, b| (rank(a.0), a.0).cmp(&(rank(b.0), b.0)));
    let part = |(k, e): (&String, i32)| {
        if e == 1 {
            k.clone()
        } else {
            format!("{k}^{e}")
        }
    };
    let mut text = if num.is_empty() {
        "1".to_string()
    } else {
        num.iter()
            .map(|(k, e)| part((k, **e)))
            .collect::<Vec<_>>()
            .join("*")
    };
    for (k, e) in den {
        text.push('/');
        text.push_str(&part((k, -*e)));
    }
    const NAMES: [&str; 15] = [
        "Area",
        "Volume",
        "Velocity",
        "Acceleration",
        "Frequency",
        "Force",
        "Momentum",
        "Energy",
        "Power",
        "Pressure",
        "Density",
        "Charge",
        "Voltage",
        "Resistance",
        "Concentration",
    ];
    if let Some(name) = NAMES
        .iter()
        .find(|n| named_dimension(n).as_ref() == Some(d))
    {
        text.push_str(&format!(" ({name})"));
    }
    text
}

/// Descompone una expresión de unidad ("kg*m^2/s^2", "1/s") en átomos con
/// exponente, en orden de aparición y sin repetir símbolos. `a/b*c` se lee de
/// izquierda a derecha: `(a/b)*c`.
pub fn unit_atoms(expr: &str) -> Result<Vec<(String, i32)>, String> {
    let mut atoms: Vec<(String, i32)> = Vec::new();
    if expr.is_empty() {
        return Ok(atoms);
    }
    let mut sign = 1;
    let mut chars = expr.chars().peekable();
    loop {
        let mut atom = String::new();
        while let Some(&c) = chars.peek() {
            if c == '*' || c == '/' || c == '^' {
                break;
            }
            atom.push(c);
            chars.next();
        }
        if atom.is_empty() {
            return Err(format!("malformed unit expression '{expr}'"));
        }
        let mut exp = 1;
        if chars.peek() == Some(&'^') {
            chars.next();
            let mut exp_str = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_ascii_digit() || c == '-' {
                    exp_str.push(c);
                    chars.next();
                } else {
                    break;
                }
            }
            exp = exp_str
                .parse()
                .map_err(|_| format!("invalid exponent in '{expr}'"))?;
        }
        if atom != "1" {
            if unit_info(&atom).is_none() {
                return Err(atom);
            }
            push_atom(&mut atoms, &atom, sign * exp);
        }
        match chars.next() {
            Some('*') => sign = 1,
            Some('/') => sign = -1,
            None => break,
            _ => return Err(format!("malformed unit expression '{expr}'")),
        }
    }
    atoms.retain(|(_, e)| *e != 0);
    Ok(atoms)
}

fn push_atom(atoms: &mut Vec<(String, i32)>, atom: &str, exp: i32) {
    if let Some(entry) = atoms.iter_mut().find(|(a, _)| a == atom) {
        entry.1 += exp;
    } else {
        atoms.push((atom.to_string(), exp));
    }
}

/// `base^exp` por multiplicación repetida: el runtime C hace exactamente lo
/// mismo, así intérprete y nativo producen los mismos bits.
pub fn unit_pow(base: f64, exp: i32) -> f64 {
    let mut r = 1.0;
    for _ in 0..exp.unsigned_abs() {
        r *= base;
    }
    if exp < 0 {
        1.0 / r
    } else {
        r
    }
}

/// Resuelve una expresión de unidad compuesta ya concatenada por el lexer/parser
/// (p. ej. "m/s^2", "mmol/L") a su dimensión combinada.
pub fn resolve_unit_expr(expr: &str) -> Result<Dimension, String> {
    let mut result: Dimension = HashMap::new();
    for (atom, exp) in unit_atoms(expr)? {
        result = dim_mul(&result, &dim_pow(&unit_dimension(&atom).unwrap(), exp));
    }
    Ok(result)
}

/// Factor de una expresión de unidad hacia su unidad coherente del SI.
pub fn resolve_unit_factor(expr: &str) -> Result<f64, String> {
    let mut result = 1.0;
    for (atom, exp) in unit_atoms(expr)? {
        result *= unit_pow(unit_info(&atom).unwrap().1, exp);
    }
    Ok(result)
}

/// Dimensión base única de un átomo simple (`km` → Length, `h` → Time);
/// `None` para unidades derivadas (`J`, `L`, `Hz`).
fn single_base(atom: &str) -> Option<String> {
    let dim = unit_dimension(atom)?;
    if dim.len() == 1 {
        let (k, e) = dim.iter().next().unwrap();
        if *e == 1 {
            return Some(k.clone());
        }
    }
    None
}

pub fn format_unit_atoms(atoms: &[(String, i32)]) -> String {
    let part = |a: &str, e: i32| {
        if e == 1 {
            a.to_string()
        } else {
            format!("{a}^{e}")
        }
    };
    let num: Vec<String> = atoms
        .iter()
        .filter(|(_, e)| *e > 0)
        .map(|(a, e)| part(a, *e))
        .collect();
    let den: Vec<String> = atoms
        .iter()
        .filter(|(_, e)| *e < 0)
        .map(|(a, e)| part(a, -*e))
        .collect();
    if num.is_empty() && den.is_empty() {
        return String::new();
    }
    let mut text = if num.is_empty() {
        "1".to_string()
    } else {
        num.join("*")
    };
    for d in den {
        text.push('/');
        text.push_str(&d);
    }
    text
}

/// Producto (`divide == false`) o cociente de dos unidades en forma canónica:
/// agrupa exponentes (`m/s*s` → `m`, `kg*m/s*m/s` → `kg*m^2/s^2`) y funde
/// átomos simples de la misma dimensión en el primero que aparece
/// (`km/h * s` → `km` con factor 1/3600). Devuelve el factor que hay que
/// aplicar al valor y la unidad resultante.
pub fn unit_combine(a: &str, b: &str, divide: bool) -> Result<(f64, String), String> {
    let mut atoms = unit_atoms(a)?;
    for (atom, exp) in unit_atoms(b)? {
        push_atom(&mut atoms, &atom, if divide { -exp } else { exp });
    }
    atoms.retain(|(_, e)| *e != 0);
    let mut scale = 1.0;
    let mut i = 0;
    while i < atoms.len() {
        if let Some(base) = single_base(&atoms[i].0) {
            let fi = unit_info(&atoms[i].0).unwrap().1;
            let mut j = i + 1;
            while j < atoms.len() {
                if single_base(&atoms[j].0).as_deref() == Some(base.as_str()) {
                    let (atom, exp) = atoms.remove(j);
                    let fj = unit_info(&atom).unwrap().1;
                    scale *= unit_pow(fj / fi, exp);
                    atoms[i].1 += exp;
                } else {
                    j += 1;
                }
            }
        }
        if atoms[i].1 == 0 {
            atoms.remove(i);
        } else {
            i += 1;
        }
    }
    Ok((scale, format_unit_atoms(&atoms)))
}

#[derive(Debug, Clone, PartialEq)]
pub enum Ty {
    Int,
    Float,
    Bool,
    Char,
    String,
    Void,
    Quantity(Dimension),
    List(Box<Ty>),
    Map(Box<Ty>, Box<Ty>),
    Set(Box<Ty>),
    Named(String),
    Applied(String, Vec<Ty>),
    Generic(String),
    /// `dyn Trait` (a single trait): the concrete type is erased.
    Dyn(String),
    /// A fixed-width integer other than `Int` (which is `Int64`).
    Sized(crate::ast::IntKind),
    /// Single-precision float (`Float` is `Float64`).
    Float32,
    Fn(Vec<Ty>, Box<Ty>),
    Unknown,
}

/// True when a type is `Unknown` or mentions `Unknown` anywhere inside it.
pub fn ty_contains_unknown(ty: &Ty) -> bool {
    match ty {
        Ty::Unknown => true,
        Ty::List(t) | Ty::Set(t) => ty_contains_unknown(t),
        Ty::Map(k, v) => ty_contains_unknown(k) || ty_contains_unknown(v),
        Ty::Applied(_, args) => args.iter().any(ty_contains_unknown),
        Ty::Fn(params, ret) => params.iter().any(ty_contains_unknown) || ty_contains_unknown(ret),
        _ => false,
    }
}

impl Ty {
    pub fn is_numeric_scalar(&self) -> bool {
        matches!(self, Ty::Int | Ty::Float)
    }

    pub fn describe(&self) -> String {
        match self {
            Ty::Int => "Int".to_string(),
            Ty::Float => "Float".to_string(),
            Ty::Bool => "Bool".to_string(),
            Ty::Char => "Char".to_string(),
            Ty::String => "String".to_string(),
            Ty::Void => "Void".to_string(),
            Ty::Quantity(d) => format!("Quantity<{}>", dim_to_string(d)),
            Ty::List(t) => format!("List<{}>", t.describe()),
            Ty::Map(k, v) => format!("Map<{}, {}>", k.describe(), v.describe()),
            Ty::Set(t) => format!("Set<{}>", t.describe()),
            Ty::Named(name) => name.clone(),
            Ty::Applied(name, args) => {
                let args: Vec<String> = args.iter().map(|ty| ty.describe()).collect();
                format!("{name}<{}>", args.join(", "))
            }
            Ty::Generic(name) => name.clone(),
            Ty::Dyn(name) => format!("dyn {name}"),
            Ty::Sized(kind) => kind.name().to_string(),
            Ty::Float32 => "Float32".to_string(),
            Ty::Fn(params, ret) => {
                let p: Vec<String> = params.iter().map(|t| t.describe()).collect();
                format!("fn({}) -> {}", p.join(", "), ret.describe())
            }
            Ty::Unknown => "?".to_string(),
        }
    }
}

#[cfg(test)]
mod affine_unit_tests {
    use super::{
        convert_value_with_role, declare_unit, define_unit, dim_single, reset_user_units,
        unit_definition, QuantityRole, UnitDefinition,
    };

    fn close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() <= 1e-12 * expected.abs().max(1.0),
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn affine_point_and_linear_interval_conversions_use_distinct_rules() {
        let temperature = dim_single("Temperature");
        let celsius =
            UnitDefinition::affine(temperature.clone(), 1.0, 273.15, "Temperature").unwrap();
        let fahrenheit =
            UnitDefinition::affine(temperature.clone(), 5.0 / 9.0, 459.67 / 1.8, "Temperature")
                .unwrap();
        let kelvin = unit_definition("K").unwrap();

        close(
            convert_value_with_role(
                0.0,
                &celsius,
                &kelvin,
                &QuantityRole::Point("Temperature".to_string()),
            )
            .unwrap(),
            273.15,
        );
        close(
            convert_value_with_role(
                212.0,
                &fahrenheit,
                &celsius,
                &QuantityRole::Point("Temperature".to_string()),
            )
            .unwrap(),
            100.0,
        );
        close(
            convert_value_with_role(1.0, &fahrenheit, &kelvin, &QuantityRole::Linear).unwrap(),
            5.0 / 9.0,
        );
        close(
            convert_value_with_role(1.0, &celsius, &kelvin, &QuantityRole::Linear).unwrap(),
            1.0,
        );
    }

    #[test]
    fn affine_point_conversion_rejects_incompatible_spaces_and_dimensions() {
        let temperature = dim_single("Temperature");
        let pressure = dim_single("Pressure");
        let celsius =
            UnitDefinition::affine(temperature.clone(), 1.0, 273.15, "Temperature").unwrap();
        let kelvin = unit_definition("K").unwrap();
        let unmarked_linear = UnitDefinition::linear(dim_single("Temperature"), 1.0);
        let pascal = UnitDefinition::affine(pressure, 1.0, 0.0, "Pressure").unwrap();

        assert!(convert_value_with_role(
            1.0,
            &celsius,
            &kelvin,
            &QuantityRole::Point("Pressure".to_string()),
        )
        .is_err());
        assert!(convert_value_with_role(
            1.0,
            &celsius,
            &pascal,
            &QuantityRole::Point("Temperature".to_string()),
        )
        .is_err());
        assert!(convert_value_with_role(
            1.0,
            &unmarked_linear,
            &kelvin,
            &QuantityRole::Point("Temperature".to_string()),
        )
        .is_err());
    }

    #[test]
    fn existing_units_keep_their_scale_and_zero_offset() {
        let kelvin = unit_definition("K").unwrap();
        assert_eq!(kelvin.dimension, dim_single("Temperature"));
        assert_eq!(kelvin.scale, 1.0);
        assert_eq!(kelvin.offset, 0.0);
        assert_eq!(kelvin.affine_space, Some("Temperature".to_string()));

        reset_user_units();
        declare_unit("affine_test_foot", dim_single("Length")).unwrap();
        define_unit("affine_test_foot", 0.3048).unwrap();
        let foot = unit_definition("affine_test_foot").unwrap();
        assert_eq!(foot.dimension, dim_single("Length"));
        assert_eq!(foot.scale, 0.3048);
        assert_eq!(foot.offset, 0.0);
        assert_eq!(foot.affine_space, None);
        reset_user_units();
    }

    #[test]
    fn affine_unit_metadata_requires_finite_positive_scale() {
        let temperature = dim_single("Temperature");
        assert!(UnitDefinition::affine(temperature.clone(), 0.0, 273.15, "Temperature").is_err());
        assert!(
            UnitDefinition::affine(temperature.clone(), f64::INFINITY, 273.15, "Temperature")
                .is_err()
        );
        assert!(UnitDefinition::affine(temperature, 1.0, f64::NAN, "Temperature").is_err());
        assert!(UnitDefinition::affine(dim_single("Temperature"), 1.0, 0.0, "").is_err());
    }
}
