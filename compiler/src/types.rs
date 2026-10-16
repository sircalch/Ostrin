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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Used by source-level TemperaturePoint lowering.
pub enum QuantityArithmetic {
    Add,
    Subtract,
}

/// Static result role for the affine arithmetic table. The type checker can
/// use this before selecting runtime conversion formulas.
#[allow(dead_code)] // Consumed by type checking when TemperaturePoint is enabled.
pub fn arithmetic_result_role(
    left: &QuantityRole,
    right: &QuantityRole,
    operation: QuantityArithmetic,
) -> Result<QuantityRole, String> {
    match (left, right, operation) {
        (QuantityRole::Linear, QuantityRole::Linear, _) => Ok(QuantityRole::Linear),
        (QuantityRole::Point(space), QuantityRole::Linear, _) => {
            Ok(QuantityRole::Point(space.clone()))
        }
        (QuantityRole::Linear, QuantityRole::Point(space), QuantityArithmetic::Add) => {
            Ok(QuantityRole::Point(space.clone()))
        }
        (QuantityRole::Linear, QuantityRole::Point(_), QuantityArithmetic::Subtract) => {
            Err("subtracting an absolute point from an interval is undefined".to_string())
        }
        (QuantityRole::Point(_), QuantityRole::Point(_), QuantityArithmetic::Add) => {
            Err("adding two absolute points is undefined".to_string())
        }
        (QuantityRole::Point(left), QuantityRole::Point(right), QuantityArithmetic::Subtract)
            if left == right =>
        {
            Ok(QuantityRole::Linear)
        }
        (QuantityRole::Point(_), QuantityRole::Point(_), QuantityArithmetic::Subtract) => {
            Err("cannot subtract points from different affine spaces".to_string())
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)] // Values are constructed by the future typed expression path.
pub struct RoleAwareQuantity {
    pub value: f64,
    pub unit: UnitDefinition,
    pub role: QuantityRole,
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

/// Full unit conversion metadata. Existing quantities keep linear conversion
/// semantics; the explicit absolute-temperature constructor opts into point
/// offsets and affine-space validation.
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

/// Point-scale catalog. These symbols intentionally do not enter `unit_info`:
/// unit suffixes such as `20 K` remain linear intervals. Absolute readings use
/// the explicit `temperature_point(value, scale)` constructor.
pub fn builtin_temperature_point_unit(symbol: &str) -> Option<UnitDefinition> {
    let temperature = dim_single("Temperature");
    match symbol {
        "K" => Some(
            UnitDefinition::affine(temperature, 1.0, 0.0, "Temperature")
                .expect("built-in Kelvin metadata is valid"),
        ),
        "degC" => Some(
            UnitDefinition::affine(temperature, 1.0, 273.15, "Temperature")
                .expect("built-in Celsius metadata is valid"),
        ),
        "degF" => Some(
            // Anchor the floating-point offset at the exact reference pair
            // 32 °F = 273.15 K. Together with `mul_add` below, this keeps
            // equivalent reference readings equal after conversion.
            UnitDefinition::affine(
                temperature,
                5.0 / 9.0,
                273.15 - 32.0 * (5.0 / 9.0),
                "Temperature",
            )
            .expect("built-in Fahrenheit metadata is valid"),
        ),
        _ => None,
    }
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
            let coherent = value.mul_add(from.scale, from.offset);
            Ok((coherent - to.offset) / to.scale)
        }
    }
}

/// Add or subtract quantities without erasing whether an affine value is a
/// point or an interval. Point shifts preserve the point operand's scale;
/// point differences return coherent linear units. Invalid affine operations
/// return an error.
#[allow(dead_code)] // Called by the interpreter; native/WASI lowering is pending.
pub fn combine_quantities_with_role(
    left: &RoleAwareQuantity,
    right: &RoleAwareQuantity,
    operation: QuantityArithmetic,
) -> Result<RoleAwareQuantity, String> {
    let result_role = arithmetic_result_role(&left.role, &right.role, operation)?;
    for value in [left, right] {
        if let QuantityRole::Point(space) = &value.role {
            convert_value_with_role(
                value.value,
                &value.unit,
                &value.unit,
                &QuantityRole::Point(space.clone()),
            )?;
        }
    }
    let (left_point, right_point) = (
        matches!(&left.role, QuantityRole::Point(_)),
        matches!(&right.role, QuantityRole::Point(_)),
    );
    match (left_point, right_point, operation) {
        (true, true, QuantityArithmetic::Add) => {
            Err("adding two absolute points is undefined".to_string())
        }
        (false, true, QuantityArithmetic::Subtract) => {
            Err("subtracting an absolute point from an interval is undefined".to_string())
        }
        (true, true, QuantityArithmetic::Subtract) => {
            let QuantityRole::Point(_) = &left.role else {
                unreachable!()
            };
            let left_coherent = left.value.mul_add(left.unit.scale, left.unit.offset);
            let right_coherent = right.value.mul_add(right.unit.scale, right.unit.offset);
            Ok(RoleAwareQuantity {
                value: left_coherent - right_coherent,
                unit: UnitDefinition::linear(left.unit.dimension.clone(), 1.0),
                role: result_role.clone(),
            })
        }
        (true, false, operation) => {
            let interval = convert_value_with_role(
                right.value,
                &right.unit,
                &left.unit,
                &QuantityRole::Linear,
            )?;
            Ok(RoleAwareQuantity {
                value: match operation {
                    QuantityArithmetic::Add => left.value + interval,
                    QuantityArithmetic::Subtract => left.value - interval,
                },
                unit: left.unit.clone(),
                role: result_role.clone(),
            })
        }
        (false, true, QuantityArithmetic::Add) => {
            let interval = convert_value_with_role(
                left.value,
                &left.unit,
                &right.unit,
                &QuantityRole::Linear,
            )?;
            Ok(RoleAwareQuantity {
                value: interval + right.value,
                unit: right.unit.clone(),
                role: result_role.clone(),
            })
        }
        (false, false, operation) => {
            let right_value = convert_value_with_role(
                right.value,
                &right.unit,
                &left.unit,
                &QuantityRole::Linear,
            )?;
            Ok(RoleAwareQuantity {
                value: match operation {
                    QuantityArithmetic::Add => left.value + right_value,
                    QuantityArithmetic::Subtract => left.value - right_value,
                },
                unit: UnitDefinition::linear(left.unit.dimension.clone(), left.unit.scale),
                role: result_role,
            })
        }
    }
}

/// Compare two values only when both have the same semantic role. Absolute
/// points are normalized with offsets; linear intervals use scale only.
#[allow(dead_code)] // Called by the interpreter; native/WASI comparison is pending.
pub fn compare_quantities_with_role(
    left: &RoleAwareQuantity,
    right: &RoleAwareQuantity,
) -> Result<std::cmp::Ordering, String> {
    let (left_value, right_value) = match (&left.role, &right.role) {
        (QuantityRole::Linear, QuantityRole::Linear) => {
            let right_value = convert_value_with_role(
                right.value,
                &right.unit,
                &left.unit,
                &QuantityRole::Linear,
            )?;
            (left.value, right_value)
        }
        (QuantityRole::Point(left_space), QuantityRole::Point(right_space))
            if left_space == right_space =>
        {
            // Validate dimensions, affine-space membership and finite scale
            // metadata before normalizing directly to the coherent scale.
            convert_value_with_role(
                left.value,
                &left.unit,
                &left.unit,
                &QuantityRole::Point(left_space.clone()),
            )?;
            convert_value_with_role(
                right.value,
                &right.unit,
                &right.unit,
                &QuantityRole::Point(right_space.clone()),
            )?;
            (
                left.value.mul_add(left.unit.scale, left.unit.offset),
                right.value.mul_add(right.unit.scale, right.unit.offset),
            )
        }
        (QuantityRole::Point(_), QuantityRole::Point(_)) => {
            return Err("cannot compare points from different affine spaces".to_string())
        }
        _ => return Err("cannot compare an absolute point with a linear interval".to_string()),
    };
    left_value
        .partial_cmp(&right_value)
        .ok_or_else(|| "cannot order NaN quantities".to_string())
}

#[allow(dead_code)] // Called by the typed unary-expression lowering.
pub fn negate_quantity_with_role(value: &RoleAwareQuantity) -> Result<RoleAwareQuantity, String> {
    match &value.role {
        QuantityRole::Linear => Ok(RoleAwareQuantity {
            value: -value.value,
            unit: UnitDefinition::linear(value.unit.dimension.clone(), value.unit.scale),
            role: QuantityRole::Linear,
        }),
        QuantityRole::Point(_) => Err("negating an absolute point is undefined".to_string()),
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
    /// Absolute temperature reading. This is an internal typed representation;
    /// source construction remains disabled until runtime and backend lowering
    /// preserve the role end to end.
    TemperaturePoint,
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
            Ty::TemperaturePoint => "TemperaturePoint".to_string(),
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
        arithmetic_result_role, builtin_temperature_point_unit, combine_quantities_with_role,
        compare_quantities_with_role, convert_value_with_role, declare_unit, define_unit,
        dim_single, negate_quantity_with_role, reset_user_units, unit_definition, unit_info,
        QuantityArithmetic, QuantityRole, RoleAwareQuantity, UnitDefinition,
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
        let celsius = builtin_temperature_point_unit("degC").unwrap();
        let fahrenheit = builtin_temperature_point_unit("degF").unwrap();
        let kelvin = builtin_temperature_point_unit("K").unwrap();
        assert_eq!(celsius.dimension, temperature);

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
            convert_value_with_role(
                0.0,
                &celsius,
                &fahrenheit,
                &QuantityRole::Point("Temperature".to_string()),
            )
            .unwrap(),
            32.0,
        );
        let round_trip = convert_value_with_role(
            convert_value_with_role(
                23.4,
                &celsius,
                &fahrenheit,
                &QuantityRole::Point("Temperature".to_string()),
            )
            .unwrap(),
            &fahrenheit,
            &celsius,
            &QuantityRole::Point("Temperature".to_string()),
        )
        .unwrap();
        close(round_trip, 23.4);
        close(
            convert_value_with_role(1.0, &fahrenheit, &kelvin, &QuantityRole::Linear).unwrap(),
            5.0 / 9.0,
        );
        close(
            convert_value_with_role(1.0, &celsius, &kelvin, &QuantityRole::Linear).unwrap(),
            1.0,
        );
        close(
            convert_value_with_role(1.0, &fahrenheit, &celsius, &QuantityRole::Linear).unwrap(),
            5.0 / 9.0,
        );
    }

    #[test]
    fn affine_point_conversion_rejects_incompatible_spaces_and_dimensions() {
        let pressure = dim_single("Pressure");
        let celsius = builtin_temperature_point_unit("degC").unwrap();
        let kelvin = builtin_temperature_point_unit("K").unwrap();
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

        let temperature_point = RoleAwareQuantity {
            value: 20.0,
            unit: celsius.clone(),
            role: QuantityRole::Point("Temperature".to_string()),
        };
        let pressure_point = RoleAwareQuantity {
            value: 20.0,
            unit: pascal,
            role: QuantityRole::Point("Temperature".to_string()),
        };
        assert!(compare_quantities_with_role(&temperature_point, &pressure_point).is_err());
    }

    #[test]
    fn existing_units_keep_their_scale_and_zero_offset() {
        let kelvin = builtin_temperature_point_unit("K").unwrap();
        assert_eq!(kelvin.dimension, dim_single("Temperature"));
        assert_eq!(kelvin.scale, 1.0);
        assert_eq!(kelvin.offset, 0.0);
        assert_eq!(kelvin.affine_space, Some("Temperature".to_string()));
        assert_eq!(
            unit_definition("K").unwrap().affine_space,
            Some("Temperature".to_string())
        );
        assert!(unit_info("degC").is_none());
        assert!(unit_info("degF").is_none());
        assert!(builtin_temperature_point_unit("m").is_none());

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

    #[test]
    fn point_arithmetic_preserves_roles_and_rejects_invalid_affine_operations() {
        let celsius = builtin_temperature_point_unit("degC").unwrap();
        let fahrenheit = builtin_temperature_point_unit("degF").unwrap();
        let celsius_interval = UnitDefinition::linear(dim_single("Temperature"), 1.0);
        let fahrenheit_interval = UnitDefinition::linear(dim_single("Temperature"), 5.0 / 9.0);
        let point = |value, unit| RoleAwareQuantity {
            value,
            unit,
            role: QuantityRole::Point("Temperature".to_string()),
        };
        let interval = |value, unit| RoleAwareQuantity {
            value,
            unit,
            role: QuantityRole::Linear,
        };

        let point_role = QuantityRole::Point("Temperature".to_string());
        let linear_role = QuantityRole::Linear;
        assert_eq!(
            arithmetic_result_role(&point_role, &point_role, QuantityArithmetic::Subtract).unwrap(),
            linear_role
        );
        assert_eq!(
            arithmetic_result_role(&point_role, &linear_role, QuantityArithmetic::Add).unwrap(),
            point_role
        );
        assert!(arithmetic_result_role(&point_role, &point_role, QuantityArithmetic::Add).is_err());

        let difference = combine_quantities_with_role(
            &point(0.0, celsius.clone()),
            &point(32.0, fahrenheit.clone()),
            QuantityArithmetic::Subtract,
        )
        .unwrap();
        close(difference.value, 0.0);
        assert_eq!(difference.role, QuantityRole::Linear);
        assert_eq!(difference.unit.offset, 0.0);
        assert_eq!(difference.unit.affine_space, None);

        let shifted = combine_quantities_with_role(
            &point(20.0, celsius.clone()),
            &interval(18.0, fahrenheit_interval.clone()),
            QuantityArithmetic::Add,
        )
        .unwrap();
        close(shifted.value, 30.0);
        assert_eq!(shifted.role, QuantityRole::Point("Temperature".to_string()));

        let shifted = combine_quantities_with_role(
            &point(20.0, celsius.clone()),
            &interval(18.0, fahrenheit_interval.clone()),
            QuantityArithmetic::Subtract,
        )
        .unwrap();
        close(shifted.value, 10.0);
        assert_eq!(shifted.role, QuantityRole::Point("Temperature".to_string()));

        let shifted = combine_quantities_with_role(
            &interval(18.0, fahrenheit_interval),
            &point(20.0, celsius.clone()),
            QuantityArithmetic::Add,
        )
        .unwrap();
        close(shifted.value, 30.0);
        assert_eq!(shifted.role, QuantityRole::Point("Temperature".to_string()));

        let left = point(20.0, celsius.clone());
        let right = point(69.0, fahrenheit.clone());
        assert_eq!(
            compare_quantities_with_role(&left, &right).unwrap(),
            std::cmp::Ordering::Less
        );
        assert_eq!(
            compare_quantities_with_role(&left, &point(20.0, celsius.clone())).unwrap(),
            std::cmp::Ordering::Equal
        );
        assert!(combine_quantities_with_role(&left, &right, QuantityArithmetic::Add).is_err());
        assert!(combine_quantities_with_role(
            &interval(2.0, celsius_interval.clone()),
            &left,
            QuantityArithmetic::Subtract
        )
        .is_err());
        assert!(compare_quantities_with_role(&left, &interval(20.0, celsius_interval)).is_err());
        assert!(negate_quantity_with_role(&left).is_err());
        let negated_interval = negate_quantity_with_role(&interval(
            4.0,
            UnitDefinition::linear(dim_single("Temperature"), 1.0),
        ))
        .unwrap();
        close(negated_interval.value, -4.0);
        assert_eq!(negated_interval.role, QuantityRole::Linear);
    }

    #[test]
    fn affine_temperature_operations_preserve_coherent_values_across_scales() {
        let temperature = dim_single("Temperature");
        let role = QuantityRole::Point("Temperature".to_string());
        let kelvin = builtin_temperature_point_unit("K").unwrap();
        let scales = [
            builtin_temperature_point_unit("K").unwrap(),
            builtin_temperature_point_unit("degC").unwrap(),
            builtin_temperature_point_unit("degF").unwrap(),
        ];
        let close_with_roundoff = |actual: f64, expected: f64, magnitude: f64| {
            let tolerance = 16.0 * f64::EPSILON * magnitude.abs().max(expected.abs()).max(1.0);
            assert!(
                (actual - expected).abs() <= tolerance,
                "expected {expected} ± {tolerance}, got {actual}"
            );
        };

        // Exercise every point-unit conversion pair across absolute zero,
        // familiar reference points, and a large finite reading.
        for from in &scales {
            for to in &scales {
                for value in [-273.15, -40.0, 0.0, 37.0, 100.0, 1.0e6] {
                    let coherent = value * from.scale + from.offset;
                    let converted = convert_value_with_role(value, from, to, &role).unwrap();
                    close(converted * to.scale + to.offset, coherent);
                    close(
                        convert_value_with_role(converted, to, from, &role).unwrap(),
                        value,
                    );
                }
            }
        }

        // Equivalent reference readings must compare equal across scales.
        // Fused multiply-add and the Fahrenheit offset anchor avoid a false
        // ordering caused by intermediate rounding at 32 °F = 0 °C.
        for celsius in [-40.0, 0.0, 37.0, 100.0] {
            let celsius_point = RoleAwareQuantity {
                value: celsius,
                unit: scales[1].clone(),
                role: role.clone(),
            };
            let fahrenheit_value =
                convert_value_with_role(celsius, &scales[1], &scales[2], &role).unwrap();
            let fahrenheit_point = RoleAwareQuantity {
                value: fahrenheit_value,
                unit: scales[2].clone(),
                role: role.clone(),
            };
            assert_eq!(
                compare_quantities_with_role(&celsius_point, &fahrenheit_point).unwrap(),
                std::cmp::Ordering::Equal,
                "{celsius} °C and {fahrenheit_value} °F should compare equal"
            );
            assert_eq!(
                compare_quantities_with_role(&fahrenheit_point, &celsius_point).unwrap(),
                std::cmp::Ordering::Equal,
                "{fahrenheit_value} °F and {celsius} °C should compare equal"
            );
        }

        // The same interval must move an absolute point by the same coherent
        // amount regardless of which scale expresses either operand.
        for point_unit in &scales {
            for interval_scale in [1.0, 5.0 / 9.0] {
                let interval_unit = UnitDefinition::linear(temperature.clone(), interval_scale);
                for coherent_point in [0.0, 233.15, 273.15, 373.15, 1.0e6] {
                    let point_value =
                        convert_value_with_role(coherent_point, &kelvin, point_unit, &role)
                            .unwrap();
                    for coherent_delta in [-100.0, -1.0, 0.0, 1.0, 100.0] {
                        let interval_value = coherent_delta / interval_unit.scale;
                        let point = RoleAwareQuantity {
                            value: point_value,
                            unit: point_unit.clone(),
                            role: role.clone(),
                        };
                        let interval = RoleAwareQuantity {
                            value: interval_value,
                            unit: interval_unit.clone(),
                            role: QuantityRole::Linear,
                        };

                        for (operation, sign) in [
                            (QuantityArithmetic::Add, 1.0),
                            (QuantityArithmetic::Subtract, -1.0),
                        ] {
                            let result =
                                combine_quantities_with_role(&point, &interval, operation).unwrap();
                            assert_eq!(result.role, role);
                            close(
                                result.value * result.unit.scale + result.unit.offset,
                                coherent_point + sign * coherent_delta,
                            );
                        }

                        let other_point_value = convert_value_with_role(
                            coherent_point + coherent_delta,
                            &kelvin,
                            point_unit,
                            &role,
                        )
                        .unwrap();
                        let difference = combine_quantities_with_role(
                            &point,
                            &RoleAwareQuantity {
                                value: other_point_value,
                                unit: point_unit.clone(),
                                role: role.clone(),
                            },
                            QuantityArithmetic::Subtract,
                        )
                        .unwrap();
                        assert_eq!(difference.role, QuantityRole::Linear);
                        assert_eq!(difference.unit.offset, 0.0);
                        assert_eq!(difference.unit.affine_space, None);
                        close_with_roundoff(
                            difference.value * difference.unit.scale,
                            -coherent_delta,
                            (point_value.abs() + other_point_value.abs()) * point_unit.scale
                                + 2.0 * point_unit.offset.abs(),
                        );
                    }
                }
            }
        }
    }
}
