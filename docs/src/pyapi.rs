//! Extracts the public Python API from the typed stub into a search index.
//!
//! The stub is the source of truth for the callable surface, so the site's
//! API search covers every public class, method and constant even when no
//! hand-written reference entry exists yet.

use serde_json::{Value, json};

/// One searchable symbol of the Python API.
#[derive(Debug, Clone, PartialEq)]
pub struct ApiSymbol {
    /// Qualified name, e.g. `Anim.grow_arrow` or `Scene`.
    pub name: String,
    /// Owning class, when the symbol is a member.
    pub class: Option<String>,
    /// `class`, `method`, `property`, `constructor`, `function`, `attribute`,
    /// `constant` or `type`.
    pub kind: &'static str,
    /// Call signature without `self`, e.g. `grow_arrow() -> Anim`.
    pub signature: String,
    /// First paragraph of the docstring, whitespace-collapsed.
    pub summary: String,
}

impl ApiSymbol {
    fn to_json(&self) -> Value {
        json!({
            "name": self.name,
            "class": self.class,
            "kind": self.kind,
            "signature": self.signature,
            "summary": self.summary,
        })
    }
}

/// Parse a `.pyi` stub into public symbols. Private names (leading `_`, other
/// than `__init__`) and overload duplicates are skipped.
pub fn parse_stub(source: &str) -> Vec<ApiSymbol> {
    let lines: Vec<&str> = source.lines().collect();
    let mut symbols: Vec<ApiSymbol> = Vec::new();
    let mut class: Option<String> = None;
    let mut decorators: Vec<String> = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let indent = line.len() - line.trim_start().len();
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            i += 1;
            continue;
        }
        if indent == 0 && !trimmed.starts_with('@') {
            class = None;
        }

        if let Some(decorator) = trimmed.strip_prefix('@') {
            decorators.push(decorator.to_string());
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("class ").filter(|_| indent == 0) {
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            let (summary, next) = docstring_after(&lines, i + 1);
            if !name.starts_with('_') {
                symbols.push(ApiSymbol {
                    name: name.clone(),
                    class: None,
                    kind: "class",
                    signature: format!("class {}", rest.trim_end_matches(':')),
                    summary,
                });
            }
            class = Some(name);
            decorators.clear();
            i = next;
            continue;
        }

        let is_member = indent > 0 && class.is_some();
        if trimmed.starts_with("def ") && (indent == 0 || is_member) {
            // Gather a possibly multi-line signature until its parentheses
            // balance and the header ends.
            let mut header = trimmed.to_string();
            let mut j = i;
            while !header_complete(&header) && j + 1 < lines.len() {
                j += 1;
                header.push(' ');
                header.push_str(lines[j].trim());
            }
            let (summary, next) = docstring_after(&lines, j + 1);
            let owner = if indent == 0 { None } else { class.clone() };
            if let Some(symbol) = def_symbol(&header, owner, &decorators, summary)
                && !symbols
                    .iter()
                    .any(|s| s.name == symbol.name && s.kind == symbol.kind)
            {
                symbols.push(symbol);
            }
            decorators.clear();
            i = next;
            continue;
        }

        // `NAME: Type` constants and class attributes, `Alias: TypeAlias = ...`.
        if let Some((name, annotation)) = trimmed.split_once(':')
            && is_identifier(name)
            && !name.starts_with('_')
            && (indent == 0 || is_member)
        {
            let annotation = annotation.trim();
            let (summary, next) = docstring_after(&lines, i + 1);
            let (kind, qualified, owner) = match (&class, indent) {
                (Some(owner), n) if n > 0 => {
                    ("attribute", format!("{owner}.{name}"), Some(owner.clone()))
                }
                _ if annotation.starts_with("TypeAlias") => ("type", name.to_string(), None),
                _ => ("constant", name.to_string(), None),
            };
            let annotation = annotation
                .trim_start_matches("ClassVar[")
                .trim_end_matches(']');
            let signature = if kind == "type" {
                annotation
                    .split_once('=')
                    .map(|(_, value)| format!("{name} = {}", value.trim()))
                    .unwrap_or_else(|| name.to_string())
            } else {
                format!("{name}: {annotation}")
            };
            symbols.push(ApiSymbol {
                name: qualified,
                class: owner,
                kind,
                signature,
                summary,
            });
            decorators.clear();
            i = next;
            continue;
        }

        decorators.clear();
        i += 1;
    }
    symbols
}

/// Serialize parsed symbols as the JSON array consumed by the site search.
pub fn index_json(source: &str) -> String {
    Value::Array(parse_stub(source).iter().map(ApiSymbol::to_json).collect()).to_string()
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_')
}

fn header_complete(header: &str) -> bool {
    let depth = header.chars().fold(0i32, |depth, c| match c {
        '(' | '[' => depth + 1,
        ')' | ']' => depth - 1,
        _ => depth,
    });
    depth == 0 && (header.ends_with(':') || header.ends_with("..."))
}

fn def_symbol(
    header: &str,
    class: Option<String>,
    decorators: &[String],
    summary: String,
) -> Option<ApiSymbol> {
    let rest = header.strip_prefix("def ")?;
    let (name, after_name) = rest.split_once('(')?;
    let name = name.trim();
    if name.starts_with('_') && name != "__init__" {
        return None;
    }

    // Split the parameter list at its matching closing parenthesis.
    let mut depth = 1;
    let mut close = None;
    for (index, c) in after_name.char_indices() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(index);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close?;
    let params = after_name[..close].trim().trim_end_matches(',');
    let params = ["self, ", "self", "cls, ", "cls"]
        .iter()
        .find_map(|prefix| params.strip_prefix(prefix))
        .unwrap_or(params)
        .trim();
    let returns = after_name[close + 1..]
        .trim()
        .trim_end_matches("...")
        .trim()
        .trim_end_matches(':')
        .trim()
        .strip_prefix("->")
        .map(|r| format!(" -> {}", r.trim()))
        .unwrap_or_default();

    let is_property = decorators.iter().any(|d| d == "property");
    let (kind, display, qualified) = match &class {
        Some(owner) if name == "__init__" => ("constructor", owner.clone(), owner.clone()),
        Some(owner) if is_property => ("property", name.to_string(), format!("{owner}.{name}")),
        Some(owner) => ("method", name.to_string(), format!("{owner}.{name}")),
        None => ("function", name.to_string(), name.to_string()),
    };
    let signature = if kind == "property" {
        format!("{display}{}", returns.replacen(" -> ", ": ", 1))
    } else {
        format!("{display}({params}){returns}")
    };
    Some(ApiSymbol {
        name: qualified,
        class,
        kind,
        signature,
        summary,
    })
}

/// Return the first docstring paragraph starting at `start` (if the next
/// non-blank line opens one) and the index just past the docstring.
fn docstring_after(lines: &[&str], start: usize) -> (String, usize) {
    let mut i = start;
    while i < lines.len() && lines[i].trim().is_empty() {
        i += 1;
    }
    let Some(first) = lines.get(i).map(|l| l.trim()) else {
        return (String::new(), start);
    };
    let Some(opened) = first.strip_prefix("\"\"\"") else {
        return (String::new(), start);
    };

    let mut paragraph: Vec<String> = Vec::new();
    let mut in_first_paragraph = true;
    let mut push = |text: &str, paragraph: &mut Vec<String>| {
        if text.is_empty() {
            if !paragraph.is_empty() {
                in_first_paragraph = false;
            }
        } else if in_first_paragraph {
            paragraph.push(text.to_string());
        }
    };

    if let Some(body) = opened.strip_suffix("\"\"\"") {
        push(body.trim(), &mut paragraph);
        return (paragraph.join(" "), i + 1);
    }
    push(opened.trim(), &mut paragraph);
    let mut j = i + 1;
    while j < lines.len() {
        let text = lines[j].trim();
        if let Some(body) = text.strip_suffix("\"\"\"") {
            push(body.trim(), &mut paragraph);
            j += 1;
            break;
        }
        push(text, &mut paragraph);
        j += 1;
    }
    (paragraph.join(" "), j)
}

#[cfg(test)]
mod tests {
    use super::*;

    const STUB: &str = r#"
"""Module docstring."""
from typing import overload

CurvePoint: TypeAlias = tuple[float, float]
"""A coordinate pair."""

class Anim:
    QUAD: ClassVar[EasingCurve]
    def grow_arrow(self) -> Anim:
        """Grow an arrow from its tail.

        Second paragraph is not part of the summary.
        """
        ...
    def fade_in(self) -> Anim: ...
    def _private(self) -> None: ...

class Scene:
    """A resolution-independent scene."""
    def __init__(
        self,
        *,
        frame: tuple[float, float] = (16.0, 9.0),
    ) -> None:
        """Create a scene."""
        ...
    @property
    def geometry(self) -> Geometry:
        """Shape factories."""
        ...
    @overload
    def pan_to(self, x: float, y: float) -> Anim: ...
    @overload
    def pan_to(self, target: Endpoint) -> Anim: ...

def parallel(*items: Playable) -> Composition: ...

GOLD: Color
"#;

    fn find<'a>(symbols: &'a [ApiSymbol], name: &str) -> &'a ApiSymbol {
        symbols
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("missing {name}: {symbols:#?}"))
    }

    #[test]
    fn extracts_members_with_signatures_and_summaries() {
        let symbols = parse_stub(STUB);
        let grow = find(&symbols, "Anim.grow_arrow");
        assert_eq!(grow.kind, "method");
        assert_eq!(grow.class.as_deref(), Some("Anim"));
        assert_eq!(grow.signature, "grow_arrow() -> Anim");
        assert_eq!(grow.summary, "Grow an arrow from its tail.");
        assert_eq!(find(&symbols, "Anim.fade_in").summary, "");
        assert_eq!(find(&symbols, "Anim.QUAD").kind, "attribute");
        assert!(symbols.iter().all(|s| !s.name.contains("_private")));
    }

    #[test]
    fn handles_classes_constructors_properties_overloads_and_constants() {
        let symbols = parse_stub(STUB);
        let scene = find(&symbols, "Scene");
        assert_eq!(scene.kind, "class");
        assert_eq!(scene.summary, "A resolution-independent scene.");
        let ctor = symbols.iter().find(|s| s.kind == "constructor").unwrap();
        assert_eq!(ctor.name, "Scene");
        assert_eq!(
            ctor.signature,
            "Scene(*, frame: tuple[float, float] = (16.0, 9.0)) -> None"
        );
        assert_eq!(
            find(&symbols, "Scene.geometry").signature,
            "geometry: Geometry"
        );
        assert_eq!(
            symbols.iter().filter(|s| s.name == "Scene.pan_to").count(),
            1,
            "overloads collapse to one entry"
        );
        assert_eq!(find(&symbols, "parallel").kind, "function");
        assert_eq!(find(&symbols, "GOLD").kind, "constant");
        assert_eq!(find(&symbols, "CurvePoint").kind, "type");
        assert_eq!(find(&symbols, "CurvePoint").summary, "A coordinate pair.");
    }

    #[test]
    fn real_stub_exposes_the_public_surface() {
        let source = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../crates/gaanim_python/gaanim/gaanim_core.pyi"
        ))
        .expect("stub");
        let symbols = parse_stub(&source);
        assert!(symbols.len() > 500, "only {} symbols", symbols.len());
        for name in [
            "Scene",
            "Scene.play",
            "Anim.grow_from_center",
            "Drawable.move_to",
            "BLUE",
        ] {
            find(&symbols, name);
        }
    }
}
