//! The audience's characters: the parts and motion of
//! `crates/gaanim_project/relay/public/avatar-parts.json`, the file the
//! relay's voting page draws from, so a character looks and moves the same
//! on a phone and in a presentation.
//!
//! A character is five indexes, [body, color, eyes, mouth, extra].
//! [`CharacterCatalog::pose`] draws it at a time: breathing, blinking and,
//! optionally, playing an expression. It is a pure function of its inputs
//! and follows the catalog's `about` rules, as the page's `avatarPose` does.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use kurbo::{Affine, BezPath};
use peniko::Color;
use serde::Deserialize;

/// The catalog the relay serves, compiled in.
const CATALOG_JSON: &str = include_str!("../../gaanim_project/relay/public/avatar-parts.json");

/// How many indexes a character has: body, color, eyes, mouth, extra.
pub const CHARACTER_PARTS: usize = 5;

/// A character: indexes into the catalog's bodies, colors, eyes, mouths and
/// extras.
pub type Character = [usize; CHARACTER_PARTS];

/// The catalog compiled into Gaanim.
pub fn catalog() -> &'static CharacterCatalog {
    static CATALOG: OnceLock<CharacterCatalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        CharacterCatalog::from_json(CATALOG_JSON).expect("the compiled character catalog is valid")
    })
}

/// One path of a character's drawing, in its 100-unit square.
#[derive(Debug, Clone)]
pub struct CharacterLayer {
    pub path: Arc<BezPath>,
    pub color: Color,
    /// Stroke width with round caps, or `None` for a fill.
    pub stroke: Option<f64>,
    /// The layer as a filled outline: `path` itself, or its stroke with
    /// round caps and joins. Transform it like `path`.
    pub outline: Arc<BezPath>,
    /// Where the layer sits now: breathing, blinking and the expression's
    /// motion.
    pub transform: Affine,
}

/// The drawing's envelope in its 100-unit square: room for hats, ears and
/// headphones around the body. A character's bounds are this box, so
/// layouts do not move as it breathes or jumps.
pub const CHARACTER_ENVELOPE: kurbo::Rect = kurbo::Rect::new(-6.0, -30.0, 106.0, 96.0);

/// From the 100-unit square (y down) to a scene where the character is
/// `size` units tall (the envelope's height), centered on the origin, y up.
pub fn to_scene(size: f64) -> Affine {
    let scale = size / CHARACTER_ENVELOPE.height();
    let center = CHARACTER_ENVELOPE.center();
    Affine::scale_non_uniform(scale, -scale) * Affine::translate((-center.x, -center.y))
}

/// An expression playing: `start` and the pose's time share one clock.
#[derive(Debug, Clone, PartialEq)]
pub struct ExpressionPlay {
    pub name: String,
    pub start: f64,
    /// Repeat the motion and keep the face until another expression.
    pub looped: bool,
}

#[derive(Debug, Clone)]
struct Layer {
    path: Arc<BezPath>,
    outline: Arc<BezPath>,
    paint: String,
    stroke: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct RawLayer {
    d: String,
    #[serde(default)]
    fill: Option<String>,
    #[serde(default)]
    stroke: Option<String>,
    #[serde(default)]
    width: Option<f64>,
}

fn layers<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Vec<Layer>, D::Error> {
    let raw = Vec::<RawLayer>::deserialize(deserializer)?;
    raw.into_iter()
        .map(|layer| {
            let path = BezPath::from_svg(&layer.d)
                .map_err(|error| serde::de::Error::custom(format!("{:?}: {error}", layer.d)))?;
            let (paint, stroke) = match (layer.fill, layer.stroke) {
                (_, Some(stroke)) => (stroke, Some(layer.width.unwrap_or(3.0))),
                (Some(fill), None) => (fill, None),
                (None, None) => {
                    return Err(serde::de::Error::custom("a layer needs fill or stroke"));
                }
            };
            let outline = match stroke {
                Some(width) => kurbo::stroke(
                    path.iter(),
                    &kurbo::Stroke::new(width)
                        .with_caps(kurbo::Cap::Round)
                        .with_join(kurbo::Join::Round),
                    &kurbo::StrokeOpts::default(),
                    0.01,
                ),
                None => path.clone(),
            };
            Ok(Layer {
                path: Arc::new(path),
                outline: Arc::new(outline),
                paint,
                stroke,
            })
        })
        .collect()
}

#[derive(Debug, Deserialize)]
struct Body {
    top: f64,
    face: f64,
    chin: f64,
    base: f64,
    #[serde(deserialize_with = "layers")]
    layers: Vec<Layer>,
}

impl Body {
    fn anchor(&self, name: &str) -> f64 {
        match name {
            "face" => self.face,
            "chin" => self.chin,
            _ => self.top,
        }
    }
}

#[derive(Debug, Deserialize)]
struct Part {
    name: String,
    #[serde(default)]
    anchor: Option<String>,
    #[serde(default)]
    behind: bool,
    #[serde(default)]
    keep: bool,
    #[serde(default = "yes")]
    blink: bool,
    #[serde(deserialize_with = "layers")]
    layers: Vec<Layer>,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Deserialize)]
struct Face {
    #[serde(deserialize_with = "layers")]
    layers: Vec<Layer>,
}

#[derive(Debug, Deserialize)]
struct Effect {
    anchor: String,
    motion: String,
    pivot: (f64, f64),
    #[serde(deserialize_with = "layers")]
    layers: Vec<Layer>,
}

#[derive(Debug, Deserialize)]
struct Motion {
    duration: f64,
    #[serde(default)]
    ease: Option<String>,
    /// [p, dx, dy, sx, sy, rot]
    keys: Vec<[f64; 6]>,
}

impl Motion {
    /// [dx, dy, sx, sy, rot] at progress `p`.
    fn at(&self, p: f64) -> [f64; 5] {
        let keys = &self.keys;
        let mut index = 0;
        while index + 2 < keys.len() && p > keys[index + 1][0] {
            index += 1;
        }
        let from = keys[index];
        let to = keys[(index + 1).min(keys.len() - 1)];
        let span = to[0] - from[0];
        let mut s = if span > 0.0 {
            ((p - from[0]) / span).clamp(0.0, 1.0)
        } else {
            1.0
        };
        if self.ease.as_deref() != Some("linear") {
            s = s * s * (3.0 - 2.0 * s);
        }
        std::array::from_fn(|i| from[i + 1] + (to[i + 1] - from[i + 1]) * s)
    }
}

#[derive(Debug, Deserialize)]
struct Breathe {
    period: f64,
    amount: f64,
}

#[derive(Debug, Deserialize)]
struct Blink {
    every: (f64, f64),
    duration: f64,
}

#[derive(Debug, Deserialize)]
struct Idle {
    breathe: Breathe,
    blink: Blink,
}

#[derive(Debug, Deserialize)]
struct Expression {
    #[serde(default)]
    eyes: Option<String>,
    #[serde(default)]
    mouth: Option<String>,
    #[serde(default)]
    motion: Option<String>,
    #[serde(default)]
    effects: Vec<String>,
    hold: f64,
}

/// The parts characters are made of and how they move.
#[derive(Debug, Deserialize)]
pub struct CharacterCatalog {
    ink: String,
    white: String,
    colors: Vec<String>,
    bodies: Vec<Body>,
    eyes: Vec<Part>,
    mouths: Vec<Part>,
    extras: Vec<Part>,
    faces: HashMap<String, Face>,
    effects: HashMap<String, Effect>,
    motions: HashMap<String, Motion>,
    idle: Idle,
    expressions: HashMap<String, Expression>,
}

/// translate(pivot + (dx, dy)) rotate(rot°) scale(sx, sy) translate(-pivot).
fn motion_affine(pivot: (f64, f64), [dx, dy, sx, sy, rot]: [f64; 5]) -> Affine {
    Affine::translate((pivot.0 + dx, pivot.1 + dy))
        * Affine::rotate(rot.to_radians())
        * Affine::scale_non_uniform(sx, sy)
        * Affine::translate((-pivot.0, -pivot.1))
}

/// `#rrggbb` as a color.
fn hex_color(text: &str) -> Option<Color> {
    let hex = text.strip_prefix('#').filter(|hex| hex.len() == 6)?;
    let channel = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).ok();
    Some(Color::from_rgb8(channel(0)?, channel(2)?, channel(4)?))
}

/// FNV-1a of `text`'s UTF-8 bytes: the seed of a character's blinking.
pub fn character_seed(text: &str) -> u32 {
    text.bytes().fold(0x811c_9dc5, |hash: u32, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    })
}

impl CharacterCatalog {
    pub fn from_json(json: &str) -> Result<Self, String> {
        let catalog: Self = serde_json::from_str(json).map_err(|error| error.to_string())?;
        let paints = catalog
            .bodies
            .iter()
            .flat_map(|body| &body.layers)
            .chain(catalog.eyes.iter().flat_map(|part| &part.layers))
            .chain(catalog.mouths.iter().flat_map(|part| &part.layers))
            .chain(catalog.extras.iter().flat_map(|part| &part.layers))
            .chain(catalog.faces.values().flat_map(|face| &face.layers))
            .chain(catalog.effects.values().flat_map(|effect| &effect.layers))
            .map(|layer| layer.paint.as_str())
            .chain(catalog.colors.iter().map(String::as_str))
            .chain([catalog.ink.as_str(), catalog.white.as_str()]);
        for paint in paints {
            if !matches!(paint, "body" | "ink" | "white") && hex_color(paint).is_none() {
                return Err(format!("{paint:?} is not a color of the character catalog"));
            }
        }
        for expression in catalog.expressions.values() {
            let known = |name: &Option<String>, list: &[Part]| {
                name.as_ref().is_none_or(|name| {
                    catalog.faces.contains_key(name) || list.iter().any(|part| &part.name == name)
                })
            };
            let effects_known = expression.effects.iter().all(|effect| {
                catalog
                    .effects
                    .get(effect)
                    .is_some_and(|effect| catalog.motions.contains_key(&effect.motion))
            });
            let motion_known = expression
                .motion
                .as_ref()
                .is_none_or(|motion| catalog.motions.contains_key(motion));
            if !(known(&expression.eyes, &catalog.eyes)
                && known(&expression.mouth, &catalog.mouths)
                && effects_known
                && motion_known)
            {
                return Err("an expression names a part the catalog lacks".into());
            }
        }
        Ok(catalog)
    }

    /// How many choices each of a character's indexes has.
    pub fn counts(&self) -> Character {
        [
            self.bodies.len(),
            self.colors.len(),
            self.eyes.len(),
            self.mouths.len(),
            self.extras.len(),
        ]
    }

    /// Whether `character` indexes this catalog.
    pub fn contains(&self, character: &Character) -> bool {
        character
            .iter()
            .zip(self.counts())
            .all(|(index, count)| *index < count)
    }

    /// The most layers a pose of any character can have: how many paths to
    /// keep ready for one.
    pub fn max_layers(&self) -> usize {
        let most = |lists: &[&[Layer]]| lists.iter().map(|layers| layers.len()).max().unwrap_or(0);
        let parts = |parts: &[Part]| {
            parts
                .iter()
                .map(|part| part.layers.len())
                .max()
                .unwrap_or(0)
        };
        let faces = most(
            &self
                .faces
                .values()
                .map(|face| face.layers.as_slice())
                .collect::<Vec<_>>(),
        );
        let effects = self
            .expressions
            .values()
            .map(|expression| {
                expression
                    .effects
                    .iter()
                    .filter_map(|name| self.effects.get(name))
                    .map(|effect| effect.layers.len())
                    .sum::<usize>()
            })
            .max()
            .unwrap_or(0);
        most(
            &self
                .bodies
                .iter()
                .map(|body| body.layers.as_slice())
                .collect::<Vec<_>>(),
        ) + parts(&self.eyes).max(faces)
            + parts(&self.mouths).max(faces)
            + parts(&self.extras)
            + effects
    }

    /// A character read from `seed`, the same every time, as the relay
    /// gives a phone that did not choose one.
    pub fn character_from_seed(&self, seed: u32) -> Character {
        let counts = self.counts();
        let mut state = seed;
        std::array::from_fn(|part| {
            // xorshift32: spreads the bits of a small seed.
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as usize % counts[part].max(1)
        })
    }

    /// Where `character` stands, in its 100-unit square: the y of the base
    /// of its body.
    pub fn feet(&self, character: &Character) -> f64 {
        self.bodies[character[0] % self.bodies.len().max(1)].base
    }

    /// The expressions characters can play, sorted.
    pub fn expressions(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.expressions.keys().map(String::as_str).collect();
        names.sort_unstable();
        names
    }

    fn paint(&self, character: &Character, name: &str) -> Color {
        let text = match name {
            "body" => &self.colors[character[1]],
            "ink" => &self.ink,
            "white" => &self.white,
            other => other,
        };
        hex_color(text).unwrap_or(Color::BLACK)
    }

    fn face_part<'a>(&'a self, list: &'a [Part], name: &str) -> Option<&'a [Layer]> {
        self.faces
            .get(name)
            .map(|face| face.layers.as_slice())
            .or_else(|| {
                list.iter()
                    .find(|part| part.name == name)
                    .map(|part| part.layers.as_slice())
            })
    }

    /// `character` at time `t`, back to front, in its 100-unit square. `seed`
    /// ([`character_seed`] of the nickname) sets its blinking; `expression`
    /// plays on the same clock as `t`. Indexes past the catalog wrap.
    pub fn pose(
        &self,
        character: &Character,
        seed: u32,
        t: f64,
        expression: Option<&ExpressionPlay>,
    ) -> Vec<CharacterLayer> {
        let character: Character =
            std::array::from_fn(|part| character[part] % self.counts()[part].max(1));
        let body = &self.bodies[character[0]];
        let own_eyes = &self.eyes[character[2]];
        let extra = &self.extras[character[4]];
        let base = (50.0, body.base);

        let breathe = &self.idle.breathe;
        let breath = breathe.amount * (std::f64::consts::TAU * t / breathe.period).sin();
        let mut whole = motion_affine(base, [0.0, 0.0, 1.0 - breath / 2.0, 1.0 + breath, 0.0]);

        let mut eyes: &[Layer] = &own_eyes.layers;
        let mut own = true;
        let mut mouth: &[Layer] = &self.mouths[character[3]].layers;
        let mut effects = Vec::new();
        if let Some(play) = expression
            && let Some(shown) = self.expressions.get(&play.name)
        {
            let since = t - play.start;
            if since >= 0.0 && (play.looped || since < shown.hold) {
                if let Some(name) = &shown.eyes
                    && !own_eyes.keep
                    && let Some(layers) = self.face_part(&self.eyes, name)
                {
                    eyes = layers;
                    own = false;
                }
                if let Some(name) = &shown.mouth
                    && let Some(layers) = self.face_part(&self.mouths, name)
                {
                    mouth = layers;
                }
                if let Some(motion) = shown
                    .motion
                    .as_ref()
                    .and_then(|name| self.motions.get(name))
                {
                    let p = if play.looped {
                        since.rem_euclid(motion.duration) / motion.duration
                    } else {
                        (since / motion.duration).min(1.0)
                    };
                    whole = motion_affine(base, motion.at(p)) * whole;
                }
                for effect in shown
                    .effects
                    .iter()
                    .filter_map(|name| self.effects.get(name))
                {
                    let motion = &self.motions[&effect.motion];
                    let p = since.rem_euclid(motion.duration) / motion.duration;
                    let at = Affine::translate((0.0, body.anchor(&effect.anchor)))
                        * motion_affine(effect.pivot, motion.at(p));
                    effects.push((&effect.layers, at));
                }
            }
        }

        let mut blink = 1.0;
        if own && own_eyes.blink {
            let every = self.idle.blink.every;
            let duration = self.idle.blink.duration;
            let period = every.0 + (every.1 - every.0) * f64::from(seed & 0xffff) / 65535.0;
            let offset = period * f64::from((seed >> 16) & 0xffff) / 65535.0;
            let x = (t + offset).rem_euclid(period);
            if x < duration {
                blink = 1.0 - 0.9 * (std::f64::consts::PI * x / duration).sin();
            }
        }
        let face = Affine::translate((0.0, body.face - 50.0));
        let eyes_at = face * motion_affine((50.0, 50.0), [0.0, 0.0, 1.0, blink, 0.0]);
        let extra_at =
            Affine::translate((0.0, body.anchor(extra.anchor.as_deref().unwrap_or("top"))));

        let mut out = Vec::new();
        let mut push = |layers: &[Layer], at: Affine| {
            for layer in layers {
                out.push(CharacterLayer {
                    path: layer.path.clone(),
                    outline: layer.outline.clone(),
                    color: self.paint(&character, &layer.paint),
                    stroke: layer.stroke,
                    transform: whole * at,
                });
            }
        };
        if extra.behind {
            push(&extra.layers, extra_at);
        }
        push(&body.layers, Affine::IDENTITY);
        push(eyes, eyes_at);
        push(mouth, face);
        if !extra.behind {
            push(&extra.layers, extra_at);
        }
        for (layers, at) in effects {
            push(layers, at);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn play(name: &str, start: f64, looped: bool) -> ExpressionPlay {
        ExpressionPlay {
            name: name.into(),
            start,
            looped,
        }
    }

    #[test]
    fn the_compiled_catalog_loads_with_its_expressions() {
        let catalog = catalog();
        assert_eq!(catalog.counts(), [7, 10, 7, 7, 9]);
        assert_eq!(
            catalog.expressions(),
            ["happy", "hurt", "sad", "surprised", "winner"]
        );
        assert!(catalog.contains(&[6, 9, 6, 6, 8]));
        assert!(!catalog.contains(&[7, 0, 0, 0, 0]));
    }

    #[test]
    fn the_seed_is_fnv1a_of_the_nickname() {
        // The page hashes the same bytes: "Ana" and a nickname with accents.
        assert_eq!(character_seed(""), 0x811c_9dc5);
        assert_eq!(character_seed("a"), 0xe40c_292c);
        assert_eq!(character_seed("Gato Épico"), character_seed("Gato Épico"));
    }

    #[test]
    fn an_expression_changes_the_face_while_it_holds() {
        let catalog = catalog();
        // A round, dot-eyed, smiling character without an extra.
        let character = [0, 0, 0, 1, 0];
        let calm = catalog.pose(&character, 7, 1.0, None);
        let sad = catalog.pose(&character, 7, 1.0, Some(&play("sad", 0.5, false)));
        // Sad eyes add brows; the tear is an effect on top.
        assert!(sad.len() > calm.len());
        // After its hold the face is the character's again.
        let after = catalog.pose(&character, 7, 3.0, Some(&play("sad", 0.5, false)));
        assert_eq!(after.len(), calm.len());
        // A looped expression keeps going.
        let looped = catalog.pose(&character, 7, 30.0, Some(&play("winner", 0.5, true)));
        assert!(looped.len() > calm.len());
    }

    #[test]
    fn glasses_stay_on_through_expressions() {
        let catalog = catalog();
        let glasses = [0, 0, 4, 1, 0];
        let calm = catalog.pose(&glasses, 7, 1.0, None);
        let happy = catalog.pose(&glasses, 7, 1.0, Some(&play("happy", 0.9, false)));
        // Body and eyes are the same paths; only the mouth changes.
        assert!(Arc::ptr_eq(&calm[1].path, &happy[1].path));
    }

    #[test]
    fn motion_keys_interpolate_with_smoothstep_or_linearly() {
        let catalog = catalog();
        let hop = &catalog.motions["hop"];
        assert_eq!(hop.at(0.0), [0.0, 0.0, 1.0, 1.0, 0.0]);
        assert_eq!(hop.at(1.0), [0.0, 0.0, 1.0, 1.0, 0.0]);
        let orbit = &catalog.motions["orbit"];
        assert!((orbit.at(0.25)[4] - 90.0).abs() < 1e-9);
    }
}

#[cfg(test)]
mod parity {
    use super::*;

    /// Prints poses for comparing with the page's `avatarPose`:
    /// `cargo test -p gaanim_objects parity -- --ignored --nocapture`.
    #[test]
    #[ignore = "prints poses to compare with avatar.js"]
    fn print_poses() {
        let catalog = catalog();
        for (character, name) in PARITY_CASES {
            let seed = character_seed(name);
            for t in PARITY_TIMES {
                for (expression, start, looped) in PARITY_EXPRESSIONS {
                    let play = (!expression.is_empty()).then(|| ExpressionPlay {
                        name: expression.to_string(),
                        start,
                        looped,
                    });
                    for layer in catalog.pose(&character, seed, t, play.as_ref()) {
                        let [a, b, c, d, e, f] = layer.transform.as_coeffs();
                        let rgba = layer.color.to_rgba8();
                        println!(
                            "POSE {character:?} {t} {expression} {a:.6} {b:.6} {c:.6} {d:.6} {e:.6} {f:.6} {:02x}{:02x}{:02x} {:?}",
                            rgba.r, rgba.g, rgba.b, layer.stroke
                        );
                    }
                }
            }
        }
    }

    pub(super) const PARITY_CASES: [(Character, &str); 4] = [
        ([0, 0, 0, 1, 0], "Ana"),
        ([1, 3, 4, 2, 3], "Gato Épico"),
        ([3, 4, 6, 6, 4], "Beto"),
        ([5, 7, 1, 5, 7], "Ñandú Veloz"),
    ];
    pub(super) const PARITY_TIMES: [f64; 5] = [0.0, 0.37, 1.1, 2.93, 17.5];
    pub(super) const PARITY_EXPRESSIONS: [(&str, f64, bool); 6] = [
        ("", 0.0, false),
        ("happy", 0.2, false),
        ("sad", 0.0, false),
        ("hurt", 0.1, true),
        ("winner", 0.0, true),
        ("surprised", 1.0, false),
    ];
}
