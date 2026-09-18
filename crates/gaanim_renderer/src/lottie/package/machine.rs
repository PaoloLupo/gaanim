//! Deterministic, event-driven subset of dotLottie v2 state machines.
use super::{LottieAsset, LottieCommand, LottieError, Package, Value, array, invalid, string};
use std::collections::{HashMap, HashSet};

/// A typed input value. Event inputs are fired separately and never persist.
#[derive(Debug, Clone, PartialEq)]
pub enum LottieInput {
    Numeric(f64),
    Boolean(bool),
    String(String),
}

impl LottieInput {
    fn parse(kind: &str, value: &Value) -> Result<Self, LottieError> {
        match kind {
            "Numeric" => value.as_f64().filter(|v| v.is_finite()).map(Self::Numeric),
            "Boolean" => value.as_bool().map(Self::Boolean),
            "String" => value.as_str().map(|v| Self::String(v.to_owned())),
            _ => None,
        }
        .ok_or_else(|| invalid(format!("invalid {kind} input value")))
    }
    fn kind(&self) -> &'static str {
        match self {
            Self::Numeric(_) => "Numeric",
            Self::Boolean(_) => "Boolean",
            Self::String(_) => "String",
        }
    }
}

#[derive(Debug, Clone)]
enum Comparison {
    Literal(LottieInput),
    Input(String),
    Event,
}

#[derive(Debug, Clone)]
struct Guard {
    name: String,
    condition: String,
    compare: Comparison,
}

impl Guard {
    fn matches(&self, inputs: &HashMap<String, LottieInput>, event: Option<&str>) -> bool {
        if matches!(self.compare, Comparison::Event) {
            return event == Some(self.name.as_str());
        }
        let Some(left) = inputs.get(&self.name) else {
            return false;
        };
        let right = match &self.compare {
            Comparison::Literal(value) => value,
            Comparison::Input(name) => match inputs.get(name) {
                Some(v) => v,
                None => return false,
            },
            Comparison::Event => unreachable!(),
        };
        match self.condition.as_str() {
            "Equal" => left == right,
            "NotEqual" => left != right,
            condition => {
                if let (LottieInput::Numeric(a), LottieInput::Numeric(b)) = (left, right) {
                    match condition {
                        "LessThan" => a < b,
                        "LessThanOrEqual" => a <= b,
                        "GreaterThan" => a > b,
                        "GreaterThanOrEqual" => a >= b,
                        _ => false,
                    }
                } else {
                    false
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
struct Transition {
    target: usize,
    guards: Vec<Guard>,
}

#[derive(Debug, Clone)]
pub(super) struct State {
    pub animation: String,
    global: bool,
    final_state: bool,
    autoplay: bool,
    looping: bool,
    loop_count: Option<u64>,
    speed: f64,
    mode: String,
    segment: Option<(f64, f64)>,
    pub background: Option<u32>,
    transitions: Vec<Transition>,
}

impl State {
    pub fn frame(&self, asset: &LottieAsset, elapsed: f64) -> f64 {
        let frames = &asset.composition.frames;
        let (start, end) = self.segment.unwrap_or((frames.start, frames.end));
        let length = end - start;
        let bounce = matches!(self.mode.as_str(), "Bounce" | "ReverseBounce");
        let cycle = length * if bounce { 2.0 } else { 1.0 };
        let distance = if self.autoplay {
            elapsed * asset.frame_rate() * self.speed
        } else {
            0.0
        };
        let total = if self.looping {
            self.loop_count.map(|n| n as f64 * cycle)
        } else {
            Some(cycle)
        };
        let mut phase = if total.is_some_and(|total| distance >= total) {
            cycle
        } else {
            distance.rem_euclid(cycle)
        };
        if bounce && phase > length {
            phase = cycle - phase;
        }
        if matches!(self.mode.as_str(), "Reverse" | "ReverseBounce") {
            phase = length - phase;
        }
        // Very short valid markers must not invert the clamp interval.
        (start + phase).clamp(start, end - (length * 0.5).min(1e-7))
    }
}

#[derive(Debug, Clone)]
pub(super) struct Machine {
    states: Vec<State>,
    initial: usize,
    initial_animation: String,
    inputs: HashMap<String, Option<LottieInput>>,
}

pub(super) struct Runtime {
    current: usize,
    inputs: HashMap<String, LottieInput>,
    pub entered: f64,
}

fn boolean(value: &Value, key: &str, default: bool) -> Result<bool, LottieError> {
    value
        .get(key)
        .map(|v| {
            v.as_bool()
                .ok_or_else(|| invalid(format!("invalid boolean '{key}'")))
        })
        .unwrap_or(Ok(default))
}

fn no_actions(value: &Value) -> Result<(), LottieError> {
    for key in ["entryActions", "exitActions", "actions", "interactions"] {
        if !array(value, key)?.is_empty() {
            return Err(invalid(format!("state machine '{key}' are unsupported")));
        }
    }
    Ok(())
}

impl Machine {
    pub fn parse(json: &Value, package: &Package) -> Result<Self, LottieError> {
        no_actions(json)?;
        let mut inputs = HashMap::new();
        for input in array(json, "inputs")? {
            let name = string(input, "name")?;
            if name.starts_with('$')
                || matches!(
                    name,
                    "elapsed_time"
                        | "pointer_x"
                        | "pointer_y"
                        | "current_frame"
                        | "current_progress"
                )
            {
                return Err(invalid(format!(
                    "reserved automatic input '{name}' is unsupported"
                )));
            }
            let kind = string(input, "type")?;
            let value = if kind == "Event" {
                None
            } else {
                Some(LottieInput::parse(kind, &input["value"])?)
            };
            if inputs.insert(name.to_owned(), value).is_some() {
                return Err(invalid(format!("duplicate input '{name}'")));
            }
        }
        let raw_states = array(json, "states")?;
        let mut names = HashMap::new();
        for (index, state) in raw_states.iter().enumerate() {
            let name = string(state, "name")?;
            if names.insert(name, index).is_some() {
                return Err(invalid(format!("duplicate state '{name}'")));
            }
        }
        let initial = *names
            .get(string(json, "initial")?)
            .ok_or_else(|| invalid("unknown initial state"))?;
        let mut states = Vec::new();
        for state in raw_states {
            no_actions(state)?;
            let global = match string(state, "type")? {
                "GlobalState" => true,
                "PlaybackState" => false,
                kind => return Err(invalid(format!("unsupported state '{kind}'"))),
            };
            let animation = if global {
                String::new()
            } else {
                string(state, "animation")?.to_owned()
            };
            let source = if global {
                Value::Null
            } else {
                package.animation_json(&animation)?
            };
            let speed = state
                .get("speed")
                .map(|v| {
                    v.as_f64()
                        .filter(|v| v.is_finite() && *v > 0.0)
                        .ok_or_else(|| invalid("invalid state speed"))
                })
                .unwrap_or(Ok(1.0))?;
            let mode = state
                .get("mode")
                .map(|_| string(state, "mode"))
                .transpose()?
                .unwrap_or("Forward");
            if !matches!(mode, "Forward" | "Reverse" | "Bounce" | "ReverseBounce") {
                return Err(invalid(format!("unsupported playback mode '{mode}'")));
            }
            let loop_count = state
                .get("loopCount")
                .map(|v| {
                    v.as_u64()
                        .filter(|v| *v > 0)
                        .ok_or_else(|| invalid("invalid loopCount"))
                })
                .transpose()?;
            let background = state
                .get("backgroundColor")
                .map(|v| {
                    v.as_u64()
                        .and_then(|n| n.try_into().ok())
                        .ok_or_else(|| invalid("invalid backgroundColor"))
                })
                .transpose()?;
            let segment = if let Some(marker) = state.get("segment") {
                let marker = marker
                    .as_str()
                    .ok_or_else(|| invalid("state segment must name a marker"))?;
                array(&source, "markers")?
                    .iter()
                    .find(|m| m["cm"] == marker)
                    .map(|m| {
                        let start = m["tm"]
                            .as_f64()
                            .ok_or_else(|| invalid("invalid marker start"))?;
                        let duration = m["dr"]
                            .as_f64()
                            .ok_or_else(|| invalid("invalid marker duration"))?;
                        if !start.is_finite()
                            || !duration.is_finite()
                            || duration <= 0.0
                            || start < source["ip"].as_f64().unwrap_or(0.0)
                            || start + duration > source["op"].as_f64().unwrap_or(0.0)
                        {
                            return Err(invalid("marker is outside the animation frame range"));
                        }
                        Ok((start, start + duration))
                    })
                    .transpose()?
            } else {
                None
            };
            let mut transitions = Vec::new();
            for transition in array(state, "transitions")? {
                no_actions(transition)?;
                if string(transition, "type")? != "Transition" {
                    return Err(invalid("only immediate state transitions are supported"));
                }
                let target_name = string(transition, "toState")?;
                let target = *names
                    .get(target_name)
                    .ok_or_else(|| invalid(format!("unknown target state '{target_name}'")))?;
                if raw_states[target]["type"] == "GlobalState" {
                    return Err(invalid("transition targets must be playback states"));
                }
                let mut guards = Vec::new();
                for guard in array(transition, "guards")? {
                    let kind = string(guard, "type")?;
                    let name = string(guard, "inputName")?;
                    let input = inputs
                        .get(name)
                        .ok_or_else(|| invalid(format!("unknown guard input '{name}'")))?;
                    if input.as_ref().map_or("Event", LottieInput::kind) != kind {
                        return Err(invalid("guard type does not match input"));
                    }
                    let (condition, compare) = if kind == "Event" {
                        (String::new(), Comparison::Event)
                    } else {
                        let condition = string(guard, "conditionType")?;
                        if !matches!(condition, "Equal" | "NotEqual")
                            && !(kind == "Numeric"
                                && matches!(
                                    condition,
                                    "LessThan"
                                        | "LessThanOrEqual"
                                        | "GreaterThan"
                                        | "GreaterThanOrEqual"
                                ))
                        {
                            return Err(invalid(format!(
                                "unsupported guard comparison '{condition}'"
                            )));
                        }
                        let value = &guard["compareTo"];
                        let compare = if let Some(reference) =
                            value.as_str().and_then(|s| s.strip_prefix('$'))
                        {
                            if inputs
                                .get(reference)
                                .and_then(Option::as_ref)
                                .map(LottieInput::kind)
                                != Some(kind)
                            {
                                return Err(invalid("invalid guard input reference"));
                            }
                            Comparison::Input(reference.to_owned())
                        } else {
                            Comparison::Literal(LottieInput::parse(kind, value)?)
                        };
                        (condition.to_owned(), compare)
                    };
                    guards.push(Guard {
                        name: name.to_owned(),
                        condition,
                        compare,
                    });
                }
                transitions.push(Transition { target, guards });
            }
            // Stable ordering: guarded transitions precede fallback transitions.
            transitions.sort_by_key(|t| t.guards.is_empty());
            states.push(State {
                animation,
                global,
                final_state: boolean(state, "final", false)?,
                autoplay: boolean(state, "autoplay", true)?,
                looping: boolean(state, "loop", false)?,
                loop_count,
                speed,
                mode: mode.to_owned(),
                segment,
                background,
                transitions,
            });
        }
        let mut machine = Self {
            states,
            initial,
            initial_animation: String::new(),
            inputs,
        };
        let runtime = machine.start(&HashMap::new(), 0.0)?;
        machine.initial_animation = machine.states[runtime.current].animation.clone();
        Ok(machine)
    }

    pub fn initial_animation(&self) -> &str {
        &self.initial_animation
    }
    pub fn animations(&self) -> impl Iterator<Item = &str> {
        self.states
            .iter()
            .filter(|s| !s.global)
            .map(|s| s.animation.as_str())
    }
    pub fn state(&self, runtime: &Runtime) -> &State {
        &self.states[runtime.current]
    }

    pub fn validate_input(
        &self,
        name: &str,
        value: Option<&LottieInput>,
    ) -> Result<(), LottieError> {
        let expected = self
            .inputs
            .get(name)
            .ok_or_else(|| invalid(format!("unknown input '{name}'")))?;
        if expected.as_ref().map(LottieInput::kind) != value.map(LottieInput::kind)
            || matches!(value, Some(LottieInput::Numeric(n)) if !n.is_finite())
        {
            return Err(invalid(format!("wrong value type for input '{name}'")));
        }
        Ok(())
    }

    pub fn start(
        &self,
        overrides: &HashMap<String, LottieInput>,
        time: f64,
    ) -> Result<Runtime, LottieError> {
        let mut inputs: HashMap<_, _> = self
            .inputs
            .iter()
            .filter_map(|(k, v)| v.clone().map(|v| (k.clone(), v)))
            .collect();
        inputs.extend(overrides.clone());
        let mut runtime = Runtime {
            current: self.initial,
            inputs,
            entered: time,
        };
        self.settle(&mut runtime, None, time)?;
        if self.states[runtime.current].global {
            return Err(invalid(
                "initial global state must resolve to a playback state",
            ));
        }
        Ok(runtime)
    }

    pub fn apply(
        &self,
        runtime: &mut Runtime,
        command: &LottieCommand,
        time: f64,
    ) -> Result<(), LottieError> {
        let event = match command {
            LottieCommand::Input(name, value) => {
                runtime.inputs.insert(name.clone(), value.clone());
                None
            }
            LottieCommand::Event(name) => Some(name.as_str()),
            LottieCommand::Theme(_) => None,
        };
        self.settle(runtime, event, time)
    }

    fn settle(
        &self,
        runtime: &mut Runtime,
        mut event: Option<&str>,
        time: f64,
    ) -> Result<(), LottieError> {
        let mut visited = HashSet::new();
        loop {
            if self.states[runtime.current].final_state {
                return Ok(());
            }
            if !visited.insert((runtime.current, event.is_some())) {
                return Err(invalid("instantaneous state transition cycle"));
            }
            let current = runtime.current;
            let transition = self
                .states
                .iter()
                .filter(|s| s.global)
                .chain(std::iter::once(&self.states[runtime.current]))
                // Global overrides already targeting the active state are satisfied;
                // they must not continually re-enter it and reset its playback clock.
                .flat_map(|s| {
                    s.transitions
                        .iter()
                        .filter(move |t| !s.global || t.target != current)
                })
                .find(|t| t.guards.iter().all(|g| g.matches(&runtime.inputs, event)));
            let Some(transition) = transition else {
                return Ok(());
            };
            runtime.current = transition.target;
            runtime.entered = time;
            event = None; // edge-triggered events are consumed by one transition
        }
    }
}
