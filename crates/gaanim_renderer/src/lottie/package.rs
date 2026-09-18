//! In-memory dotLottie resources. No archive path is ever opened on the host.
use super::{LottieAsset, LottieError};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

mod machine;
pub use machine::LottieInput;
use machine::Machine;

type VariantKey = (String, Option<String>);
type PackageCache = Mutex<HashMap<PathBuf, Arc<Package>>>;
static PACKAGES: OnceLock<PackageCache> = OnceLock::new();

/// Optional selectors for a dotLottie archive. Plain JSON accepts no selectors.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LottiePackageOptions {
    pub animation_id: Option<String>,
    pub theme_id: Option<String>,
    pub state_machine_id: Option<String>,
}

pub(super) fn invalid(message: impl Into<String>) -> LottieError {
    LottieError::Package(message.into())
}

pub(super) fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str, LottieError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| invalid(format!("missing or invalid '{key}'")))
}

pub(super) fn array<'a>(value: &'a Value, key: &str) -> Result<&'a [Value], LottieError> {
    match value.get(key) {
        None => Ok(&[]),
        Some(Value::Array(items)) => Ok(items),
        _ => Err(invalid(format!("'{key}' must be an array"))),
    }
}

fn safe_path(path: &str) -> Result<String, LottieError> {
    if path.is_empty() || path.contains(['\\', ':', '\0']) || path.starts_with('/') {
        return Err(invalid(format!("invalid archive path '{path}'")));
    }
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(invalid("archive path escapes package"));
                }
            }
            part => parts.push(part),
        }
    }
    Ok(parts.join("/"))
}

#[derive(Debug)]
pub struct Package {
    path: PathBuf,
    entries: HashMap<String, Vec<u8>>,
    manifest: Value,
    animation_dir: &'static str,
    pub animation_ids: Vec<String>,
    pub theme_ids: Vec<String>,
    pub state_machine_ids: Vec<String>,
    variants: Mutex<HashMap<VariantKey, Arc<LottieAsset>>>,
}

impl Package {
    pub fn load(path: &Path) -> Result<Arc<Self>, LottieError> {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let cache = PACKAGES.get_or_init(Default::default);
        if let Some(package) = cache
            .lock()
            .expect("package cache poisoned")
            .get(&path)
            .cloned()
        {
            return Ok(package);
        }
        let file = std::fs::File::open(&path).map_err(|source| LottieError::Read {
            path: path.clone(),
            source,
        })?;
        let mut archive =
            zip::ZipArchive::new(file).map_err(|e| invalid(format!("{}: {e}", path.display())))?;
        let mut entries = HashMap::new();
        let mut total = 0_u64;
        if archive.len() > 4096 {
            return Err(invalid("package exceeds 4096 entries"));
        }
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).map_err(|e| invalid(e.to_string()))?;
            if entry.is_dir() {
                continue;
            }
            let name = safe_path(entry.name())?;
            total = total.saturating_add(entry.size());
            if total > 256 * 1024 * 1024 {
                return Err(invalid("package exceeds 256 MiB uncompressed"));
            }
            let mut bytes = Vec::new();
            (&mut entry)
                .take(256 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| invalid(e.to_string()))?;
            if bytes.len() as u64 != entry.size() {
                return Err(invalid("invalid archive entry size"));
            }
            if entries.insert(name, bytes).is_some() {
                return Err(invalid("duplicate archive entry"));
            }
        }
        let manifest: Value = serde_json::from_slice(
            entries
                .get("manifest.json")
                .ok_or_else(|| invalid("missing manifest.json"))?,
        )
        .map_err(|e| invalid(format!("invalid manifest.json: {e}")))?;
        let animation_dir = match string(&manifest, "version")? {
            "1" | "1.0" => "animations",
            "2" | "2.0" => "a",
            version => {
                return Err(invalid(format!(
                    "unsupported dotLottie version '{version}'"
                )));
            }
        };
        let ids = |key: &str| -> Result<Vec<String>, LottieError> {
            let mut seen = HashSet::new();
            array(&manifest, key)?
                .iter()
                .map(|item| {
                    let id = string(item, "id")?;
                    if id.contains(['/', '\\', ':']) || matches!(id, "." | "..") || !seen.insert(id)
                    {
                        return Err(invalid(format!("invalid or duplicate {key} id '{id}'")));
                    }
                    Ok(id.to_owned())
                })
                .collect()
        };
        let animation_ids = ids("animations")?;
        if animation_ids.is_empty() {
            return Err(invalid("manifest must contain animations"));
        }
        let package = Arc::new(Self {
            theme_ids: ids("themes")?,
            state_machine_ids: ids("stateMachines")?,
            animation_ids,
            path: path.clone(),
            entries,
            manifest,
            animation_dir,
            variants: Mutex::new(HashMap::new()),
        });
        cache
            .lock()
            .expect("package cache poisoned")
            .insert(path, package.clone());
        Ok(package)
    }

    fn json(&self, name: &str) -> Result<Value, LottieError> {
        serde_json::from_slice(
            self.entries
                .get(name)
                .ok_or_else(|| invalid(format!("{}: missing '{name}'", self.path.display())))?,
        )
        .map_err(|e| invalid(format!("{name}: {e}")))
    }

    fn animation_metadata(&self, id: &str) -> &Value {
        self.manifest["animations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == id)
            .unwrap_or(&Value::Null)
    }

    fn animation_json(&self, id: &str) -> Result<Value, LottieError> {
        if !self.animation_ids.iter().any(|v| v == id) {
            return Err(invalid(format!("unknown animation '{id}'")));
        }
        self.json(&format!("{}/{id}.json", self.animation_dir))
    }

    pub(super) fn asset(
        &self,
        id: &str,
        theme: Option<&str>,
    ) -> Result<Arc<LottieAsset>, LottieError> {
        let key = (id.to_owned(), theme.map(str::to_owned));
        if let Some(asset) = self
            .variants
            .lock()
            .expect("variant cache poisoned")
            .get(&key)
            .cloned()
        {
            return Ok(asset);
        }
        let mut json = self.animation_json(id)?;
        if let Some(theme) = theme {
            if !self.theme_ids.iter().any(|v| v == theme) {
                return Err(invalid(format!("unknown theme '{theme}'")));
            }
            let allowed = self.animation_metadata(id).get("themes");
            if let Some(allowed) = allowed {
                if !allowed
                    .as_array()
                    .is_some_and(|a| a.iter().any(|v| v == theme))
                {
                    return Err(invalid(format!(
                        "theme '{theme}' is not compatible with animation '{id}'"
                    )));
                }
            }
            apply_theme(&mut json, &self.json(&format!("t/{theme}.json"))?, id)?;
        }
        let asset = LottieAsset::from_json(self.path.clone(), json, Some(self))?;
        self.variants
            .lock()
            .expect("variant cache poisoned")
            .insert(key, asset.clone());
        Ok(asset)
    }

    pub(super) fn image(
        &self,
        directory: Option<&str>,
        file: &str,
    ) -> Result<Vec<u8>, LottieError> {
        if file.starts_with("data:") {
            use base64::Engine;
            let (header, data) = file
                .split_once(',')
                .ok_or_else(|| invalid("invalid image data URI"))?;
            if !header.ends_with(";base64") {
                return Err(invalid("image data URI must use base64"));
            }
            return base64::engine::general_purpose::STANDARD
                .decode(data)
                .map_err(|e| invalid(e.to_string()));
        }
        // Relative references are relative to the animation document; some exporters
        // instead store a package-root path. Both are confined to the ZIP namespace.
        let reference = format!("{}{file}", directory.unwrap_or_default());
        let relative = safe_path(&format!("{}/{reference}", self.animation_dir))?;
        if let Some(bytes) = self.entries.get(&relative) {
            return Ok(bytes.clone());
        }
        let root = safe_path(&reference)?;
        self.entries
            .get(&root)
            .or_else(|| {
                // dotLottie exporters can omit the image directory in u/p.
                let image_dir = if self.animation_dir == "a" {
                    "i"
                } else {
                    "images"
                };
                (!root.contains('/'))
                    .then(|| self.entries.get(&format!("{image_dir}/{root}")))
                    .flatten()
            })
            .cloned()
            .ok_or_else(|| invalid(format!("missing packaged image '{reference}'")))
    }
}

pub(super) fn clear_cache() {
    if let Some(cache) = PACKAGES.get() {
        cache.lock().expect("package cache poisoned").clear();
    }
}

fn apply_theme(json: &mut Value, theme: &Value, animation: &str) -> Result<(), LottieError> {
    if theme.get("rules").is_none() {
        return Err(invalid("theme has no rules"));
    }
    for rule in array(theme, "rules")? {
        if rule.get("keyframes").is_some() || rule.get("expression").is_some() {
            return Err(invalid("animated themes and expressions are unsupported"));
        }
        let id = string(rule, "id")?;
        let kind = string(rule, "type")?;
        if !matches!(
            kind,
            "Color" | "Scalar" | "Position" | "Vector" | "Gradient"
        ) {
            return Err(invalid(format!("unsupported theme rule '{kind}'")));
        }
        if rule.get("animations").is_some()
            && !array(rule, "animations")?.iter().any(|v| v == animation)
        {
            continue;
        }
        let value = rule
            .get("value")
            .ok_or_else(|| invalid("theme rule requires value"))?;
        let (value, points) = theme_value(kind, value)?;
        replace_slots(json, id, &value, points);
    }
    Ok(())
}

fn theme_value(kind: &str, value: &Value) -> Result<(Value, Option<usize>), LottieError> {
    if kind == "Scalar" {
        if !value.as_f64().is_some_and(f64::is_finite) {
            return Err(invalid("Scalar theme value must be finite"));
        }
        return Ok((value.clone(), None));
    }
    let values = value
        .as_array()
        .ok_or_else(|| invalid(format!("{kind} theme value must be an array")))?;
    if kind == "Gradient" {
        if values.is_empty() {
            return Err(invalid("empty theme gradient"));
        }
        let mut colors = Vec::new();
        let mut opacity = Vec::new();
        for stop in values {
            let offset = stop["offset"]
                .as_f64()
                .filter(|v| (0.0..=1.0).contains(v))
                .ok_or_else(|| invalid("invalid gradient offset"))?;
            let color = stop["color"]
                .as_array()
                .filter(|a| a.len() == 3 || a.len() == 4)
                .ok_or_else(|| invalid("invalid gradient color"))?;
            if color
                .iter()
                .any(|v| !v.as_f64().is_some_and(|v| (0.0..=1.0).contains(&v)))
            {
                return Err(invalid("invalid gradient color channel"));
            }
            colors.push(json!(offset));
            colors.extend(color[..3].iter().cloned());
            opacity.push(json!(offset));
            opacity.push(color.get(3).cloned().unwrap_or(json!(1.0)));
        }
        colors.extend(opacity);
        return Ok((Value::Array(colors), Some(values.len())));
    }
    if !match kind {
        "Color" => values.len() == 3,
        "Vector" => values.len() == 2,
        _ => (2..=3).contains(&values.len()),
    } || (kind == "Color" && values.len() != 3)
        || values.iter().any(|v| {
            !v.as_f64()
                .is_some_and(|n| n.is_finite() && (kind != "Color" || (0.0..=1.0).contains(&n)))
        })
    {
        return Err(invalid(format!("invalid {kind} theme value")));
    }
    Ok((value.clone(), None))
}

fn replace_slots(node: &mut Value, id: &str, value: &Value, points: Option<usize>) {
    match node {
        Value::Object(object) => {
            if object.get("sid").and_then(Value::as_str) == Some(id) {
                object.insert("a".into(), json!(0));
                object.insert("k".into(), value.clone());
                object.remove("x");
                if object.get("s").and_then(Value::as_bool) == Some(true) {
                    object.remove("s");
                    object.remove("y");
                }
                if let Some(points) = points {
                    object.insert("p".into(), json!(points));
                }
                return;
            }
            // A gradient's stop count lives beside its animated k property.
            if let Some(points) = points {
                if object
                    .get("k")
                    .and_then(|k| k.get("sid"))
                    .and_then(Value::as_str)
                    == Some(id)
                {
                    object.insert("p".into(), json!(points));
                }
            }
            for child in object.values_mut() {
                replace_slots(child, id, value, points);
            }
        }
        Value::Array(items) => {
            for item in items {
                replace_slots(item, id, value, points);
            }
        }
        _ => {}
    }
}

#[derive(Debug, Clone)]
pub enum LottieCommand {
    Theme(Option<String>),
    Input(String, LottieInput),
    Event(String),
}

/// Per-clip immutable resources plus authored commands; clones own their state.
#[derive(Debug, Clone)]
pub struct PackagePlayback {
    pub package: Arc<Package>,
    pub fit: &'static str,
    animation: String,
    machine: Option<Machine>,
    initial_theme: Option<String>,
    initial_inputs: HashMap<String, LottieInput>,
    commands: Vec<(f64, LottieCommand)>,
}

pub(super) struct Sample {
    pub asset: Arc<LottieAsset>,
    pub frame: Option<f64>,
    pub background: Option<u32>,
}

impl PackagePlayback {
    pub fn load(path: &Path, options: &LottiePackageOptions) -> Result<Self, LottieError> {
        if options.animation_id.is_some() && options.state_machine_id.is_some() {
            return Err(invalid(
                "animation_id and state_machine_id are mutually exclusive",
            ));
        }
        let package = Package::load(path)?;
        let explicit = options.animation_id.is_some() || options.state_machine_id.is_some();
        let machine_id = options.state_machine_id.as_deref().or_else(|| {
            (!explicit)
                .then(|| package.manifest["initial"]["stateMachine"].as_str())
                .flatten()
        });
        let machine = machine_id
            .map(|id| {
                if !package.state_machine_ids.iter().any(|v| v == id) {
                    return Err(invalid(format!("unknown state machine '{id}'")));
                }
                Machine::parse(&package.json(&format!("s/{id}.json"))?, &package)
            })
            .transpose()?;
        let animation = if let Some(machine) = &machine {
            machine.initial_animation().to_owned()
        } else {
            options
                .animation_id
                .as_deref()
                .or_else(|| package.manifest["initial"]["animation"].as_str())
                .or_else(|| package.manifest["activeAnimationId"].as_str())
                .unwrap_or(&package.animation_ids[0])
                .to_owned()
        };
        let initial_theme = options.theme_id.clone().or_else(|| {
            package.animation_metadata(&animation)["initialTheme"]
                .as_str()
                .map(str::to_owned)
        });
        let session = Self {
            package,
            fit: "contain",
            animation,
            machine,
            initial_theme,
            initial_inputs: HashMap::new(),
            commands: Vec::new(),
        };
        session.prepare_theme(session.initial_theme.as_deref())?;
        session.sample(0.0, 0.0)?;
        Ok(session)
    }

    pub fn initial_asset(&self) -> Result<Arc<LottieAsset>, LottieError> {
        Ok(self.sample(f64::NEG_INFINITY, 0.0)?.asset)
    }

    pub fn is_machine(&self) -> bool {
        self.machine.is_some()
    }

    fn prepare_theme(&self, theme: Option<&str>) -> Result<(), LottieError> {
        if let Some(machine) = &self.machine {
            for id in machine.animations() {
                self.package.asset(id, theme)?;
            }
        } else {
            self.package.asset(&self.animation, theme)?;
        }
        Ok(())
    }

    /// Validate transactionally before recording any mutation.
    pub fn record(
        &mut self,
        command: LottieCommand,
        time: f64,
        start: f64,
        active: bool,
    ) -> Result<(), LottieError> {
        if !time.is_finite() || (active && time < start) {
            return Err(invalid("Lottie command precedes activation"));
        }
        match &command {
            LottieCommand::Theme(id) => self.prepare_theme(id.as_deref())?,
            LottieCommand::Input(name, value) => self
                .machine
                .as_ref()
                .ok_or_else(|| invalid("set_input requires a state machine"))?
                .validate_input(name, Some(value))?,
            LottieCommand::Event(name) => {
                if !active {
                    return Err(invalid("fire_event requires an activated clip"));
                }
                self.machine
                    .as_ref()
                    .ok_or_else(|| invalid("fire_event requires a state machine"))?
                    .validate_input(name, None)?;
            }
        }
        let mut next = self.clone();
        if active {
            next.commands.push((time, command));
            next.commands.sort_by(|a, b| a.0.total_cmp(&b.0));
        } else {
            match command {
                LottieCommand::Theme(theme) => next.initial_theme = theme,
                LottieCommand::Input(name, value) => {
                    next.initial_inputs.insert(name, value);
                }
                LottieCommand::Event(_) => unreachable!(),
            }
        }
        next.sample(next.commands.last().map_or(start, |v| v.0), start)?;
        *self = next;
        Ok(())
    }

    pub(super) fn sample(&self, time: f64, start: f64) -> Result<Sample, LottieError> {
        let mut theme = self.initial_theme.as_deref();
        let mut runtime = self
            .machine
            .as_ref()
            .map(|m| m.start(&self.initial_inputs, start))
            .transpose()?;
        for (at, command) in &self.commands {
            if *at > time {
                break;
            }
            match command {
                LottieCommand::Theme(id) => theme = id.as_deref(),
                command => {
                    if let (Some(machine), Some(runtime)) = (&self.machine, &mut runtime) {
                        machine.apply(runtime, command, *at)?;
                    }
                }
            }
        }
        let (id, frame, background) =
            if let (Some(machine), Some(runtime)) = (&self.machine, &runtime) {
                let state = machine.state(runtime);
                let asset = self.package.asset(&state.animation, theme)?;
                (
                    state.animation.as_str(),
                    Some(state.frame(&asset, (time - runtime.entered).max(0.0))),
                    state.background,
                )
            } else {
                (self.animation.as_str(), None, None)
            };
        let background = background.or_else(|| {
            self.package.animation_metadata(id)["background"]
                .as_u64()
                .and_then(|n| n.try_into().ok())
        });
        Ok(Sample {
            asset: self.package.asset(id, theme)?,
            frame,
            background,
        })
    }
}

#[cfg(test)]
mod tests;
