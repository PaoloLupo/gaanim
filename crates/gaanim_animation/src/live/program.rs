//! Behaviors: the plain Python functions a scene writes for a live zone,
//! compiled when the scene is authored (`gaanim.live.compile_behavior`) into
//! a [`Program`] that travels in the bundle and runs here without Python.
//!
//! A program is a list of instructions in SSA form: instruction `i` writes
//! register `i` and reads only earlier registers, so evaluating it is one
//! pass over a `Vec<f64>`. Python's `if` becomes [`Inst::Select`], helper
//! functions are inlined and module constants frozen, so there are no jumps,
//! calls or loops to run. Numbers follow Python's rules where Rust's differ
//! (`%`, `//`, `round`, `max`/`min`), and transcendental functions come from
//! the pinned `libm` crate, so every platform computes the same poses.

use serde::{Deserialize, Serialize};

/// The IR's version: a program with a newer major version is refused, a
/// newer minor one may only add instructions this build refuses by name.
pub const PROGRAM_VERSION: [u32; 2] = [1, 1];

/// A register: the index of the instruction that writes it.
pub type Reg = u32;

/// What the engine tells a behavior about a player each frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Input {
    /// Seconds since the player arrived in the zone.
    T,
    /// Seconds since the zone opened.
    Time,
    /// When the player arrived, in seconds since the zone opened.
    Joined,
    /// Order of arrival, from 0.
    Index,
    /// Players in the zone.
    Count,
    /// Position in the standings, from 0 for the leader.
    Rank,
    Score,
    /// The leader's score.
    Leader,
    /// The rank before the last change (the rank itself until one).
    PreviousRank,
    /// Seconds since the rank last changed (since arriving until then).
    RankSince,
    PreviousScore,
    ScoreSince,
    /// The player's team, from 0 (0 for everyone in a game without teams).
    Team,
    /// Order of arrival within the team, from 0.
    TeamIndex,
    /// Players of the team in the zone.
    TeamCount,
    /// The team's score: its players' scores added up.
    TeamScore,
    /// Position of the team, from 0 for the leading one.
    TeamRank,
}

impl Input {
    pub const ALL: [Input; 17] = [
        Input::T,
        Input::Time,
        Input::Joined,
        Input::Index,
        Input::Count,
        Input::Rank,
        Input::Score,
        Input::Leader,
        Input::PreviousRank,
        Input::RankSince,
        Input::PreviousScore,
        Input::ScoreSince,
        Input::Team,
        Input::TeamIndex,
        Input::TeamCount,
        Input::TeamScore,
        Input::TeamRank,
    ];
}

/// One instruction. Operands are registers; booleans are 1.0 and 0.0.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Inst {
    /// A constant, written as Python's `repr` of the float (`"0.1"`,
    /// `"inf"`, `"nan"`) so it parses back to the same bits.
    Const(String),
    Input(Input),
    /// A number in [0, 1) read from the player and the operand, the same
    /// for the same player everywhere.
    Random(Reg),
    Neg(Reg),
    Not(Reg),
    /// Python's truth of a number: not zero (NaN is true).
    Truth(Reg),
    Abs(Reg),
    Floor(Reg),
    Ceil(Reg),
    Trunc(Reg),
    /// `round(x)`: halves to even.
    Round(Reg),
    Sqrt(Reg),
    Exp(Reg),
    Expm1(Reg),
    Ln(Reg),
    Log1p(Reg),
    Log2(Reg),
    Log10(Reg),
    Sin(Reg),
    Cos(Reg),
    Tan(Reg),
    Asin(Reg),
    Acos(Reg),
    Atan(Reg),
    Sinh(Reg),
    Cosh(Reg),
    Tanh(Reg),
    IsNan(Reg),
    IsInf(Reg),
    IsFinite(Reg),
    Add(Reg, Reg),
    Sub(Reg, Reg),
    Mul(Reg, Reg),
    Div(Reg, Reg),
    /// Python's `//`.
    FloorDiv(Reg, Reg),
    /// Python's `%`: the sign of the divisor.
    Mod(Reg, Reg),
    /// `math.fmod`: the sign of the dividend.
    Fmod(Reg, Reg),
    Pow(Reg, Reg),
    Atan2(Reg, Reg),
    Hypot(Reg, Reg),
    CopySign(Reg, Reg),
    /// Python's `max(a, b)`: `b` only when `b > a`.
    Max(Reg, Reg),
    /// Python's `min(a, b)`: `b` only when `b < a`.
    Min(Reg, Reg),
    Lt(Reg, Reg),
    Le(Reg, Reg),
    Gt(Reg, Reg),
    Ge(Reg, Reg),
    Eq(Reg, Reg),
    Ne(Reg, Reg),
    And(Reg, Reg),
    Or(Reg, Reg),
    /// `if c then a else b`, with `c` a boolean register.
    Select(Reg, Reg, Reg),
}

impl Inst {
    fn operands(&self) -> Vec<Reg> {
        use Inst::*;
        match *self {
            Const(_) | Input(_) => Vec::new(),
            Random(a) | Neg(a) | Not(a) | Truth(a) | Abs(a) | Floor(a) | Ceil(a) | Trunc(a)
            | Round(a) | Sqrt(a) | Exp(a) | Expm1(a) | Ln(a) | Log1p(a) | Log2(a) | Log10(a)
            | Sin(a) | Cos(a) | Tan(a) | Asin(a) | Acos(a) | Atan(a) | Sinh(a) | Cosh(a)
            | Tanh(a) | IsNan(a) | IsInf(a) | IsFinite(a) => vec![a],
            Add(a, b) | Sub(a, b) | Mul(a, b) | Div(a, b) | FloorDiv(a, b) | Mod(a, b)
            | Fmod(a, b) | Pow(a, b) | Atan2(a, b) | Hypot(a, b) | CopySign(a, b) | Max(a, b)
            | Min(a, b) | Lt(a, b) | Le(a, b) | Gt(a, b) | Ge(a, b) | Eq(a, b) | Ne(a, b)
            | And(a, b) | Or(a, b) => vec![a, b],
            Select(c, a, b) => vec![c, a, b],
        }
    }
}

/// The registers a behavior's `pose(...)` reads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PoseRegs {
    pub x: Reg,
    pub y: Reg,
    pub rotation: Reg,
    pub scale: Reg,
    pub sx: Reg,
    pub sy: Reg,
    pub lean: Reg,
    /// NaN when the engine looks where the character moves.
    pub look_x: Reg,
    pub look_y: Reg,
    /// Whether the nickname shows under the feet, when the zone draws names.
    pub show_name: Reg,
    pub flip: Reg,
    pub visible: Reg,
    /// An index into [`Program::strings`], or negative for none.
    pub express: Reg,
    /// NaN when the behavior leaves it to the engine.
    pub since: Reg,
    #[serde(rename = "loop")]
    pub looped: Reg,
}

impl PoseRegs {
    fn all(&self) -> [Reg; 15] {
        [
            self.x,
            self.y,
            self.rotation,
            self.scale,
            self.sx,
            self.sy,
            self.lean,
            self.look_x,
            self.look_y,
            self.show_name,
            self.flip,
            self.visible,
            self.express,
            self.since,
            self.looped,
        ]
    }
}

/// A compiled behavior.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Program {
    pub version: [u32; 2],
    /// The Python function it came from, for messages.
    #[serde(default)]
    pub name: String,
    /// The expression names it may return.
    #[serde(default)]
    pub strings: Vec<String>,
    pub code: Vec<Inst>,
    pub pose: PoseRegs,
    /// Constants parsed from `code`, so evaluating never parses.
    #[serde(skip)]
    constants: Vec<f64>,
}

/// Why a program cannot run.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ProgramError {
    #[error("{0}")]
    Json(String),
    #[error(
        "the behavior was compiled for version {}.{} of live programs; this Gaanim runs {}.{}",
        found[0], found[1], PROGRAM_VERSION[0], PROGRAM_VERSION[1]
    )]
    Version { found: [u32; 2] },
    #[error("instruction {at} reads register {reg}, which is not written before it")]
    Operand { at: usize, reg: Reg },
    #[error("instruction {at} has constant {text:?}, which is not a number")]
    Constant { at: usize, text: String },
    #[error("pose reads register {reg}, but the program has {len}")]
    Pose { reg: Reg, len: usize },
    #[error("the behavior has no instructions")]
    Empty,
}

/// Where a pose is and how it looks, as a behavior returned it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    /// Where the character's feet are, in scene units.
    pub x: f64,
    pub y: f64,
    /// Radians, counterclockwise, about the character's middle.
    pub rotation: f64,
    pub scale: f64,
    /// Stretch across and along the body, about the feet: squash and
    /// stretch. 1 leaves it as drawn.
    pub sx: f64,
    pub sy: f64,
    /// Radians the body leans counterclockwise, about the feet.
    pub lean: f64,
    /// Where the eyes look, each in [-1, 1] (right and up), or `None` for
    /// where the character moves.
    pub look: Option<(f64, f64)>,
    /// Show the nickname under the feet, when the zone draws names.
    pub show_name: bool,
    /// Mirrored left to right.
    pub flip: bool,
    pub visible: bool,
    /// The expression to play, an index into [`Program::strings`].
    pub express: Option<usize>,
    /// The player's `t` when the expression started, or `None` to start it
    /// when it first appears.
    pub since: Option<f64>,
    pub looped: bool,
}

/// A player's inputs for one evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Inputs {
    pub t: f64,
    pub time: f64,
    pub joined: f64,
    pub index: f64,
    pub count: f64,
    pub rank: f64,
    pub score: f64,
    pub leader: f64,
    pub previous_rank: f64,
    pub rank_since: f64,
    pub previous_score: f64,
    pub score_since: f64,
    pub team: f64,
    pub team_index: f64,
    pub team_count: f64,
    pub team_score: f64,
    pub team_rank: f64,
    /// Read by `p.random(k)`: the player's character seed.
    pub seed: u32,
}

impl Inputs {
    /// The same player `dt` seconds later, for measuring how a pose moves:
    /// the clocks advance, the standings stay.
    pub fn later(&self, dt: f64) -> Self {
        Self {
            t: self.t + dt,
            time: self.time + dt,
            rank_since: self.rank_since + dt,
            score_since: self.score_since + dt,
            ..*self
        }
    }

    pub fn get(&self, input: Input) -> f64 {
        match input {
            Input::T => self.t,
            Input::Time => self.time,
            Input::Joined => self.joined,
            Input::Index => self.index,
            Input::Count => self.count,
            Input::Rank => self.rank,
            Input::Score => self.score,
            Input::Leader => self.leader,
            Input::PreviousRank => self.previous_rank,
            Input::RankSince => self.rank_since,
            Input::PreviousScore => self.previous_score,
            Input::ScoreSince => self.score_since,
            Input::Team => self.team,
            Input::TeamIndex => self.team_index,
            Input::TeamCount => self.team_count,
            Input::TeamScore => self.team_score,
            Input::TeamRank => self.team_rank,
        }
    }
}

impl Program {
    /// Read and check a program as `compile_behavior` writes it.
    pub fn from_json(json: &str) -> Result<Self, ProgramError> {
        let program: Self =
            serde_json::from_str(json).map_err(|error| ProgramError::Json(error.to_string()))?;
        program.checked()
    }

    /// Check a program read from anywhere (a bundle deserializes it without
    /// [`Program::from_json`]) and parse its constants.
    pub fn checked(mut self) -> Result<Self, ProgramError> {
        if self.version[0] != PROGRAM_VERSION[0] || self.version[1] > PROGRAM_VERSION[1] {
            return Err(ProgramError::Version {
                found: self.version,
            });
        }
        if self.code.is_empty() {
            return Err(ProgramError::Empty);
        }
        let mut constants = vec![0.0; self.code.len()];
        for (at, inst) in self.code.iter().enumerate() {
            if let Some(&reg) = inst.operands().iter().find(|&&reg| reg as usize >= at) {
                return Err(ProgramError::Operand { at, reg });
            }
            if let Inst::Const(text) = inst {
                constants[at] = text.parse().map_err(|_| ProgramError::Constant {
                    at,
                    text: text.clone(),
                })?;
            }
        }
        let len = self.code.len();
        if let Some(&reg) = self.pose.all().iter().find(|&&reg| reg as usize >= len) {
            return Err(ProgramError::Pose { reg, len });
        }
        self.constants = constants;
        Ok(self)
    }

    /// Whether [`Program::checked`] parsed the constants.
    fn ready(&self) -> bool {
        self.constants.len() == self.code.len()
    }

    /// Evaluate for one player. `registers` is scratch space reused across
    /// calls. A program that did not go through [`Program::checked`] is
    /// checked first; one that fails poses the character at the origin.
    pub fn eval(&self, inputs: &Inputs, registers: &mut Vec<f64>) -> Pose {
        if !self.ready() {
            return match self.clone().checked() {
                Ok(program) => program.eval(inputs, registers),
                Err(_) => Pose::default(),
            };
        }
        registers.clear();
        registers.reserve(self.code.len());
        for (at, inst) in self.code.iter().enumerate() {
            let value = step(inst, registers, self.constants[at], inputs);
            registers.push(value);
        }
        let r = |reg: Reg| registers[reg as usize];
        let express = r(self.pose.express);
        let since = r(self.pose.since);
        Pose {
            x: r(self.pose.x),
            y: r(self.pose.y),
            rotation: r(self.pose.rotation),
            scale: r(self.pose.scale),
            sx: r(self.pose.sx),
            sy: r(self.pose.sy),
            lean: r(self.pose.lean),
            show_name: r(self.pose.show_name) != 0.0,
            look: {
                let look = (r(self.pose.look_x), r(self.pose.look_y));
                (!look.0.is_nan() && !look.1.is_nan()).then_some(look)
            },
            flip: r(self.pose.flip) != 0.0,
            visible: r(self.pose.visible) != 0.0,
            express: (express >= 0.0 && (express as usize) < self.strings.len())
                .then_some(express as usize),
            since: (!since.is_nan()).then_some(since),
            looped: r(self.pose.looped) != 0.0,
        }
    }
}

impl Default for Pose {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            rotation: 0.0,
            scale: 1.0,
            sx: 1.0,
            sy: 1.0,
            lean: 0.0,
            look: None,
            show_name: true,
            flip: false,
            visible: true,
            express: None,
            since: None,
            looped: false,
        }
    }
}

fn boolean(value: bool) -> f64 {
    if value { 1.0 } else { 0.0 }
}

fn step(inst: &Inst, registers: &[f64], constant: f64, inputs: &Inputs) -> f64 {
    use Inst::*;
    let r = |reg: Reg| registers[reg as usize];
    match *inst {
        Const(_) => constant,
        Input(input) => inputs.get(input),
        Random(k) => random(inputs.seed, r(k)),
        Neg(a) => -r(a),
        Not(a) => boolean(r(a) == 0.0),
        Truth(a) => boolean(r(a) != 0.0),
        Abs(a) => r(a).abs(),
        Floor(a) => r(a).floor(),
        Ceil(a) => r(a).ceil(),
        Trunc(a) => r(a).trunc(),
        Round(a) => r(a).round_ties_even(),
        Sqrt(a) => r(a).sqrt(),
        Exp(a) => libm::exp(r(a)),
        Expm1(a) => libm::expm1(r(a)),
        Ln(a) => libm::log(r(a)),
        Log1p(a) => libm::log1p(r(a)),
        Log2(a) => libm::log2(r(a)),
        Log10(a) => libm::log10(r(a)),
        Sin(a) => libm::sin(r(a)),
        Cos(a) => libm::cos(r(a)),
        Tan(a) => libm::tan(r(a)),
        Asin(a) => libm::asin(r(a)),
        Acos(a) => libm::acos(r(a)),
        Atan(a) => libm::atan(r(a)),
        Sinh(a) => libm::sinh(r(a)),
        Cosh(a) => libm::cosh(r(a)),
        Tanh(a) => libm::tanh(r(a)),
        IsNan(a) => boolean(r(a).is_nan()),
        IsInf(a) => boolean(r(a).is_infinite()),
        IsFinite(a) => boolean(r(a).is_finite()),
        Add(a, b) => r(a) + r(b),
        Sub(a, b) => r(a) - r(b),
        Mul(a, b) => r(a) * r(b),
        Div(a, b) => r(a) / r(b),
        FloorDiv(a, b) => py_divmod(r(a), r(b)).0,
        Mod(a, b) => py_divmod(r(a), r(b)).1,
        Fmod(a, b) => libm::fmod(r(a), r(b)),
        Pow(a, b) => libm::pow(r(a), r(b)),
        Atan2(a, b) => libm::atan2(r(a), r(b)),
        Hypot(a, b) => libm::hypot(r(a), r(b)),
        CopySign(a, b) => r(a).copysign(r(b)),
        Max(a, b) => {
            let (a, b) = (r(a), r(b));
            if b > a { b } else { a }
        }
        Min(a, b) => {
            let (a, b) = (r(a), r(b));
            if b < a { b } else { a }
        }
        Lt(a, b) => boolean(r(a) < r(b)),
        Le(a, b) => boolean(r(a) <= r(b)),
        Gt(a, b) => boolean(r(a) > r(b)),
        Ge(a, b) => boolean(r(a) >= r(b)),
        Eq(a, b) => boolean(r(a) == r(b)),
        Ne(a, b) => boolean(r(a) != r(b)),
        And(a, b) => boolean(r(a) != 0.0 && r(b) != 0.0),
        Or(a, b) => boolean(r(a) != 0.0 || r(b) != 0.0),
        Select(c, a, b) => {
            if r(c) != 0.0 {
                r(a)
            } else {
                r(b)
            }
        }
    }
}

/// CPython's float `divmod` (`Objects/floatobject.c`): `//` and `%` with
/// the divisor's sign. Division by zero gives NaN where Python raises.
pub fn py_divmod(a: f64, b: f64) -> (f64, f64) {
    if b == 0.0 {
        return (f64::NAN, f64::NAN);
    }
    let mut remainder = libm::fmod(a, b);
    let mut quotient = (a - remainder) / b;
    if remainder != 0.0 {
        if (b < 0.0) != (remainder < 0.0) {
            remainder += b;
            quotient -= 1.0;
        }
    } else {
        remainder = 0.0f64.copysign(b);
    }
    let floor = if quotient != 0.0 {
        let floor = quotient.floor();
        if quotient - floor > 0.5 {
            floor + 1.0
        } else {
            floor
        }
    } else {
        0.0f64.copysign(a / b)
    };
    (floor, remainder)
}

/// `p.random(k)`: SplitMix64 of the player's seed and the bits of `k`, as
/// `gaanim.live.Player.random` computes it.
pub fn random(seed: u32, k: f64) -> f64 {
    let k = if k == 0.0 { 0.0 } else { k };
    let mut z = (u64::from(seed) << 32 ^ k.to_bits()).wrapping_add(0x9e37_79b9_7f4a_7c15);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program(code: Vec<Inst>, x: Reg) -> Program {
        Program {
            version: PROGRAM_VERSION,
            name: "test".into(),
            strings: Vec::new(),
            pose: PoseRegs {
                x,
                y: 0,
                rotation: 0,
                scale: 0,
                sx: 0,
                sy: 0,
                lean: 0,
                look_x: 0,
                look_y: 0,
                show_name: 0,
                flip: 0,
                visible: 0,
                express: 0,
                since: 0,
                looped: 0,
            },
            code,
            constants: Vec::new(),
        }
    }

    #[test]
    fn python_division_keeps_the_divisor_sign() {
        assert_eq!(py_divmod(7.0, 2.0), (3.0, 1.0));
        assert_eq!(py_divmod(-7.0, 2.0), (-4.0, 1.0));
        assert_eq!(py_divmod(7.0, -2.0), (-4.0, -1.0));
        assert_eq!(py_divmod(-7.5, 2.0), (-4.0, 0.5));
        let (_, zero) = py_divmod(-4.0, 2.0);
        assert!(zero == 0.0 && zero.is_sign_positive());
        assert!(py_divmod(1.0, 0.0).0.is_nan());
    }

    #[test]
    fn operands_must_come_before_their_reader() {
        let bad = program(vec![Inst::Const("1".into()), Inst::Add(0, 1)], 1).checked();
        assert_eq!(bad, Err(ProgramError::Operand { at: 1, reg: 1 }));
        let bad = program(vec![Inst::Const("one".into())], 0).checked();
        assert!(matches!(bad, Err(ProgramError::Constant { .. })));
        let mut newer = program(vec![Inst::Const("1".into())], 0);
        newer.version = [PROGRAM_VERSION[0] + 1, 0];
        assert!(matches!(newer.checked(), Err(ProgramError::Version { .. })));
    }

    #[test]
    fn a_program_selects_like_python_if() {
        // x = 2 * t if t < 1 else max(t, nan) + 0.5
        let code = vec![
            Inst::Input(Input::T),
            Inst::Const("1".into()),
            Inst::Lt(0, 1),
            Inst::Const("2".into()),
            Inst::Mul(3, 0),
            Inst::Const("nan".into()),
            Inst::Max(0, 5),
            Inst::Const("0.5".into()),
            Inst::Add(6, 7),
            Inst::Select(2, 4, 8),
        ];
        let program = program(code, 9).checked().unwrap();
        let mut registers = Vec::new();
        let at = |t: f64, registers: &mut Vec<f64>| {
            program
                .eval(
                    &Inputs {
                        t,
                        ..Inputs::default()
                    },
                    registers,
                )
                .x
        };
        assert_eq!(at(0.25, &mut registers), 0.5);
        assert_eq!(at(3.0, &mut registers), 3.5);
    }

    #[test]
    fn a_program_reads_back_from_json() {
        let json = r#"{"version":[1,0],"name":"f","strings":["happy"],
            "code":[{"input":"t"},{"const":"0.1"},{"add":[0,1]},{"const":"0"},{"const":"nan"}],
            "pose":{"x":2,"y":1,"rotation":3,"scale":1,"sx":1,"sy":1,"lean":3,"look_x":4,"look_y":4,"show_name":1,"flip":3,"visible":1,
                    "express":3,"since":4,"loop":3}}"#;
        let program = Program::from_json(json).unwrap();
        let pose = program.eval(
            &Inputs {
                t: 0.2,
                ..Inputs::default()
            },
            &mut Vec::new(),
        );
        assert_eq!(pose.x, 0.2 + 0.1);
        assert_eq!(pose.express, Some(0));
        assert_eq!(pose.since, None);
        assert!(pose.visible && !pose.flip);
    }

    #[test]
    fn random_is_in_the_unit_interval_and_stable() {
        let a = random(12345, 0.0);
        assert!((0.0..1.0).contains(&a));
        assert_eq!(a, random(12345, -0.0));
        assert_ne!(a, random(12345, 1.0));
        assert_ne!(a, random(12346, 0.0));
    }

    /// The programs `tests/live/generate_cases.py` compiled, with the poses
    /// CPython returned for sample players: the compiler and this evaluator
    /// must give the same.
    #[test]
    fn compiled_behaviors_pose_like_python() {
        let entries: serde_json::Value =
            serde_json::from_str(include_str!("cases.json")).unwrap();
        let number = |value: &serde_json::Value| -> f64 { value.as_str().unwrap().parse().unwrap() };
        let mut registers = Vec::new();
        for entry in entries.as_array().unwrap() {
            let program: Program = serde_json::from_value(entry["program"].clone()).unwrap();
            let program = program.checked().unwrap();
            for case in entry["cases"].as_array().unwrap() {
                let given = &case["inputs"];
                // Cases written before teams have none.
                let input = |name: &str| given.get(name).map_or(0.0, number);
                let inputs = Inputs {
                    t: input("t"),
                    time: input("time"),
                    joined: input("joined"),
                    index: input("index"),
                    count: input("count"),
                    rank: input("rank"),
                    score: input("score"),
                    leader: input("leader"),
                    previous_rank: input("previous_rank"),
                    rank_since: input("rank_since"),
                    previous_score: input("previous_score"),
                    score_since: input("score_since"),
                    team: input("team"),
                    team_index: input("team_index"),
                    team_count: input("team_count"),
                    team_score: input("team_score"),
                    team_rank: input("team_rank"),
                    seed: given["seed"].as_u64().unwrap() as u32,
                };
                let pose = program.eval(&inputs, &mut registers);
                let expected = &case["pose"];
                let context = format!("{} with {given}", program.name);
                // Transcendental functions may differ from the platform's
                // C library in the last bit; everything else is exact.
                let close = |got: f64, want: f64| {
                    got == want
                        || (got.is_nan() && want.is_nan())
                        || (got - want).abs() <= 1e-12 * want.abs().max(1.0)
                };
                for (field, got) in [
                    ("x", pose.x),
                    ("y", pose.y),
                    ("rotation", pose.rotation),
                    ("scale", pose.scale),
                    ("sx", pose.sx),
                    ("sy", pose.sy),
                    ("lean", pose.lean),
                ] {
                    let want = number(&expected[field]);
                    assert!(close(got, want), "{field}: {got} != {want} in {context}");
                }
                let look = expected["look"].as_array().map(|look| {
                    (number(&look[0]), number(&look[1]))
                });
                match (pose.look, look) {
                    (Some(got), Some(want)) => assert!(
                        close(got.0, want.0) && close(got.1, want.1),
                        "look in {context}"
                    ),
                    (got, want) => assert_eq!(got, want, "look in {context}"),
                }
                assert_eq!(pose.flip, expected["flip"].as_bool().unwrap(), "{context}");
                assert_eq!(
                    pose.show_name,
                    expected["show_name"].as_bool().unwrap(),
                    "{context}"
                );
                assert_eq!(pose.visible, expected["visible"].as_bool().unwrap(), "{context}");
                assert_eq!(pose.looped, expected["loop"].as_bool().unwrap(), "{context}");
                assert_eq!(
                    pose.express.map(|index| program.strings[index].as_str()),
                    expected["express"].as_str(),
                    "{context}"
                );
                match expected["since"].as_str() {
                    None => assert_eq!(pose.since, None, "{context}"),
                    Some(since) => {
                        let want: f64 = since.parse().unwrap();
                        assert!(close(pose.since.unwrap(), want), "since in {context}");
                    }
                }
            }
        }
    }
}
