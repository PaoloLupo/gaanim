//! Quantities with units as Typst math, in the spirit of LaTeX's siunitx.
//!
//! [`quantity_math`] turns a number and a unit such as `"m^3"`, `"km/h"` or
//! `"kg m/s^2"` into math markup with a thin space between number and unit,
//! upright unit symbols, exponents and a literal slash, so `1 m³` and
//! `181 L` are spaced alike.

/// The number of a quantity: an integer keeps its digits, a float is shown
/// with `decimals` places or its shortest exact form.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QuantityValue {
    Integer(i64),
    Float(f64),
}

/// Units written right after the number, without the thin space.
const ATTACHED: [&str; 5] = ["°", "′", "″", "'", "\""];

/// Typst math for `value` followed by `unit`.
///
/// `unit` lists factors separated by spaces, `*`, `·` or `.`, each a symbol
/// with an optional `^exponent` (`m^3`, `s^-1`), and `/` divides by the
/// factors after it. `decimal_separator` replaces the decimal point. An
/// empty unit gives the number alone.
pub fn quantity_math(
    value: QuantityValue,
    unit: &str,
    decimals: Option<usize>,
    decimal_separator: &str,
) -> Result<String, String> {
    if decimal_separator.chars().count() != 1 {
        return Err("decimal_separator must be one character".into());
    }
    let number = number_math(value, decimals, decimal_separator)?;
    let unit = unit.trim();
    if unit.is_empty() {
        return Ok(number);
    }
    if ATTACHED.contains(&unit) {
        let symbol = match unit {
            "'" => "′",
            "\"" => "″",
            other => other,
        };
        return Ok(format!("{number} {}", typst_string(symbol)));
    }
    Ok(format!("{number} thin {}", unit_math(unit)?))
}

fn number_math(
    value: QuantityValue,
    decimals: Option<usize>,
    decimal_separator: &str,
) -> Result<String, String> {
    let text = match value {
        QuantityValue::Integer(value) => match decimals {
            Some(decimals) if decimals > 0 => format!("{:.*}", decimals, value as f64),
            _ => value.to_string(),
        },
        QuantityValue::Float(value) => {
            if !value.is_finite() {
                return Err("a quantity must be finite".into());
            }
            match decimals {
                Some(decimals) => format!("{value:.decimals$}"),
                None => format!("{value}"),
            }
        }
    };
    let (sign, digits) = match text.strip_prefix('-') {
        Some(digits) => ("-", digits),
        None => ("", text.as_str()),
    };
    // Typst joins digits around a dot into one number; any other separator
    // is set as text, so it adds no space of its own.
    let digits = match digits.split_once('.') {
        Some((whole, fraction)) if decimal_separator != "." => {
            format!("{whole}{}{fraction}", typst_string(decimal_separator))
        }
        _ => digits.to_string(),
    };
    Ok(format!("{sign}{digits}"))
}

fn unit_math(unit: &str) -> Result<String, String> {
    let mut parts = Vec::new();
    for (index, part) in unit.split('/').enumerate() {
        let factors: Vec<&str> = part
            .split(|ch: char| ch.is_whitespace() || matches!(ch, '*' | '·' | '.'))
            .filter(|factor| !factor.is_empty())
            .collect();
        if factors.is_empty() {
            return Err(if index == 0 {
                format!("unit {unit:?} needs a symbol before '/'")
            } else {
                format!("unit {unit:?} needs a symbol after '/'")
            });
        }
        let factors = factors
            .into_iter()
            .map(factor_math)
            .collect::<Result<Vec<_>, _>>()?;
        parts.push(factors.join(" thin "));
    }
    // An escaped slash stays a slash instead of building a fraction.
    Ok(parts.join(" \\/ "))
}

fn factor_math(factor: &str) -> Result<String, String> {
    let (symbol, exponent) = match factor.split_once('^') {
        Some((symbol, exponent)) => (symbol, Some(exponent)),
        None => (factor, None),
    };
    if symbol.is_empty() {
        return Err(format!("unit factor {factor:?} has no symbol"));
    }
    let symbol = typst_string(symbol);
    match exponent {
        None => Ok(symbol),
        Some(exponent) => {
            let valid = !exponent.is_empty()
                && exponent
                    .trim_start_matches(['-', '+'])
                    .parse::<f64>()
                    .is_ok()
                && exponent
                    .chars()
                    .skip(1)
                    .all(|ch| ch.is_ascii_digit() || ch == '.');
            if !valid {
                return Err(format!(
                    "unit exponent {exponent:?} in {factor:?} must be a number, e.g. m^3 or s^-1"
                ));
            }
            Ok(format!("{symbol}^({exponent})"))
        }
    }
}

/// An upright Typst math string.
fn typst_string(text: &str) -> String {
    let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
    format!("upright(\"{escaped}\")")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(value: QuantityValue, unit: &str) -> String {
        quantity_math(value, unit, None, ".").unwrap()
    }

    #[test]
    fn number_and_unit_are_spaced_alike() {
        use QuantityValue::*;
        assert_eq!(q(Integer(181), "L"), r#"181 thin upright("L")"#);
        assert_eq!(q(Integer(1), "m^3"), r#"1 thin upright("m")^(3)"#);
        assert_eq!(
            q(Float(9.81), "m/s^2"),
            r#"9.81 thin upright("m") \/ upright("s")^(2)"#
        );
        assert_eq!(
            q(Integer(3), "kg m s^-1"),
            r#"3 thin upright("kg") thin upright("m") thin upright("s")^(-1)"#
        );
        assert_eq!(q(Integer(90), "°"), r#"90 upright("°")"#);
        assert_eq!(q(Integer(20), "°C"), r#"20 thin upright("°C")"#);
        assert_eq!(q(Integer(-4), ""), "-4");
        assert_eq!(q(Integer(5), "µm"), r#"5 thin upright("µm")"#);
    }

    #[test]
    fn decimals_and_separator_shape_the_number() {
        use QuantityValue::*;
        assert_eq!(
            quantity_math(Float(1.23456), "rad", Some(2), ",").unwrap(),
            r#"1upright(",")23 thin upright("rad")"#
        );
        assert_eq!(quantity_math(Integer(2), "", Some(1), ".").unwrap(), "2.0");
        assert_eq!(
            quantity_math(Float(-0.5), "", None, ",").unwrap(),
            r#"-0upright(",")5"#
        );
    }

    #[test]
    fn malformed_units_are_rejected() {
        use QuantityValue::*;
        for unit in ["/s", "m/", "m^", "m^x", "^2"] {
            assert!(
                quantity_math(Integer(1), unit, None, ".").is_err(),
                "{unit:?}"
            );
        }
        assert!(quantity_math(Float(f64::NAN), "m", None, ".").is_err());
        assert!(quantity_math(Integer(1), "m", None, "").is_err());
    }
}
