use bevy::prelude::{Changed, Commands, Component, DetectChangesMut, Entity, Query, Res, World};
use gaanim_core::glam::DVec3;
use gaanim_core::kurbo::{Affine, BezPath, PathEl, Point, Shape};
use gaanim_core::peniko::Color;
use gaanim_math::{Bounds3D, SpatialTransform};
use gaanim_scene::{LocalBounds, MobjectId, Path2D, PathSource, TextBaseline};
use std::collections::HashMap;
use std::sync::Arc;

/// A generic, observable reactive signal component.
///
/// Wraps any data type `T` inside Bevy ECS. Downstream reactive systems
/// can leverage Bevy's built-in change detection (`Changed<Signal<T>>`)
/// to trigger immediate, multi-threaded re-evaluations.
#[derive(Component, Debug, Clone)]
pub struct Signal<T: Send + Sync + Clone + 'static> {
    /// The current value of the signal.
    pub value: T,
}

impl<T: Send + Sync + Clone + 'static> Signal<T> {
    /// Creates a new reactive signal with an initial value.
    pub fn new(value: T) -> Self {
        Self { value }
    }
}

/// Helper aliases for common signal types.
pub type FloatSignal = Signal<f64>;
pub type Vec3Signal = Signal<gaanim_core::glam::DVec3>;
pub type ColorSignal = Signal<Color>;

/// Native numeric text driven by an expression and one or more float signals.
/// It lives in `gaanim_animation` so previews, headless snapshots, and exports
/// all run the same visualizer-phase update.
#[derive(Component, Debug, Clone)]
pub struct ReactiveReadout {
    pub source: crate::reactive::ScalarSource,
    pub parameters: Vec<(gaanim_core::ObjectId, Entity)>,
    pub format: String,
    pub prefix: String,
    pub suffix: String,
    pub invalid: String,
    /// Character shown between the integer and fractional digits.
    pub decimal_separator: char,
    pub font_family: String,
    /// Font weight (1..=1000) resolved like `scene.text`; `None` keeps the
    /// family's regular face.
    pub font_weight: Option<u16>,
    pub font_size: f64,
    pub last_text: String,
    pub last_path: Arc<BezPath>,
    pub last_bounds: Bounds3D,
    /// Width the number keeps inside a layout, 0 outside one: its box
    /// reaches this far left of its right edge, so the layout's cell holds
    /// the widest text it will show and the row around it stays still.
    pub reserve: f64,
}

/// `bounds`, right-aligned at `x = 0`, at least `reserve` wide.
pub fn reserve_readout_bounds(mut bounds: Bounds3D, reserve: f64) -> Bounds3D {
    bounds.min.x = bounds.min.x.min(bounds.max.x - reserve);
    bounds
}

/// Keeps the independently styleable pieces of a reactive readout laid out as
/// one equation row while the numeric outline changes width.
#[derive(Component, Debug, Clone)]
pub struct ReactiveReadoutLayout {
    pub label: Option<gaanim_core::ObjectId>,
    pub equals: Option<gaanim_core::ObjectId>,
    pub number: gaanim_core::ObjectId,
    pub unit: Option<gaanim_core::ObjectId>,
    pub spacing: f64,
    /// Horizontal alignment of the row on the group origin: `-1.0` starts it
    /// there, `0.0` centers it and `1.0` ends it there.
    pub align: f64,
}

/// Replace the `.` of a formatted number with `separator`. With a comma
/// separator, `,` grouping becomes `.` so `1,234.5` reads `1.234,5`.
pub fn localize_decimal_separator(number: &str, separator: char) -> String {
    if separator == '.' {
        return number.to_owned();
    }
    number
        .chars()
        .map(|c| match c {
            '.' => separator,
            ',' if separator == ',' => '.',
            c => c,
        })
        .collect()
}

/// A parsed Python format specification for one number:
/// `[[fill]align][sign][z][#][0][width][grouping][.precision][type]`.
#[derive(Clone, Debug, PartialEq)]
struct NumberFormat {
    fill: char,
    align: char,
    sign: char,
    no_negative_zero: bool,
    alternate: bool,
    width: usize,
    grouping: Option<char>,
    precision: Option<usize>,
    kind: Option<char>,
}

/// Check that `specification` is a Python format specification for a
/// float, as `format(value, specification)` accepts it.
pub fn validate_number_format(specification: &str) -> Result<(), String> {
    parse_number_format(specification).map(|_| ())
}

fn parse_number_format(specification: &str) -> Result<NumberFormat, String> {
    let invalid = |reason: &str| {
        Err(format!(
            "invalid number format {specification:?}: {reason}; use a Python format \
             specification for floats such as \".2f\", \"05.1f\", \"+,.0f\", \".1%\" or \".3e\""
        ))
    };
    let chars: Vec<char> = specification.chars().collect();
    let mut index = 0;
    let is_align = |c: char| matches!(c, '<' | '>' | '^' | '=');
    let (mut fill, mut align) = (' ', None);
    if chars.len() >= 2 && is_align(chars[1]) {
        (fill, align) = (chars[0], Some(chars[1]));
        index = 2;
    } else if chars.first().copied().is_some_and(is_align) {
        align = Some(chars[0]);
        index = 1;
    }
    let mut sign = '-';
    if let Some(&c @ ('+' | '-' | ' ')) = chars.get(index) {
        sign = c;
        index += 1;
    }
    let no_negative_zero = chars.get(index) == Some(&'z');
    index += usize::from(no_negative_zero);
    let alternate = chars.get(index) == Some(&'#');
    index += usize::from(alternate);
    if chars.get(index) == Some(&'0') {
        // A leading zero pads with zeros after the sign, unless an explicit
        // alignment says otherwise.
        if align.is_none() {
            (fill, align) = ('0', Some('='));
        }
        index += 1;
    }
    let digits = |index: &mut usize| {
        let start = *index;
        while chars.get(*index).is_some_and(char::is_ascii_digit) {
            *index += 1;
        }
        (*index > start).then(|| chars[start..*index].iter().collect::<String>())
    };
    let width = match digits(&mut index) {
        Some(text) => match text.parse::<usize>() {
            Ok(width) if width <= 256 => width,
            _ => return invalid("the width is above 256"),
        },
        None => 0,
    };
    let mut grouping = None;
    if let Some(&c @ (',' | '_')) = chars.get(index) {
        grouping = Some(c);
        index += 1;
    }
    let mut precision = None;
    if chars.get(index) == Some(&'.') {
        index += 1;
        match digits(&mut index).map(|text| text.parse::<usize>()) {
            Some(Ok(value)) if value <= 64 => precision = Some(value),
            Some(_) => return invalid("the precision is above 64"),
            None => return invalid("a precision must follow the point"),
        }
    }
    let kind = match chars.get(index) {
        None => None,
        Some(&c @ ('f' | 'F' | 'e' | 'E' | 'g' | 'G' | '%')) => {
            index += 1;
            Some(c)
        }
        Some(&c @ ('d' | 'n' | 'b' | 'o' | 'x' | 'X' | 'c' | 's')) => {
            return invalid(&format!(
                "the type {c:?} does not format floats (\".0f\" shows whole numbers)"
            ));
        }
        Some(&c) => return invalid(&format!("unexpected {c:?}")),
    };
    if index != chars.len() {
        return invalid(&format!("unexpected {:?}", chars[index]));
    }
    Ok(NumberFormat {
        fill,
        align: align.unwrap_or('>'),
        sign,
        no_negative_zero,
        alternate,
        width,
        grouping,
        precision,
        kind,
    })
}

/// Python's exponent notation: `1.50e+03`.
fn python_exponent(value: f64, precision: usize, upper: bool) -> String {
    let rust = format!("{value:.precision$e}");
    let (mantissa, exponent) = rust.split_once('e').expect("Rust exponent notation");
    let exponent: i32 = exponent.parse().expect("Rust exponent");
    let sign = if exponent < 0 { '-' } else { '+' };
    let e = if upper { 'E' } else { 'e' };
    format!("{mantissa}{e}{sign}{:02}", exponent.abs())
}

/// Python's general format: `precision` significant digits, fixed point
/// unless the exponent is below -4 or reaches the precision.
fn python_general(value: f64, precision: usize, alternate: bool, upper: bool) -> String {
    let precision = precision.max(1);
    let exponent = if value == 0.0 {
        0
    } else {
        // Round first: 9.99 at two digits is 10, whose exponent is 1.
        let rounded = format!("{value:.*e}", precision - 1);
        rounded
            .split_once('e')
            .expect("Rust exponent notation")
            .1
            .parse::<i32>()
            .expect("Rust exponent")
    };
    let mut text = if (-4..precision as i32).contains(&exponent) {
        let decimals = (precision as i32 - 1 - exponent).max(0) as usize;
        format!("{value:.decimals$}")
    } else {
        python_exponent(value, precision - 1, upper)
    };
    if !alternate {
        let (mantissa, exponent) = match text.find(['e', 'E']) {
            Some(at) => text.split_at(at),
            None => (text.as_str(), ""),
        };
        let mantissa = if mantissa.contains('.') {
            mantissa.trim_end_matches('0').trim_end_matches('.')
        } else {
            mantissa
        };
        text = format!("{mantissa}{exponent}");
    } else if !text.contains('.') {
        let at = text.find(['e', 'E']).unwrap_or(text.len());
        text.insert(at, '.');
    }
    text
}

fn group_digits(integer: &str, separator: char) -> String {
    let count = integer.chars().count();
    let mut grouped = String::with_capacity(count + count / 3);
    for (index, digit) in integer.chars().enumerate() {
        if index > 0 && (count - index).is_multiple_of(3) {
            grouped.push(separator);
        }
        grouped.push(digit);
    }
    grouped
}

/// Format `value` like Python's `format(value, specification)`, or
/// `invalid` when the value is not finite. An unsupported specification,
/// which [`validate_number_format`] rejects, falls back to `.2f`.
pub fn format_reactive_number(value: f64, specification: &str, invalid: &str) -> String {
    if !value.is_finite() {
        return invalid.to_owned();
    }
    let spec = parse_number_format(specification)
        .or_else(|_| parse_number_format(".2f"))
        .expect("the fallback format parses");
    let magnitude = if spec.kind == Some('%') {
        value.abs() * 100.0
    } else {
        value.abs()
    };
    let upper = matches!(spec.kind, Some('F' | 'E' | 'G'));
    let mut body = match spec.kind {
        Some('f' | 'F' | '%') => {
            let precision = spec.precision.unwrap_or(6);
            let mut text = format!("{magnitude:.precision$}");
            if spec.alternate && precision == 0 {
                text.push('.');
            }
            text
        }
        Some('e' | 'E') => python_exponent(magnitude, spec.precision.unwrap_or(6), upper),
        Some(_) => python_general(
            magnitude,
            spec.precision.unwrap_or(6),
            spec.alternate,
            upper,
        ),
        None => match spec.precision {
            // Like `g`, but a fixed-point result keeps one decimal.
            Some(precision) => {
                let mut text = python_general(magnitude, precision, spec.alternate, false);
                if !text.contains(['.', 'e']) {
                    text.push_str(".0");
                }
                text
            }
            // Like `str(value)`: the shortest text that reads back exactly.
            None if magnitude != 0.0 && !(1e-4..1e16).contains(&magnitude) => {
                let shortest = format!("{magnitude:e}");
                let (mantissa, exponent) =
                    shortest.split_once('e').expect("Rust exponent notation");
                let exponent: i32 = exponent.parse().expect("Rust exponent");
                let sign = if exponent < 0 { '-' } else { '+' };
                format!("{mantissa}e{sign}{:02}", exponent.abs())
            }
            None => {
                let mut text = format!("{magnitude}");
                if !text.contains('.') {
                    text.push_str(".0");
                }
                text
            }
        },
    };
    if spec.kind == Some('%') {
        body.push('%');
    }
    // A value that rounds to zero keeps its sign unless `z` drops it.
    let rounds_to_zero = !body.chars().any(|c| c.is_ascii_digit() && c != '0');
    let negative = value.is_sign_negative() && !(spec.no_negative_zero && rounds_to_zero);
    let sign = match (negative, spec.sign) {
        (true, _) => "-",
        (false, '+') => "+",
        (false, ' ') => " ",
        _ => "",
    };
    if let Some(separator) = spec.grouping {
        let split = body.find(['.', 'e', 'E', '%']).unwrap_or(body.len());
        let (integer, rest) = body.split_at(split);
        let mut integer = integer.to_owned();
        let mut grouped = group_digits(&integer, separator);
        // Zero padding is grouped too: `015,.2f` reads `0,001,234,567.89`.
        if spec.fill == '0' && spec.align == '=' {
            let others = sign.len() + rest.chars().count();
            while others + grouped.chars().count() < spec.width {
                integer.insert(0, '0');
                grouped = group_digits(&integer, separator);
            }
        }
        body = format!("{grouped}{rest}");
    }
    let length = sign.chars().count() + body.chars().count();
    let padding = spec.width.saturating_sub(length);
    let fill = |count: usize| spec.fill.to_string().repeat(count);
    match spec.align {
        '<' => format!("{sign}{body}{}", fill(padding)),
        '^' => format!(
            "{}{sign}{body}{}",
            fill(padding / 2),
            fill(padding - padding / 2)
        ),
        '=' => format!("{sign}{}{body}", fill(padding)),
        _ => format!("{}{sign}{body}", fill(padding)),
    }
}

/// Center digits vertically and anchor their right edge at the local origin.
/// This prevents baseline drift and keeps the unit fixed as the value width
/// changes.
pub fn right_align_readout_path(mut path: BezPath, _bounds: Bounds3D) -> (BezPath, Bounds3D) {
    let rect = path.bounding_box();
    path.apply_affine(Affine::translate((-rect.x1, -(rect.y0 + rect.y1) * 0.5)));
    let rect = path.bounding_box();
    (path, Bounds3D::new_2d(rect.x0, rect.y0, rect.x1, rect.y1))
}

/// Local typographic baseline after [`right_align_readout_path`] centers a
/// native text outline vertically.
pub fn right_aligned_readout_baseline(bounds: Bounds3D) -> f64 {
    -(bounds.min.y + bounds.max.y) * 0.5
}

/// Shape readout text with Typst, the resolver `scene.text` uses, into a Y-up
/// outline with its baseline at `y = 0` and its ink bounds.
///
/// The fixed prefix and suffix are shaped as whole runs; the number is laid
/// out from per-character glyphs, so a continuously changing value reuses a
/// bounded set of cached glyphs instead of compiling a new Typst document for
/// every displayed value.
pub fn shape_readout_text(
    registry: &gaanim_text::font::FontRegistry,
    prefix: &str,
    number: &str,
    suffix: &str,
    font_family: &str,
    font_size: f64,
) -> Result<(BezPath, Bounds3D), String> {
    shape_readout_text_with_weight(
        registry,
        prefix,
        number,
        suffix,
        font_family,
        None,
        font_size,
    )
}

/// [`shape_readout_text`] with an explicit font weight.
pub fn shape_readout_text_with_weight(
    registry: &gaanim_text::font::FontRegistry,
    prefix: &str,
    number: &str,
    suffix: &str,
    font_family: &str,
    weight: Option<u16>,
    font_size: f64,
) -> Result<(BezPath, Bounds3D), String> {
    let mut path = BezPath::new();
    let mut pen = 0.0;
    let mut place = |run: &str| -> Result<(), String> {
        if run.is_empty() {
            return Ok(());
        }
        let mut glyph = gaanim_text::typst_compiler::shape_typst_text_run(
            registry,
            run,
            font_family,
            weight,
            font_size,
        )
        .map_err(|errors| errors.join("; "))?;
        glyph.path.apply_affine(Affine::translate((pen, 0.0)));
        path.extend(glyph.path);
        pen += glyph.advance;
        Ok(())
    };
    place(prefix)?;
    let mut buffer = [0; 4];
    for ch in number.chars() {
        place(ch.encode_utf8(&mut buffer))?;
    }
    place(suffix)?;
    let rect = path.bounding_box();
    Ok((path, Bounds3D::new_2d(rect.x0, rect.y0, rect.x1, rect.y1)))
}

/// Restore the cached outline after snapshot replay and only recompile it when
/// the formatted text changes.
#[allow(clippy::type_complexity)]
pub fn reactive_readout_update_system(
    registry: Res<gaanim_text::font::FontRegistry>,
    playback: Option<Res<crate::updaters::PlaybackState>>,
    mut query: Query<(
        &mut ReactiveReadout,
        &mut Path2D,
        Option<&mut PathSource>,
        &mut LocalBounds,
        &mut TextBaseline,
        Option<&crate::writing::PathReveal>,
        Option<&mut crate::RollingNumber>,
        Option<&crate::RollingTweens>,
    )>,
    signals: Query<&FloatSignal>,
) {
    for (mut readout, mut path, path_source, mut bounds, mut baseline, reveal, rolling, tweens) in
        &mut query
    {
        let reveal = reveal
            .map(|progress| progress.0)
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);
        let time = playback.as_ref().map_or(0.0, |state| state.current_time);
        let value = readout
            .source
            .evaluate(time, |logical| {
                readout
                    .parameters
                    .iter()
                    .find_map(|(id, entity)| (*id == logical).then_some(*entity))
                    .and_then(|entity| signals.get(entity).ok())
                    .map(|signal| signal.value)
            })
            .unwrap_or(f64::NAN);
        if let Some(mut rolling) = rolling {
            let continuous = tweens.map_or(1.0, |tweens| tweens.continuous_weight(time));
            if rolling.last_value != Some((value, continuous)) {
                let (new_path, new_bounds) = rolling.blended_geometry(value, continuous);
                readout.last_path = Arc::new(new_path);
                readout.last_bounds = reserve_readout_bounds(new_bounds, readout.reserve);
                rolling.last_value = Some((value, continuous));
            }
            // Snapshot replay can restore the old path while retaining this
            // cache. Write only what differs: an unchanged counter must not
            // mark its geometry changed and make the renderer compare it.
            if let Some(mut source) = path_source
                && !Arc::ptr_eq(&source.0, &readout.last_path)
            {
                source.0 = readout.last_path.clone();
            }
            let revealed = crate::writing::path_at_reveal(&readout.last_path, reveal);
            if !Arc::ptr_eq(&path.0, &revealed) {
                path.0 = revealed;
            }
            bounds.set_if_neq(LocalBounds(readout.last_bounds));
            baseline.set_if_neq(TextBaseline(rolling.baseline()));
            continue;
        }
        let number = localize_decimal_separator(
            &format_reactive_number(value, &readout.format, &readout.invalid),
            readout.decimal_separator,
        );
        let text = format!("{}{}{}", readout.prefix, number, readout.suffix);
        if text == readout.last_text {
            let cached_geometry_is_current = path_source
                .as_ref()
                .map(|source| Arc::ptr_eq(&source.0, &readout.last_path))
                .unwrap_or_else(|| Arc::ptr_eq(&path.0, &readout.last_path));
            if cached_geometry_is_current {
                continue;
            }

            // Timeline seeks restore Path2D/PathSource from the t=0 snapshot,
            // while this component intentionally retains its last formatted
            // text. Restore the cached outline even when the formatted value
            // has not crossed another precision boundary.
            let cached_path = readout.last_path.clone();
            if let Some(mut source) = path_source {
                source.0 = cached_path.clone();
            }
            path.0 = crate::writing::path_at_reveal(&cached_path, reveal);
            bounds.0 = readout.last_bounds;
            continue;
        }
        if let Ok((new_path, new_bounds)) = shape_readout_text_with_weight(
            &registry,
            &readout.prefix,
            &number,
            &readout.suffix,
            &readout.font_family,
            readout.font_weight,
            readout.font_size,
        ) {
            baseline.0 = right_aligned_readout_baseline(new_bounds);
            let (new_path, new_bounds) = right_align_readout_path(new_path, new_bounds);
            let new_bounds = reserve_readout_bounds(new_bounds, readout.reserve);
            let new_path = std::sync::Arc::new(new_path);
            if let Some(mut source) = path_source {
                source.0 = new_path.clone();
            }
            path.0 = crate::writing::path_at_reveal(&new_path, reveal);
            bounds.0 = new_bounds;
            readout.last_text = text;
            readout.last_path = new_path;
            readout.last_bounds = new_bounds;
        }
    }
}

/// Align labels, numbers, and units on one optical text axis and preserve an
/// exact gap between terms.
///
/// The parts may come from different vector text engines (Typst math and the
/// native reactive-number shaper). Their visual bounds therefore do not carry
/// a shared typographic baseline. Centering those bounds produces a stable,
/// editorial row even when a symbolic label contains subscripts or accents.
pub fn reactive_readout_layout_system(
    layouts: Query<&ReactiveReadoutLayout>,
    ids: Query<(Entity, &MobjectId)>,
    bounds: Query<&LocalBounds>,
    baselines: Query<&TextBaseline>,
    mut transforms: Query<&mut SpatialTransform>,
) {
    if layouts.is_empty() {
        return;
    }
    // Map only the readout parts: a scene can hold tens of thousands of objects.
    let wanted = layouts
        .iter()
        .flat_map(|layout| {
            [
                layout.label,
                layout.equals,
                Some(layout.number),
                layout.unit,
            ]
        })
        .flatten()
        .collect::<std::collections::HashSet<_>>();
    let entities = ids
        .iter()
        .filter(|(_, id)| wanted.contains(&id.0))
        .map(|(entity, id)| (id.0, entity))
        .collect::<HashMap<_, _>>();

    for layout in &layouts {
        let ordered_ids = [
            layout.label,
            layout.equals,
            Some(layout.number),
            layout.unit,
        ];
        let parts = ordered_ids
            .into_iter()
            .flatten()
            .filter_map(|id| {
                let entity = *entities.get(&id)?;
                let local = bounds.get(entity).ok()?.0;
                let baseline = baselines
                    .get(entity)
                    .map(|baseline| baseline.0)
                    .unwrap_or((local.min.y + local.max.y) * 0.5);
                Some((id, entity, local, baseline))
            })
            .collect::<Vec<_>>();
        if parts.is_empty() {
            continue;
        }

        let rows = parts
            .iter()
            .map(|(_, _, local, baseline)| (*local, *baseline))
            .collect::<Vec<_>>();
        let translations = readout_row(&rows, layout.spacing, layout.align);
        for ((_, entity, _, _), translation) in parts.into_iter().zip(translations) {
            if let Ok(mut transform) = transforms.get_mut(entity) {
                transform.translation.x = translation.x;
                transform.translation.y = translation.y;
            }
        }
    }
}

/// Where a readout's parts go, in order: each part's local box and the
/// height of its baseline, laid out as one row `spacing` apart, its
/// baselines aligned and the row centered vertically on the origin and
/// placed on it horizontally by `align` (see [`ReactiveReadoutLayout`]).
pub fn readout_row(
    parts: &[(Bounds3D, f64)],
    spacing: f64,
    align: f64,
) -> Vec<gaanim_core::glam::DVec2> {
    let widths = parts
        .iter()
        .map(|(bounds, _)| (bounds.max.x - bounds.min.x).max(0.0))
        .collect::<Vec<_>>();
    let total_width =
        widths.iter().sum::<f64>() + spacing.max(0.0) * (parts.len().saturating_sub(1) as f64);
    let provisional_y = parts
        .iter()
        .map(|(_, baseline)| -*baseline)
        .collect::<Vec<_>>();
    let row_min_y = parts
        .iter()
        .zip(&provisional_y)
        .map(|((local, _), translation)| local.min.y + translation)
        .fold(f64::INFINITY, f64::min);
    let row_max_y = parts
        .iter()
        .zip(&provisional_y)
        .map(|((local, _), translation)| local.max.y + translation)
        .fold(f64::NEG_INFINITY, f64::max);
    let vertical_centering = -(row_min_y + row_max_y) * 0.5;
    let mut cursor = -total_width * 0.5 * (1.0 + align.clamp(-1.0, 1.0));
    let mut translations = Vec::with_capacity(parts.len());
    for (((local, _), width), translation_y) in parts.iter().zip(widths).zip(provisional_y) {
        let target_center_x = cursor + width * 0.5;
        let local_center_x = (local.min.x + local.max.x) * 0.5;
        translations.push(gaanim_core::glam::DVec2::new(
            target_center_x - local_center_x,
            translation_y + vertical_centering,
        ));
        cursor += width + spacing.max(0.0);
    }
    translations
}

/// Component defining a binding constraint between a source signal and a target entity.
///
/// When the source signal changes, the `apply` closure is executed to propagate
/// the new value into the target entity's components.
#[derive(Component)]
pub struct SignalBinding {
    /// The source entity holding the `Signal<T>` component.
    pub source: Entity,
    /// Closure that applies changes to the target entity using Bevy Commands.
    ///
    /// The closure receives the target entity and a `Commands` buffer.
    /// If reading from the `World` is required, the closure should use `SystemState`
    /// internally or capture necessary data at construction time.
    pub apply: ApplyFn,
}

/// Closure applying a signal value to its target entity.
pub type ApplyFn = Arc<dyn Fn(Entity, &mut Commands) + Send + Sync>;

impl SignalBinding {
    /// Creates a new signal binding between a source signal and a target.
    pub fn new(
        source: Entity,
        apply: impl Fn(Entity, &mut Commands) + Send + Sync + 'static,
    ) -> Self {
        Self {
            source,
            apply: Arc::new(apply),
        }
    }
}

/// Evaluates binding updates for a specific signal type `T` in parallel.
///
/// Utilizes Bevy's highly optimized `Changed` filter to skip unchanged signals.
pub fn signal_binding_system<T: Send + Sync + Clone + 'static>(
    mut commands: Commands,
    query_bindings: Query<(Entity, &SignalBinding)>,
    query_signals: Query<&Signal<T>, Changed<Signal<T>>>,
) {
    for (target_entity, binding) in &query_bindings {
        if query_signals.get(binding.source).is_ok() {
            // The source signal of type T changed during this frame! Re-run the binding.
            (binding.apply)(target_entity, &mut commands);
        }
    }
}

/// Strongly typed values supported by dynamic Mobject builders.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SpecValue {
    Float(f64),
    Vec3(gaanim_core::glam::DVec3),
    Color(Color),
    String(String),
    Bool(bool),
}

/// A serialized/FFI-friendly representation of a Mobject's creation parameters.
///
/// Enables Python scripting and UI layers to interact dynamically with Mobjects.
#[derive(Debug, Clone)]
pub struct MobjectSpec {
    /// The kind of Mobject (e.g. "circle", "rectangle", "typst").
    pub kind: String,
    /// The map of property names to their spec values.
    pub params: HashMap<String, SpecValue>,
}

/// Component instructing the engine to rebuild a Mobject's geometry
/// on any frame where its dependent signals are modified.
#[derive(Component)]
pub struct AlwaysRedraw {
    /// The list of entities containing the source signals that this Mobject depends on.
    pub signals: Vec<Entity>,
    /// Builder closure that reads the World state and returns the updated Mobject specifications.
    pub builder: Arc<dyn Fn(&World) -> MobjectSpec + Send + Sync>,
}

impl AlwaysRedraw {
    /// Creates a new AlwaysRedraw component with a list of signals and a builder function.
    pub fn new(
        signals: Vec<Entity>,
        builder: impl Fn(&World) -> MobjectSpec + Send + Sync + 'static,
    ) -> Self {
        Self {
            signals,
            builder: Arc::new(builder),
        }
    }
}

// ---------------------------------------------------------------------------
// PositionBinding — copy position axes from source entity to target each frame
// ---------------------------------------------------------------------------

/// Which axes to copy in a `PositionBinding`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AxisMask {
    pub x: bool,
    pub y: bool,
    pub z: bool,
}

impl AxisMask {
    pub const X: Self = Self {
        x: true,
        y: false,
        z: false,
    };
    pub const Y: Self = Self {
        x: false,
        y: true,
        z: false,
    };
    pub const Z: Self = Self {
        x: false,
        y: false,
        z: true,
    };
    pub const XY: Self = Self {
        x: true,
        y: true,
        z: false,
    };
    pub const XYZ: Self = Self {
        x: true,
        y: true,
        z: true,
    };

    pub fn contains(self, other: Self) -> bool {
        (!other.x || self.x) && (!other.y || self.y) && (!other.z || self.z)
    }
}

/// Component that copies specified position axes from a source entity each frame.
///
/// Runs in `SceneSet::Updaters` (after updater_system) so it sees the
/// source's updated position from updaters like orbit/bob.
#[derive(Component)]
pub struct PositionBinding {
    /// The entity whose position is read each frame.
    pub source: Entity,
    /// Which axes to copy (X, Y, Z, or any combination).
    pub axes: AxisMask,
    /// Offset applied after copying the selected source axes.
    pub offset: DVec3,
}

impl PositionBinding {
    pub fn new(source: Entity, axes: AxisMask) -> Self {
        Self {
            source,
            axes,
            offset: DVec3::ZERO,
        }
    }

    pub fn with_offset(source: Entity, axes: AxisMask, offset: DVec3) -> Self {
        Self {
            source,
            axes,
            offset,
        }
    }
}

/// System that applies `PositionBinding` — copies source position axes to target.
pub fn position_binding_system(world: &mut World) {
    apply_position_bindings(world, false);
}

/// Re-applies `PositionBinding` after `endpoint_follow_system`, so a binding
/// whose source is positioned with `follow(...)` copies the followed position
/// of the current frame. Targets that follow an endpoint themselves keep the
/// followed position.
pub fn followed_position_binding_system(world: &mut World) {
    apply_position_bindings(world, true);
}

fn apply_position_bindings(world: &mut World, skip_followers: bool) {
    let mut updates = Vec::new();

    // Collect all bindings and their source positions.
    let mut query = world.query::<(Entity, &PositionBinding)>();
    for (target, binding) in query.iter(world) {
        if skip_followers
            && world
                .get::<crate::updaters::EndpointFollow>(target)
                .is_some()
        {
            continue;
        }
        if let Some(src_transform) = world.get::<SpatialTransform>(binding.source) {
            updates.push((
                target,
                src_transform.translation,
                binding.axes,
                binding.offset,
            ));
        }
    }

    // Apply the axis copies.
    for (target, src_pos, axes, offset) in updates {
        if let Some(mut transform) = world.get_mut::<SpatialTransform>(target) {
            if axes.contains(AxisMask::X) {
                transform.translation.x = src_pos.x + offset.x;
            }
            if axes.contains(AxisMask::Y) {
                transform.translation.y = src_pos.y + offset.y;
            }
            if axes.contains(AxisMask::Z) {
                transform.translation.z = src_pos.z + offset.z;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// PointOnCurve — place a drawable at a normalized arc-length along a polyline
// ---------------------------------------------------------------------------

/// Keeps an entity positioned on a sampled curve using a normalized `FloatSignal`.
///
/// The curve is read as a `Path2D` and only its `MoveTo`/`LineTo` elements are
/// considered. This deliberately keeps the binding native and suitable for
/// polylines, function graphs, and parametric curves without calling Python per
/// frame.
#[derive(Component, Debug, Clone, Copy)]
pub struct PointOnCurve {
    /// Entity that owns the source `Path2D`.
    pub curve: Entity,
    /// Entity that owns the source `FloatSignal`.
    pub tracker: Entity,
}

impl PointOnCurve {
    pub fn new(curve: Entity, tracker: Entity) -> Self {
        Self { curve, tracker }
    }
}

/// Keeps a line centered on a sampled curve and aligned with its tangent.
#[derive(Component, Debug, Clone, Copy)]
pub struct TangentOnCurve {
    pub curve: Entity,
    pub tracker: Entity,
}

impl TangentOnCurve {
    pub fn new(curve: Entity, tracker: Entity) -> Self {
        Self { curve, tracker }
    }
}

/// Keeps a line centered on a sampled curve and aligned with its normal.
#[derive(Component, Debug, Clone, Copy)]
pub struct NormalOnCurve {
    pub curve: Entity,
    pub tracker: Entity,
}

impl NormalOnCurve {
    pub fn new(curve: Entity, tracker: Entity) -> Self {
        Self { curve, tracker }
    }
}

/// Keeps a unit circle scaled to the local osculating circle of a sampled curve.
#[derive(Component, Debug, Clone, Copy)]
pub struct CurvatureOnCurve {
    pub curve: Entity,
    pub tracker: Entity,
    pub window: f64,
}

impl CurvatureOnCurve {
    pub fn new(curve: Entity, tracker: Entity, window: f64) -> Self {
        Self {
            curve,
            tracker,
            window,
        }
    }
}

/// Updates `PointOnCurve` bindings after reactive curve regenerators.
pub fn point_on_curve_system(world: &mut World) {
    let mut updates = Vec::new();
    let mut query = world.query::<(Entity, &PointOnCurve)>();
    for (target, binding) in query.iter(world) {
        let Some(signal) = world.get::<FloatSignal>(binding.tracker) else {
            continue;
        };
        let Some(path) = world.get::<gaanim_scene::Path2D>(binding.curve) else {
            continue;
        };
        let Some(point) = point_at_polyline_fraction(path.0.as_ref(), signal.value) else {
            continue;
        };
        let z = world
            .get::<SpatialTransform>(target)
            .map(|transform| transform.translation.z)
            .unwrap_or(0.0);
        updates.push((target, DVec3::new(point.x, point.y, z)));
    }

    for (target, translation) in updates {
        if let Some(mut transform) = world.get_mut::<SpatialTransform>(target) {
            transform.translation = translation;
        }
    }
}

/// Updates tangent bindings after the curve and tracker have been updated.
pub fn tangent_on_curve_system(world: &mut World) {
    let mut updates = Vec::new();
    let mut query = world.query::<(Entity, &TangentOnCurve)>();
    for (target, binding) in query.iter(world) {
        let Some(signal) = world.get::<FloatSignal>(binding.tracker) else {
            continue;
        };
        let Some(path) = world.get::<gaanim_scene::Path2D>(binding.curve) else {
            continue;
        };
        let Some((point, tangent)) = sample_polyline(path.0.as_ref(), signal.value) else {
            continue;
        };
        let z = world
            .get::<SpatialTransform>(target)
            .map(|transform| transform.translation.z)
            .unwrap_or(0.0);
        updates.push((
            target,
            DVec3::new(point.x, point.y, z),
            gaanim_core::glam::DQuat::from_rotation_z(tangent.y.atan2(tangent.x)),
        ));
    }

    for (target, translation, rotation) in updates {
        if let Some(mut transform) = world.get_mut::<SpatialTransform>(target) {
            transform.translation = translation;
            transform.rotation = rotation;
        }
    }
}

/// Updates normal bindings after their source curve and tracker have changed.
pub fn normal_on_curve_system(world: &mut World) {
    let mut updates = Vec::new();
    let mut query = world.query::<(Entity, &NormalOnCurve)>();
    for (target, binding) in query.iter(world) {
        let Some(signal) = world.get::<FloatSignal>(binding.tracker) else {
            continue;
        };
        let Some(path) = world.get::<gaanim_scene::Path2D>(binding.curve) else {
            continue;
        };
        let Some((point, tangent)) = sample_polyline(path.0.as_ref(), signal.value) else {
            continue;
        };
        let z = world
            .get::<SpatialTransform>(target)
            .map(|transform| transform.translation.z)
            .unwrap_or(0.0);
        updates.push((
            target,
            DVec3::new(point.x, point.y, z),
            gaanim_core::glam::DQuat::from_rotation_z(
                tangent.y.atan2(tangent.x) + std::f64::consts::FRAC_PI_2,
            ),
        ));
    }

    for (target, translation, rotation) in updates {
        if let Some(mut transform) = world.get_mut::<SpatialTransform>(target) {
            transform.translation = translation;
            transform.rotation = rotation;
        }
    }
}

/// Updates osculating-circle bindings from three nearby arc-length samples.
pub fn curvature_on_curve_system(world: &mut World) {
    let mut updates = Vec::new();
    let mut query = world.query::<(Entity, &CurvatureOnCurve)>();
    for (target, binding) in query.iter(world) {
        let Some(signal) = world.get::<FloatSignal>(binding.tracker) else {
            continue;
        };
        let Some(path) = world.get::<gaanim_scene::Path2D>(binding.curve) else {
            continue;
        };
        let Some((center, radius)) =
            osculating_circle(path.0.as_ref(), signal.value, binding.window)
        else {
            continue;
        };
        let z = world
            .get::<SpatialTransform>(target)
            .map(|transform| transform.translation.z)
            .unwrap_or(0.0);
        updates.push((target, DVec3::new(center.x, center.y, z), radius));
    }
    for (target, translation, radius) in updates {
        if let Some(mut transform) = world.get_mut::<SpatialTransform>(target) {
            transform.translation = translation;
            transform.scale = DVec3::splat(radius);
        }
    }
}

/// Places every curve-bound marker on its current curve before position
/// bindings, followers and tracking lines read it. After a seek restores a
/// marker's initial transform, the late pass after curve regenerators would
/// otherwise run too late for the drawables that follow the marker.
pub fn curve_bindings_pre_pass_system(world: &mut World) {
    point_on_curve_system(world);
    tangent_on_curve_system(world);
    normal_on_curve_system(world);
    curvature_on_curve_system(world);
}

fn point_at_polyline_fraction(path: &BezPath, fraction: f64) -> Option<Point> {
    sample_polyline(path, fraction).map(|(point, _)| point)
}

fn sample_polyline(path: &BezPath, fraction: f64) -> Option<(Point, gaanim_core::kurbo::Vec2)> {
    let mut segments = Vec::new();
    let mut current = None;
    let mut subpath_start = None;
    for element in path.elements() {
        match *element {
            PathEl::MoveTo(point) => {
                current = Some(point);
                subpath_start = Some(point);
            }
            PathEl::LineTo(point) => {
                if let Some(start) = current {
                    push_line_segment(&mut segments, start, point);
                }
                current = Some(point);
            }
            PathEl::QuadTo(control, point) => {
                if let Some(start) = current {
                    let mut previous = start;
                    for index in 1..=24 {
                        let t = index as f64 / 24.0;
                        let inverse = 1.0 - t;
                        let next = Point::new(
                            inverse * inverse * start.x
                                + 2.0 * inverse * t * control.x
                                + t * t * point.x,
                            inverse * inverse * start.y
                                + 2.0 * inverse * t * control.y
                                + t * t * point.y,
                        );
                        push_line_segment(&mut segments, previous, next);
                        previous = next;
                    }
                }
                current = Some(point);
            }
            PathEl::CurveTo(control1, control2, point) => {
                if let Some(start) = current {
                    let mut previous = start;
                    for index in 1..=32 {
                        let t = index as f64 / 32.0;
                        let inverse = 1.0 - t;
                        let next = Point::new(
                            inverse.powi(3) * start.x
                                + 3.0 * inverse * inverse * t * control1.x
                                + 3.0 * inverse * t * t * control2.x
                                + t.powi(3) * point.x,
                            inverse.powi(3) * start.y
                                + 3.0 * inverse * inverse * t * control1.y
                                + 3.0 * inverse * t * t * control2.y
                                + t.powi(3) * point.y,
                        );
                        push_line_segment(&mut segments, previous, next);
                        previous = next;
                    }
                }
                current = Some(point);
            }
            PathEl::ClosePath => {
                if let (Some(start), Some(end)) = (current, subpath_start) {
                    push_line_segment(&mut segments, start, end);
                    current = Some(end);
                }
            }
        }
    }

    let total_length: f64 = segments.iter().map(|(_, _, length)| length).sum();
    if total_length <= f64::EPSILON {
        return None;
    }
    let distance = fraction.clamp(0.0, 1.0) * total_length;
    let mut traversed = 0.0;
    for (start, end, length) in &segments {
        if distance <= traversed + length {
            return Some((
                start.lerp(*end, (distance - traversed) / length),
                *end - *start,
            ));
        }
        traversed += length;
    }
    segments.last().map(|(start, end, _)| (*end, *end - *start))
}

fn push_line_segment(segments: &mut Vec<(Point, Point, f64)>, start: Point, end: Point) {
    let length = (end - start).hypot();
    if length > f64::EPSILON {
        segments.push((start, end, length));
    }
}

fn osculating_circle(path: &BezPath, fraction: f64, window: f64) -> Option<(Point, f64)> {
    let fraction = fraction.clamp(0.0, 1.0);
    let window = window.clamp(1e-4, 0.5);
    let left = (fraction - window).max(0.0);
    let right = (fraction + window).min(1.0);
    if fraction - left <= f64::EPSILON || right - fraction <= f64::EPSILON {
        return None;
    }
    let a = point_at_polyline_fraction(path, left)?;
    let b = point_at_polyline_fraction(path, fraction)?;
    let c = point_at_polyline_fraction(path, right)?;
    let d = 2.0 * (a.x * (b.y - c.y) + b.x * (c.y - a.y) + c.x * (a.y - b.y));
    if d.abs() <= 1e-9 {
        return None;
    }
    let a2 = a.x * a.x + a.y * a.y;
    let b2 = b.x * b.x + b.y * b.y;
    let c2 = c.x * c.x + c.y * c.y;
    let center = Point::new(
        (a2 * (b.y - c.y) + b2 * (c.y - a.y) + c2 * (a.y - b.y)) / d,
        (a2 * (c.x - b.x) + b2 * (a.x - c.x) + c2 * (b.x - a.x)) / d,
    );
    let radius = (a - center).hypot();
    (radius.is_finite() && radius > f64::EPSILON).then_some((center, radius))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::{App, Update};
    use std::sync::Arc;

    #[test]
    fn reactive_readout_keeps_create_hidden_when_its_value_changes() {
        let parameter_id = gaanim_core::ObjectId::from_raw(11);
        let mut app = App::new();
        app.insert_resource(gaanim_text::font::FontRegistry::new());
        app.add_systems(Update, reactive_readout_update_system);

        let signal = app.world_mut().spawn(FloatSignal::new(44.2)).id();
        let empty = Arc::new(BezPath::new());
        let readout = app
            .world_mut()
            .spawn((
                ReactiveReadout {
                    source: crate::reactive::ScalarSource::signal(parameter_id),
                    parameters: vec![(parameter_id, signal)],
                    format: ".1f".to_owned(),
                    prefix: String::new(),
                    suffix: String::new(),
                    invalid: "—".to_owned(),
                    decimal_separator: '.',
                    font_family: "sans-serif".to_owned(),
                    font_weight: None,
                    font_size: 40.0,
                    last_text: "—".to_owned(),
                    last_path: empty.clone(),
                    last_bounds: Bounds3D::default(),
                    reserve: 0.0,
                },
                Path2D(empty.clone()),
                PathSource(empty),
                LocalBounds(Bounds3D::default()),
                TextBaseline::default(),
                crate::writing::PathReveal(0.0),
            ))
            .id();

        app.update();

        assert!(
            !app.world().get::<PathSource>(readout).unwrap().0.is_empty(),
            "the full numeric outline must remain available to the Create animation"
        );
        assert!(
            app.world().get::<Path2D>(readout).unwrap().0.is_empty(),
            "a reactive value update must not reveal the readout before Create starts"
        );
    }

    #[test]
    fn reactive_readout_replaces_both_render_paths_when_signal_changes() {
        let parameter_id = gaanim_core::ObjectId::from_raw(7);
        let mut app = App::new();
        app.insert_resource(gaanim_text::font::FontRegistry::new());
        app.add_systems(Update, reactive_readout_update_system);

        let signal = app.world_mut().spawn(FloatSignal::new(2.5)).id();
        let initial_path = Arc::new(BezPath::new());
        let readout = app
            .world_mut()
            .spawn((
                ReactiveReadout {
                    source: crate::reactive::ScalarSource::signal(parameter_id),
                    parameters: vec![(parameter_id, signal)],
                    format: ".1f".to_owned(),
                    prefix: String::new(),
                    suffix: String::new(),
                    invalid: "—".to_owned(),
                    decimal_separator: '.',
                    font_family: "sans-serif".to_owned(),
                    font_weight: None,
                    font_size: 40.0,
                    last_text: "1.0".to_owned(),
                    last_path: initial_path.clone(),
                    last_bounds: Bounds3D::default(),
                    reserve: 0.0,
                },
                Path2D(initial_path.clone()),
                PathSource(initial_path),
                LocalBounds(Bounds3D::default()),
                TextBaseline::default(),
            ))
            .id();

        app.update();

        let component = app.world().get::<ReactiveReadout>(readout).unwrap();
        let path = app.world().get::<Path2D>(readout).unwrap();
        let source = app.world().get::<PathSource>(readout).unwrap();
        assert_eq!(component.last_text, "2.5");
        assert!(!path.0.elements().is_empty());
        assert!(Arc::ptr_eq(&path.0, &source.0));

        let expected_path = path.0.clone();
        let expected_bounds = app.world().get::<LocalBounds>(readout).unwrap().0;
        let restored_initial = Arc::new(BezPath::new());
        app.world_mut().entity_mut(readout).insert((
            Path2D(restored_initial.clone()),
            PathSource(restored_initial),
            LocalBounds(Bounds3D::default()),
        ));

        // Editor playback seeks from its t=0 snapshot every frame. The
        // formatted value may remain unchanged for several frames, but the
        // snapshot-restored geometry must not remain at the initial value.
        app.update();

        let path = app.world().get::<Path2D>(readout).unwrap();
        let source = app.world().get::<PathSource>(readout).unwrap();
        assert!(Arc::ptr_eq(&path.0, &expected_path));
        assert!(Arc::ptr_eq(&source.0, &expected_path));
        assert_eq!(
            app.world().get::<LocalBounds>(readout).unwrap().0,
            expected_bounds
        );
    }

    fn assert_same_outline(actual: &BezPath, expected: &BezPath) {
        let points = |path: &BezPath| {
            path.elements()
                .iter()
                .flat_map(|element| match *element {
                    PathEl::MoveTo(p) | PathEl::LineTo(p) => vec![p],
                    PathEl::QuadTo(a, b) => vec![a, b],
                    PathEl::CurveTo(a, b, c) => vec![a, b, c],
                    PathEl::ClosePath => vec![],
                })
                .collect::<Vec<_>>()
        };
        let kinds = |path: &BezPath| {
            path.elements()
                .iter()
                .map(std::mem::discriminant)
                .collect::<Vec<_>>()
        };
        assert_eq!(kinds(actual), kinds(expected));
        for (actual, expected) in points(actual).into_iter().zip(points(expected)) {
            assert!(
                actual.distance(expected) < 1e-9,
                "{actual:?} != {expected:?}"
            );
        }
    }

    /// Typst's own outline for `text`, placed like a readout.
    fn typst_readout(
        registry: &gaanim_text::font::FontRegistry,
        text: &str,
        family: &str,
    ) -> (BezPath, f64) {
        let path =
            gaanim_text::typst_compiler::shape_typst_text_run(registry, text, family, None, 0.75)
                .unwrap()
                .path;
        let rect = path.bounding_box();
        let (path, _) = right_align_readout_path(path, Bounds3D::default());
        (path, -(rect.y0 + rect.y1) * 0.5)
    }

    #[test]
    fn reactive_readout_resolves_family_like_scene_text() {
        let parameter_id = gaanim_core::ObjectId::from_raw(5);
        let mut app = App::new();
        app.insert_resource(gaanim_text::font::FontRegistry::new());
        app.add_systems(Update, reactive_readout_update_system);
        let signal = app.world_mut().spawn(FloatSignal::new(2.5)).id();
        let empty = Arc::new(BezPath::new());
        let readout = app
            .world_mut()
            .spawn((
                ReactiveReadout {
                    source: crate::reactive::ScalarSource::signal(parameter_id),
                    parameters: vec![(parameter_id, signal)],
                    format: ".1f".to_owned(),
                    prefix: String::new(),
                    suffix: String::new(),
                    invalid: "—".to_owned(),
                    decimal_separator: '.',
                    font_family: "Libertinus Serif".to_owned(),
                    font_weight: None,
                    font_size: 0.75,
                    last_text: String::new(),
                    last_path: empty.clone(),
                    last_bounds: Bounds3D::default(),
                    reserve: 0.0,
                },
                Path2D(empty.clone()),
                PathSource(empty),
                LocalBounds(Bounds3D::default()),
                TextBaseline::default(),
            ))
            .id();

        app.update();
        let registry = app.world().resource::<gaanim_text::font::FontRegistry>();

        // Libertinus is known to Typst but not by name to the legacy registry,
        // which silently fell back to a system sans face.
        let (expected, baseline) = typst_readout(registry, "2.5", "Libertinus Serif");
        assert_same_outline(
            &app.world().get::<PathSource>(readout).unwrap().0,
            &expected,
        );
        assert!((app.world().get::<TextBaseline>(readout).unwrap().0 - baseline).abs() < 1e-9);
        if let Ok((legacy, bounds)) =
            gaanim_text::shaper::compile_text_to_path(registry, "2.5", "Libertinus Serif", 0.75)
        {
            let (legacy, _) = right_align_readout_path(legacy, bounds);
            assert_ne!(legacy.bounding_box(), expected.bounding_box());
        }

        // Fixed prefix/suffix runs and padded numbers keep Typst's advances.
        let (path, bounds) =
            shape_readout_text(registry, "x = ", "  2.5", " m", "Libertinus Serif", 0.75).unwrap();
        let (path, _) = right_align_readout_path(path, bounds);
        assert_same_outline(
            &path,
            &typst_readout(registry, "x =   2.5 m", "Libertinus Serif").0,
        );
    }

    #[test]
    fn reactive_readout_layout_preserves_equal_gaps_when_number_width_changes() {
        let label_id = gaanim_core::ObjectId::from_raw(11);
        let equals_id = gaanim_core::ObjectId::from_raw(12);
        let number_id = gaanim_core::ObjectId::from_raw(13);
        let unit_id = gaanim_core::ObjectId::from_raw(14);
        let mut app = App::new();
        app.add_systems(Update, reactive_readout_layout_system);

        let spawn_part = |app: &mut App, id, width: f64, min_y: f64, max_y: f64, baseline: f64| {
            app.world_mut()
                .spawn((
                    MobjectId(id),
                    LocalBounds(Bounds3D::new_2d(0.0, min_y, width, max_y)),
                    TextBaseline(baseline),
                    SpatialTransform::default(),
                ))
                .id()
        };
        let label = spawn_part(&mut app, label_id, 20.0, -9.0, 19.0, -1.0);
        let equals = spawn_part(&mut app, equals_id, 12.0, -5.0, 7.0, 1.0);
        let number = spawn_part(&mut app, number_id, 40.0, -12.0, 12.0, -3.0);
        let unit = spawn_part(&mut app, unit_id, 18.0, -7.0, 23.0, 2.0);
        app.world_mut().spawn(ReactiveReadoutLayout {
            label: Some(label_id),
            equals: Some(equals_id),
            number: number_id,
            unit: Some(unit_id),
            spacing: 10.0,
            align: 0.0,
        });

        app.update();

        let visual_edges = |app: &App, entity| {
            let bounds = app.world().get::<LocalBounds>(entity).unwrap().0;
            let transform = app.world().get::<SpatialTransform>(entity).unwrap();
            (
                bounds.min.x + transform.translation.x,
                bounds.max.x + transform.translation.x,
                bounds.min.y + transform.translation.y,
                bounds.max.y + transform.translation.y,
            )
        };
        let label_edges = visual_edges(&app, label);
        let equals_edges = visual_edges(&app, equals);
        let initial_number_edges = visual_edges(&app, number);
        let initial_unit_edges = visual_edges(&app, unit);
        assert_eq!(equals_edges.0 - label_edges.1, 10.0);
        assert_eq!(initial_number_edges.0 - equals_edges.1, 10.0);
        assert_eq!(initial_unit_edges.0 - initial_number_edges.1, 10.0);
        let world_baseline = |app: &App, entity| {
            app.world().get::<TextBaseline>(entity).unwrap().0
                + app
                    .world()
                    .get::<SpatialTransform>(entity)
                    .unwrap()
                    .translation
                    .y
        };
        let expected_baseline = world_baseline(&app, number);
        assert_eq!(world_baseline(&app, label), expected_baseline);
        assert_eq!(world_baseline(&app, equals), expected_baseline);
        assert_eq!(world_baseline(&app, unit), expected_baseline);

        app.world_mut()
            .entity_mut(number)
            .insert(LocalBounds(Bounds3D::new_2d(0.0, 0.0, 70.0, 24.0)));
        app.update();

        let label_edges = visual_edges(&app, label);
        let equals_edges = visual_edges(&app, equals);
        let number_edges = visual_edges(&app, number);
        let unit_edges = visual_edges(&app, unit);
        assert_eq!(equals_edges.0 - label_edges.1, 10.0);
        assert_eq!(number_edges.0 - equals_edges.1, 10.0);
        assert_eq!(unit_edges.0 - number_edges.1, 10.0);
        assert_eq!(label_edges.0, -75.0);
        assert_eq!(unit_edges.1, 75.0);
    }

    #[test]
    fn reactive_readout_layout_aligns_a_changing_number_on_its_start() {
        let number_id = gaanim_core::ObjectId::from_raw(21);
        let mut app = App::new();
        app.add_systems(Update, reactive_readout_layout_system);
        // Rolling numbers are right anchored: their outline ends at x = 0.
        let number = app
            .world_mut()
            .spawn((
                MobjectId(number_id),
                LocalBounds(Bounds3D::new_2d(-30.0, -5.0, 0.0, 5.0)),
                SpatialTransform::default(),
            ))
            .id();
        app.world_mut().spawn(ReactiveReadoutLayout {
            label: None,
            equals: None,
            number: number_id,
            unit: None,
            spacing: 0.0,
            align: -1.0,
        });
        let start = |app: &App| {
            app.world().get::<LocalBounds>(number).unwrap().0.min.x
                + app
                    .world()
                    .get::<SpatialTransform>(number)
                    .unwrap()
                    .translation
                    .x
        };
        app.update();
        assert_eq!(start(&app), 0.0);
        app.world_mut()
            .entity_mut(number)
            .insert(LocalBounds(Bounds3D::new_2d(-80.0, -5.0, 0.0, 5.0)));
        app.update();
        assert_eq!(start(&app), 0.0);
    }

    #[test]
    fn point_on_curve_uses_normalized_arc_length_and_clamps_tracker() {
        let mut path = BezPath::new();
        path.move_to(Point::new(0.0, 0.0));
        path.line_to(Point::new(100.0, 0.0));
        path.line_to(Point::new(100.0, 300.0));

        let mut world = World::new();
        let curve = world.spawn(gaanim_scene::Path2D(Arc::new(path))).id();
        let tracker = world.spawn(FloatSignal::new(0.5)).id();
        let target = world
            .spawn((
                SpatialTransform::default(),
                PointOnCurve::new(curve, tracker),
            ))
            .id();

        point_on_curve_system(&mut world);
        assert_eq!(
            world.get::<SpatialTransform>(target).unwrap().translation,
            DVec3::new(100.0, 100.0, 0.0),
        );

        world.get_mut::<FloatSignal>(tracker).unwrap().value = 0.5;
        let tangent = world
            .spawn((
                SpatialTransform::default(),
                TangentOnCurve::new(curve, tracker),
            ))
            .id();
        tangent_on_curve_system(&mut world);
        let transform = world.get::<SpatialTransform>(tangent).unwrap();
        assert_eq!(transform.translation, DVec3::new(100.0, 100.0, 0.0));
        let direction = transform.rotation * DVec3::X;
        assert!(direction.x.abs() < 1e-9 && (direction.y - 1.0).abs() < 1e-9);

        let normal = world
            .spawn((
                SpatialTransform::default(),
                NormalOnCurve::new(curve, tracker),
            ))
            .id();
        normal_on_curve_system(&mut world);
        let direction = world.get::<SpatialTransform>(normal).unwrap().rotation * DVec3::X;
        assert!((direction.x + 1.0).abs() < 1e-9 && direction.y.abs() < 1e-9);

        let mut cubic = BezPath::new();
        cubic.move_to(Point::new(0.0, 0.0));
        cubic.curve_to(
            Point::new(0.0, 120.0),
            Point::new(120.0, 120.0),
            Point::new(120.0, 0.0),
        );
        assert_eq!(
            point_at_polyline_fraction(&cubic, 0.0),
            Some(Point::new(0.0, 0.0))
        );
        let end = point_at_polyline_fraction(&cubic, 1.0).expect("cubic endpoint");
        assert!((end.x - 120.0).abs() < 1e-9 && end.y.abs() < 1e-9);

        world.get_mut::<FloatSignal>(tracker).unwrap().value = 2.0;
        point_on_curve_system(&mut world);
        assert_eq!(
            world.get::<SpatialTransform>(target).unwrap().translation,
            DVec3::new(100.0, 300.0, 0.0),
        );
    }
}

// ---------------------------------------------------------------------------
// AlwaysRedrawRegen — rebuild Path2D each frame via a closure
// ---------------------------------------------------------------------------

/// Component that regenerates an entity's `Path2D` every frame by calling a closure
/// with `&mut World` access. Unlike `AlwaysRedraw` (which returns a spec),
/// this directly writes the new path.
#[derive(Component)]
pub struct AlwaysRedrawRegen {
    /// Closure that reads world state and returns a new BezPath.
    pub regen: Arc<dyn Fn(&World) -> gaanim_core::kurbo::BezPath + Send + Sync>,
}

/// Component that deterministically regenerates a 3D line list from the world snapshot.
#[derive(Component)]
pub struct ReactiveLineRegen {
    pub regen: Arc<dyn Fn(&World) -> gaanim_scene::LineListData + Send + Sync>,
}

impl ReactiveLineRegen {
    pub fn new(
        regen: impl Fn(&World) -> gaanim_scene::LineListData + Send + Sync + 'static,
    ) -> Self {
        Self {
            regen: Arc::new(regen),
        }
    }
}

/// Component that deterministically regenerates a 3D triangle mesh from the world snapshot.
#[derive(Component)]
pub struct ReactiveMeshRegen {
    pub regen: Arc<dyn Fn(&World) -> gaanim_scene::TriangleMeshData + Send + Sync>,
}

impl ReactiveMeshRegen {
    pub fn new(
        regen: impl Fn(&World) -> gaanim_scene::TriangleMeshData + Send + Sync + 'static,
    ) -> Self {
        Self {
            regen: Arc::new(regen),
        }
    }
}

impl AlwaysRedrawRegen {
    pub fn new(
        regen: impl Fn(&World) -> gaanim_core::kurbo::BezPath + Send + Sync + 'static,
    ) -> Self {
        Self {
            regen: Arc::new(regen),
        }
    }
}

/// Exclusive system that executes `AlwaysRedrawRegen` closures and updates Path2D.
pub fn always_redraw_regen_system(world: &mut World) {
    let mut updates = Vec::new();

    let mut query = world.query::<(Entity, &AlwaysRedrawRegen)>();
    for (entity, regen) in query.iter(world) {
        let path = (regen.regen)(world);
        let bounds = if path.elements().is_empty() {
            Bounds3D::default()
        } else {
            let rect = gaanim_core::kurbo::Shape::bounding_box(&path);
            Bounds3D::new_2d(
                rect.x0 - 12.0,
                rect.y0 - 12.0,
                rect.x1 + 12.0,
                rect.y1 + 12.0,
            )
        };
        // Respect any active PathReveal (draw) progress. Without this,
        // a reactive ExpressionPlot would overwrite the trimmed Path2D
        // produced by its Write/Create animation and appear fully from
        // frame 0.
        let reveal = world
            .get::<crate::writing::PathReveal>(entity)
            .map(|r| r.0)
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);
        let trim = world.get::<crate::writing::PathTrimWindow>(entity).copied();
        updates.push((entity, path, bounds, reveal, trim));
    }

    for (entity, path, bounds, reveal, trim) in updates {
        let path = Arc::new(path);
        let visible = crate::writing::visible_path(&path, reveal, trim.as_ref());
        if let Some(mut path_comp) = world.get_mut::<gaanim_scene::Path2D>(entity) {
            path_comp.0 = visible;
        }
        if let Some(mut path_source) = world.get_mut::<gaanim_scene::PathSource>(entity) {
            path_source.0 = path;
        }
        if let Some(mut local_bounds) = world.get_mut::<gaanim_scene::LocalBounds>(entity) {
            local_bounds.0 = bounds;
        }
        // Keep the PathReveal component alive so snapshot restore
        // can see the correct reveal factor.
        if world.get::<crate::writing::PathReveal>(entity).is_none()
            && (reveal - 1.0).abs() > 1e-9
            && let Ok(mut em) = world.get_entity_mut(entity)
        {
            em.insert(crate::writing::PathReveal(reveal));
        }
    }
}

/// Regenerate reactive 3D raw geometry before renderer extraction.
pub fn reactive_3d_regen_system(world: &mut World) {
    let line_updates = {
        let mut query = world.query::<(Entity, &ReactiveLineRegen)>();
        query
            .iter(world)
            .map(|(entity, regen)| (entity, (regen.regen)(world)))
            .collect::<Vec<_>>()
    };
    let mesh_updates = {
        let mut query = world.query::<(Entity, &ReactiveMeshRegen)>();
        query
            .iter(world)
            .map(|(entity, regen)| (entity, (regen.regen)(world)))
            .collect::<Vec<_>>()
    };
    for (entity, data) in line_updates {
        if let Some(mut current) = world.get_mut::<gaanim_scene::LineListData>(entity) {
            *current = data;
        }
    }
    for (entity, data) in mesh_updates {
        if let Some(mut current) = world.get_mut::<gaanim_scene::TriangleMeshData>(entity) {
            *current = data;
        }
    }
}
