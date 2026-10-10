//! Names of values as people read them in the editor.

use std::fmt::{Debug, Write};

/// The variant name `Debug` gives `value`, in snake case: `RoundedRect(..)`
/// is `rounded_rect`. Only the name is formatted, however large the data.
pub fn variant_name(value: &impl Debug) -> String {
    /// Keeps what `Debug` writes up to the first character that cannot be
    /// part of a name, then stops it.
    struct Head(String);
    impl Write for Head {
        fn write_str(&mut self, text: &str) -> std::fmt::Result {
            for ch in text.chars() {
                if !(ch.is_alphanumeric() || ch == '_') {
                    return Err(std::fmt::Error);
                }
                self.0.push(ch);
            }
            Ok(())
        }
    }
    let mut head = Head(String::new());
    let _ = write!(head, "{value:?}");
    let mut name = String::with_capacity(head.0.len() + 4);
    for (index, ch) in head.0.chars().enumerate() {
        if ch.is_uppercase() {
            if index > 0 {
                name.push('_');
            }
            name.extend(ch.to_lowercase());
        } else {
            name.push(ch);
        }
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    #[allow(dead_code)]
    enum Shape {
        RoundedRect(f64, f64),
        Circle { radius: f64 },
        Group,
    }

    #[test]
    fn variants_are_named_in_snake_case() {
        assert_eq!(variant_name(&Shape::RoundedRect(1.0, 2.0)), "rounded_rect");
        assert_eq!(variant_name(&Shape::Circle { radius: 1.0 }), "circle");
        assert_eq!(variant_name(&Shape::Group), "group");
    }
}
