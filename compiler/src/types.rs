use std::collections::HashMap;

pub type Dimension = HashMap<String, i32>;

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
        .map(|(k, v)| if *v == 1 { k.clone() } else { format!("{k}^{v}") })
        .collect();
    parts.sort();
    parts.join("*")
}

/// Símbolos de unidad atómicos conocidos por la stdlib (documento 01, §3.1/§3.5).
/// Un catálogo real cargaría esto desde la stdlib; aquí basta un subconjunto
/// suficiente para validar los ejemplos y el propio diseño.
pub fn unit_dimension(symbol: &str) -> Option<Dimension> {
    Some(match symbol {
        "m" | "nm" | "km" | "cm" | "mm" => dim_single("Length"),
        "s" | "ms" | "min" | "h" => dim_single("Time"),
        "kg" | "g" | "mg" => dim_single("Mass"),
        "K" => dim_single("Temperature"),
        "A" => dim_single("ElectricCurrent"),
        "mol" | "mmol" => dim_single("AmountOfSubstance"),
        "cd" => dim_single("LuminousIntensity"),
        "USD" | "EUR" => dim_single("Currency"),
        "bit" | "byte" => dim_single("Information"),
        "L" => dim_pow(&dim_single("Length"), 3),
        "C" => dim_single("Charge"),
        "atm" | "Pa" => dim_single("Pressure"),
        _ => return None,
    })
}

/// Resuelve una expresión de unidad compuesta ya concatenada por el lexer/parser
/// (p. ej. "m/s^2", "mmol/L") a su dimensión combinada.
pub fn resolve_unit_expr(expr: &str) -> Result<Dimension, String> {
    let mut result: Dimension = HashMap::new();
    let mut op = '*';
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
        let mut atom_dim = unit_dimension(&atom).ok_or_else(|| atom.clone())?;
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
            let exp: i32 = exp_str.parse().map_err(|_| format!("invalid exponent in '{expr}'"))?;
            atom_dim = dim_pow(&atom_dim, exp);
        }
        result = if op == '*' { dim_mul(&result, &atom_dim) } else { dim_div(&result, &atom_dim) };
        match chars.next() {
            Some(c @ ('*' | '/')) => op = c,
            None => break,
            _ => return Err(format!("malformed unit expression '{expr}'")),
        }
    }
    Ok(result)
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
