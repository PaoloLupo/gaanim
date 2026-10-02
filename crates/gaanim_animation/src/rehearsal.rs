//! A rehearsal: a made-up audience that plays a scene's polls along the
//! timeline, so previews, exports and snapshots show what a real session
//! would, without phones (`scene.rehearsal`).
//!
//! The scene describes the crowd ([`RehearsalSpec`]) and each poll how it
//! leans ([`Lean`]); compiling the scene plans the whole session once
//! ([`Rehearsal::plan`]): when each player joins, and what each one answers
//! and when. [`Rehearsal::results_at`] then reports, for any timeline time,
//! exactly what the relay would: who is in the room, the votes so far, each
//! quiz's seconds left and the standings. Everything that shows audience
//! data reads it the same way it reads a live presentation, so a preview
//! plays like a session, sped up to the scene's own timing: a question's
//! answers arrive between its opening and the stop where a presentation
//! would wait for them.
//!
//! The plan depends on the spec, the seed and the scene's timing alone, so
//! every preview, seek and export of a scene shows the same session.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::DetectChanges;

use serde::{Deserialize, Serialize};

use crate::polls::{AUDIENCE_AGE_CAP, PollResults};

/// Nicknames of made-up players, in the order they join; more players than
/// names reuse them with a number.
pub const NAMES: [&str; 30] = [
    "Ana", "Beto", "Caro", "Dani", "Eli", "Fede", "Gabi", "Hugo", "Iris", "Juan", "Kai", "Lía",
    "Mateo", "Nora", "Omar", "Paz", "Quique", "Rosa", "Sofi", "Tomás", "Uma", "Vero", "Walter",
    "Ximena", "Yago", "Zoe", "Bruno", "Clara", "Diego", "Emma",
];

/// Most players a rehearsal makes up, as many as a room shows.
pub const MAX_PLAYERS: usize = 200;

/// Who plays a rehearsal, as a scene describes it.
#[derive(Debug, Clone, PartialEq)]
pub struct RehearsalSpec {
    /// The players' nicknames, in the order they join.
    pub names: Vec<String>,
    /// Picks another crowd: other join times, answers and characters.
    pub seed: u64,
    /// Seconds over which the players join once the room opens; `None`
    /// fills the time until the room's first stop.
    pub arrive: Option<f64>,
    /// How often a player answers a quiz right, on average (0 to 1).
    pub skill: f64,
    /// How fast players answer, from 0 (at the last moment) to 1 (at once).
    pub speed: f64,
    /// The skill of each team's players, in the scene's team order; empty
    /// gives every team `skill`.
    pub team_skill: Vec<f64>,
}

/// A game's teams, as a rehearsal joins them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RehearsalTeams {
    pub count: usize,
    /// Players choose their team, so teams come out uneven; otherwise each
    /// joins the smallest, as the relay deals them.
    pub choose: bool,
}

impl RehearsalSpec {
    /// `count` players named from [`NAMES`].
    pub fn names(count: usize) -> Vec<String> {
        (0..count)
            .map(|index| {
                let name = NAMES[index % NAMES.len()];
                match index / NAMES.len() {
                    0 => name.to_string(),
                    round => format!("{name} {}", round + 1),
                }
            })
            .collect()
    }
}

impl Default for RehearsalSpec {
    fn default() -> Self {
        Self {
            names: Self::names(12),
            seed: 0,
            arrive: None,
            skill: 0.6,
            speed: 0.5,
            team_skill: Vec::new(),
        }
    }
}

/// How a poll's rehearsed answers lean.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Lean {
    /// Made up: a poll leans some random way, a quiz follows the players'
    /// skill.
    #[default]
    Auto,
    /// This share of the players answers the quiz right (0 to 1).
    Right(f64),
    /// One weight per answer: answers are picked in these proportions.
    Weights(Vec<f64>),
}

/// A poll as a rehearsal plans it, on the timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedPoll {
    pub id: String,
    pub answers: usize,
    pub open: f64,
    /// When the answers are all in: the first stop after `open`, where a
    /// presentation waits for them, or else the close.
    pub due: f64,
    /// `(correct answers, one bit each; seconds; points)` for a quiz.
    pub quiz: Option<(u32, u32, u32)>,
    /// Players may choose several answers.
    pub multiple: bool,
    pub lean: Lean,
}

/// A made-up player.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RehearsedPlayer {
    pub name: String,
    /// Timeline time the player joins.
    pub joined: f64,
    /// Seed of the character the player made.
    pub character: u32,
    /// The player's team, 0 in a game without teams.
    #[serde(default)]
    pub team: usize,
}

/// A made-up answer.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RehearsedVote {
    /// Index into [`Rehearsal::players`].
    pub player: usize,
    /// The answers chosen, one bit each.
    pub options: u32,
    /// Timeline time the answer arrives.
    pub at: f64,
    /// Points it earned, for a quiz.
    pub points: u32,
}

/// A poll's made-up answers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RehearsedPoll {
    pub id: String,
    pub answers: usize,
    pub open: f64,
    pub due: f64,
    /// Seconds to answer, for a quiz.
    pub time: Option<f64>,
    pub votes: Vec<RehearsedVote>,
}

/// A planned session: every player and answer, on the timeline.
#[derive(bevy::prelude::Resource, Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Rehearsal {
    pub players: Vec<RehearsedPlayer>,
    pub polls: Vec<RehearsedPoll>,
    /// How many teams the game has, 0 without teams.
    #[serde(default)]
    pub teams: usize,
}

/// SplitMix64: a random number from a seed and the path to it, so each
/// choice depends on what it is about and nothing else.
fn random(seed: u64, keys: &[u64]) -> f64 {
    let mut state = seed;
    for key in keys {
        state = state.wrapping_add(0x9e37_79b9_7f4a_7c15).wrapping_add(*key);
        state = (state ^ (state >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        state = (state ^ (state >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        state ^= state >> 31;
    }
    (state >> 11) as f64 / (1u64 << 53) as f64
}

/// What each random number is about.
mod about {
    pub const JOIN: u64 = 1;
    pub const SKILL: u64 = 2;
    pub const SPEED: u64 = 3;
    pub const CHARACTER: u64 = 4;
    pub const WEIGHT: u64 = 5;
    pub const RIGHT: u64 = 6;
    pub const PICK: u64 = 7;
    pub const WHEN: u64 = 8;
    pub const TEAM: u64 = 9;
}

/// The index `weights` pick for a draw `u` in [0, 1).
fn pick(weights: &[f64], u: f64) -> usize {
    let total: f64 = weights.iter().sum();
    let mut left = u * total;
    for (index, weight) in weights.iter().enumerate() {
        if left < *weight {
            return index;
        }
        left -= weight;
    }
    weights.len() - 1
}

impl Rehearsal {
    /// Plan the session of `spec`: the room is open from `room.0` and its
    /// first stop is at `room.1`, if the scene shows its audience; `polls`
    /// are the scene's polls in order, `teams` the game's teams.
    pub fn plan(
        spec: &RehearsalSpec,
        room: Option<(f64, f64)>,
        polls: &[PlannedPoll],
        teams: Option<RehearsalTeams>,
    ) -> Self {
        let seed = spec.seed;
        let count = spec.names.len();
        // Players join over the room's time, in order and a little apart;
        // without a room they joined long ago.
        let (start, span) = match (room, spec.arrive) {
            (Some((open, _)), Some(arrive)) => (open, arrive),
            (Some((open, stop)), None) => (open, (stop - open).max(0.0)),
            (None, Some(arrive)) => (polls.first().map_or(0.0, |poll| poll.open), arrive),
            (None, None) => (-AUDIENCE_AGE_CAP, 0.0),
        };
        let mut sizes = vec![0usize; teams.map_or(0, |teams| teams.count)];
        let players: Vec<RehearsedPlayer> = spec
            .names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let team = match teams {
                    None => 0,
                    Some(RehearsalTeams {
                        count,
                        choose: true,
                    }) => {
                        let draw = random(seed, &[about::TEAM, index as u64]);
                        ((draw * count as f64) as usize).min(count - 1)
                    }
                    // The smallest team, the first on a tie.
                    Some(_) => (0..sizes.len())
                        .min_by_key(|team| sizes[*team])
                        .unwrap_or(0),
                };
                if let Some(size) = sizes.get_mut(team) {
                    *size += 1;
                }
                let jitter = 0.2 + 0.6 * random(seed, &[about::JOIN, index as u64]);
                RehearsedPlayer {
                    name: name.clone(),
                    joined: start + span * (index as f64 + jitter) / count.max(1) as f64,
                    character: (random(seed, &[about::CHARACTER, index as u64])
                        * f64::from(u32::MAX)) as u32,
                    team,
                }
            })
            .collect();
        // Each player is a little better or worse, faster or slower.
        let base = |team: usize| spec.team_skill.get(team).copied().unwrap_or(spec.skill);
        let skill: Vec<f64> = (0..count)
            .map(|index| {
                let spread = random(seed, &[about::SKILL, index as u64]) - 0.5;
                (base(players[index].team) + 0.5 * spread).clamp(0.02, 0.98)
            })
            .collect();
        let speed: Vec<f64> = (0..count)
            .map(|index| {
                let spread = random(seed, &[about::SPEED, index as u64]) - 0.5;
                (spec.speed + 0.4 * spread).clamp(0.0, 1.0)
            })
            .collect();
        let polls = polls
            .iter()
            .enumerate()
            .map(|(number, poll)| {
                let key = number as u64;
                // A poll without a lean leans some random way.
                let weights: Vec<f64> = match &poll.lean {
                    Lean::Weights(weights) => weights.clone(),
                    _ => (0..poll.answers)
                        .map(|answer| {
                            0.15 + random(seed, &[about::WEIGHT, key, answer as u64]).powf(1.5)
                        })
                        .collect(),
                };
                let votes = players
                    .iter()
                    .enumerate()
                    .filter(|(_, player)| player.joined < poll.due)
                    .map(|(index, player)| {
                        let player_key = index as u64;
                        // Fast players answer early; nobody at the very end.
                        let draw = random(seed, &[about::WHEN, key, player_key]);
                        let fraction =
                            (0.04 + (1.0 - speed[index]) * 1.1 * draw.powf(1.3)).min(0.96);
                        let from = player.joined.max(poll.open);
                        let at = from + fraction * (poll.due - from).max(0.0);
                        let pick_draw = random(seed, &[about::PICK, key, player_key]);
                        let options = match (poll.quiz, &poll.lean) {
                            (Some((correct, _, _)), Lean::Auto | Lean::Right(_)) => {
                                let right = match poll.lean {
                                    // The question sets the average; players (and
                                    // teams) keep how far they are from it.
                                    Lean::Right(share) => {
                                        (share + skill[index] - spec.skill).clamp(0.0, 1.0)
                                    }
                                    _ => skill[index],
                                };
                                if random(seed, &[about::RIGHT, key, player_key]) < right {
                                    correct
                                } else if poll.multiple {
                                    // Almost: one answer too many or missing.
                                    let flip = (pick_draw * poll.answers as f64) as usize;
                                    let wrong = correct ^ (1 << flip.min(poll.answers - 1));
                                    if wrong == 0 { correct ^ 1 ^ 2 } else { wrong }
                                } else {
                                    // A wrong answer, any of the others.
                                    let correct = correct.trailing_zeros() as usize;
                                    let wrong = (pick_draw * (poll.answers - 1) as f64) as usize;
                                    let wrong = wrong.min(poll.answers - 2);
                                    1 << if wrong >= correct { wrong + 1 } else { wrong }
                                }
                            }
                            _ if poll.multiple => {
                                // Each answer on its own, the popular ones more
                                // often; at least one.
                                let top = weights.iter().copied().fold(0.0, f64::max).max(1e-9);
                                let chosen = (0..poll.answers).fold(0u32, |chosen, answer| {
                                    let draw = random(
                                        seed,
                                        &[about::PICK, key, player_key, answer as u64],
                                    );
                                    if draw < 0.8 * weights[answer] / top {
                                        chosen | 1 << answer
                                    } else {
                                        chosen
                                    }
                                });
                                if chosen == 0 {
                                    1 << pick(&weights, pick_draw)
                                } else {
                                    chosen
                                }
                            }
                            _ => 1 << pick(&weights, pick_draw),
                        };
                        // Scored as the relay scores: `fraction` is the share of
                        // the quiz's time taken.
                        let points = match poll.quiz {
                            Some((correct, _, points)) if options == correct => {
                                (f64::from(points) * (1.0 - fraction / 2.0)).round() as u32
                            }
                            _ => 0,
                        };
                        RehearsedVote {
                            player: index,
                            options,
                            at,
                            points,
                        }
                    })
                    .collect();
                RehearsedPoll {
                    id: poll.id.clone(),
                    answers: poll.answers,
                    open: poll.open,
                    due: poll.due.max(poll.open),
                    time: poll.quiz.map(|(_, time, _)| f64::from(time)),
                    votes,
                }
            })
            .collect();
        let teams = teams.map_or(0, |teams| teams.count);
        Self {
            players,
            polls,
            teams,
        }
    }

    /// The players' scores and correct answers at `time`, in player order.
    pub fn scores(&self, time: f64) -> Vec<(u64, u32)> {
        let mut scores = vec![(0u64, 0u32); self.players.len()];
        for vote in self
            .polls
            .iter()
            .flat_map(|poll| &poll.votes)
            .filter(|vote| vote.at <= time)
        {
            if let Some(score) = scores.get_mut(vote.player) {
                score.0 += u64::from(vote.points);
                score.1 += u32::from(vote.points > 0);
            }
        }
        scores
    }

    /// Each player's game at `time`, in player order: points, quizzes
    /// answered and right, and the run of right answers up to the last quiz
    /// whose answers are all in.
    pub fn stats_at(&self, time: f64) -> Vec<crate::polls::PlayerStats> {
        let mut stats = vec![crate::polls::PlayerStats::default(); self.players.len()];
        for poll in self.polls.iter().filter(|poll| poll.time.is_some()) {
            for vote in poll.votes.iter().filter(|vote| vote.at <= time) {
                let Some(player) = stats.get_mut(vote.player) else {
                    continue;
                };
                player.score += u64::from(vote.points);
                player.answered += 1;
                player.correct += u32::from(vote.points > 0);
            }
        }
        // Streaks follow the quizzes in order, once each one is over.
        let mut quizzes: Vec<&RehearsedPoll> = self
            .polls
            .iter()
            .filter(|poll| poll.time.is_some() && poll.due <= time)
            .collect();
        quizzes.sort_by(|a, b| a.due.total_cmp(&b.due));
        for quiz in quizzes {
            for (index, player) in self.players.iter().enumerate() {
                if player.joined > quiz.due {
                    continue;
                }
                let right = quiz
                    .votes
                    .iter()
                    .any(|vote| vote.player == index && vote.points > 0);
                stats[index].streak = if right { stats[index].streak + 1 } else { 0 };
            }
        }
        stats
    }

    /// The poll open at `time`, or else the last one opened. A poll that
    /// opens exactly at `time` is not open yet: that instant is also the end
    /// of the segment before it, where a presenter still stands on the last
    /// poll's stop (the two times may differ by rounding, hence the margin).
    pub fn latest_at(&self, time: f64) -> Option<usize> {
        (0..self.polls.len())
            .filter(|index| self.polls[*index].open < time - 1e-6)
            .max_by(|a, b| self.polls[*a].open.total_cmp(&self.polls[*b].open))
    }

    /// What player `index` answered on poll `poll` by `time`.
    pub fn answer_of(
        &self,
        poll: usize,
        index: usize,
        time: f64,
    ) -> Option<crate::polls::PlayerAnswer> {
        let poll = &self.polls[poll];
        poll.votes
            .iter()
            .find(|vote| vote.player == index && vote.at <= time)
            .map(|vote| crate::polls::PlayerAnswer {
                options: vote.options,
                elapsed: poll.time.map_or(0.0, |seconds| {
                    seconds * (vote.at - poll.open) / (poll.due - poll.open).max(1e-9)
                }),
                points: vote.points,
                right: poll.time.map(|_| vote.points > 0),
            })
    }

    /// A made-up player as a live zone sees it.
    pub fn player(&self, index: usize) -> crate::live::Player {
        let player = &self.players[index];
        crate::live::Player {
            name: player.name.as_str().into(),
            character: gaanim_objects::character::catalog().character_from_seed(player.character),
            team: player.team,
            stats: Default::default(),
            answer: None,
        }
    }

    /// What the relay would report at timeline `time`. Marked live, as the
    /// values a scene shows.
    pub fn results_at(&self, time: f64) -> PollResults {
        let joined: Vec<usize> = (0..self.players.len())
            .filter(|index| self.players[*index].joined <= time)
            .collect();
        let scores = self.scores(time);
        // Best first; ties go to more correct answers, then the name, as
        // the relay ranks.
        let mut leaderboard: Vec<usize> = joined.clone();
        leaderboard.sort_by(|a, b| {
            scores[*b]
                .0
                .cmp(&scores[*a].0)
                .then(scores[*b].1.cmp(&scores[*a].1))
                .then_with(|| self.players[*a].name.cmp(&self.players[*b].name))
        });
        let mut counts = HashMap::new();
        let mut remaining = HashMap::new();
        let mut respondents = HashMap::new();
        let mut answers = HashMap::new();
        for (number, poll) in self.polls.iter().enumerate() {
            let mut tally = vec![0u32; poll.answers];
            let mut answered = HashMap::new();
            for vote in poll.votes.iter().filter(|vote| vote.at <= time) {
                for (answer, count) in tally.iter_mut().enumerate() {
                    *count += u32::from(vote.options & (1 << answer) != 0);
                }
                if let Some(answer) = self.answer_of(number, vote.player, time) {
                    answered.insert(Arc::from(self.players[vote.player].name.as_str()), answer);
                }
            }
            let id: Arc<str> = poll.id.as_str().into();
            respondents.insert(id.clone(), answered.len() as u32);
            answers.insert(id.clone(), answered);
            counts.insert(id.clone(), tally);
            if let Some(seconds) = poll.time {
                let window = poll.due - poll.open;
                let left = if time <= poll.open {
                    seconds
                } else if window > 0.0 && time < poll.due {
                    seconds * (1.0 - (time - poll.open) / window)
                } else {
                    0.0
                };
                remaining.insert(id, left);
            }
        }
        let catalog = gaanim_objects::character::catalog();
        let stats = self.stats_at(time);
        let mut teams = vec![crate::polls::TeamResult::default(); self.teams];
        for index in &joined {
            if let Some(team) = teams.get_mut(self.players[*index].team) {
                team.score += scores[*index].0;
                team.players += 1;
            }
        }
        PollResults {
            stats: joined
                .iter()
                .map(|index| (self.players[*index].name.as_str().into(), stats[*index]))
                .collect(),
            answers,
            respondents,
            latest: self
                .latest_at(time)
                .map(|index| self.polls[index].id.as_str().into()),
            teams,
            player_teams: if self.teams == 0 {
                HashMap::new()
            } else {
                joined
                    .iter()
                    .map(|index| {
                        let player = &self.players[*index];
                        (player.name.as_str().into(), player.team)
                    })
                    .collect()
            },
            live: true,
            counts,
            remaining,
            leaderboard: leaderboard
                .iter()
                .map(|index| (self.players[*index].name.as_str().into(), scores[*index].0))
                .collect(),
            players: joined.len() as u32,
            connected: joined.len() as u32,
            audience: joined
                .iter()
                .map(|index| {
                    let player = &self.players[*index];
                    (
                        player.name.as_str().into(),
                        (time - player.joined).min(AUDIENCE_AGE_CAP),
                    )
                })
                .collect(),
            avatars: joined
                .iter()
                .map(|index| {
                    let player = &self.players[*index];
                    (
                        player.name.as_str().into(),
                        catalog.character_from_seed(player.character),
                    )
                })
                .collect(),
        }
    }
}

/// What the rehearsal shows at the timeline's time, while no presentation
/// is live.
#[derive(bevy::prelude::Resource, Debug, Clone, Default)]
pub struct RehearsedResults {
    /// The timeline time `results` are for.
    pub time: Option<f64>,
    pub results: PollResults,
}

/// Replay the rehearsal at the timeline's time, unless a presentation is
/// live.
pub fn rehearsal_system(
    rehearsal: Option<bevy::prelude::Res<Rehearsal>>,
    live: Option<bevy::prelude::Res<PollResults>>,
    playback: Option<bevy::prelude::Res<crate::PlaybackState>>,
    mut shown: bevy::prelude::ResMut<RehearsedResults>,
) {
    let live = live.is_some_and(|results| results.live);
    let Some(rehearsal) = rehearsal.filter(|_| !live) else {
        if shown.time.is_some() {
            *shown = RehearsedResults::default();
        }
        return;
    };
    let time = playback.map_or(0.0, |playback| playback.current_time);
    if shown.time != Some(time) || rehearsal.is_changed() {
        *shown = RehearsedResults {
            time: Some(time),
            results: rehearsal.results_at(time),
        };
    }
}

/// The results a scene shows: a live presentation's, else the rehearsal's,
/// else none (`empty`).
pub fn shown_results<'a>(
    live: Option<&'a PollResults>,
    rehearsed: Option<&'a RehearsedResults>,
    empty: &'a PollResults,
) -> &'a PollResults {
    match (live, rehearsed) {
        (Some(live), _) if live.live => live,
        (_, Some(rehearsed)) if rehearsed.time.is_some() => &rehearsed.results,
        (Some(live), _) => live,
        _ => empty,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quiz(id: &str, open: f64, due: f64, lean: Lean) -> PlannedPoll {
        PlannedPoll {
            id: id.into(),
            answers: 4,
            open,
            due,
            quiz: Some((1 << 1, 20, 1000)),
            multiple: false,
            lean,
        }
    }

    #[test]
    fn players_join_in_order_over_the_room_time() {
        let spec = RehearsalSpec {
            names: RehearsalSpec::names(10),
            ..Default::default()
        };
        let rehearsal = Rehearsal::plan(&spec, Some((2.0, 12.0)), &[], None);
        let joined: Vec<f64> = rehearsal
            .players
            .iter()
            .map(|player| player.joined)
            .collect();
        assert!(joined.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(joined[0] > 2.0 && joined[9] < 12.0);
        assert_eq!(rehearsal.results_at(2.0).players, 0);
        assert_eq!(rehearsal.results_at(12.0).players, 10);
        let middle = rehearsal.results_at(7.0);
        assert!((3..=7).contains(&middle.players));
        assert_eq!(middle.audience[0].0.as_ref(), "Ana");
        assert_eq!(middle.avatars.len(), middle.players as usize);
        // Without a room, everyone joined long ago.
        let early = Rehearsal::plan(&spec, None, &[], None).results_at(0.0);
        assert_eq!(early.players, 10);
        assert!(
            early
                .audience
                .iter()
                .all(|(_, age)| *age == AUDIENCE_AGE_CAP)
        );
    }

    #[test]
    fn answers_arrive_until_the_stop_and_score_like_the_relay() {
        let spec = RehearsalSpec {
            names: RehearsalSpec::names(24),
            ..Default::default()
        };
        let polls = [quiz("q1", 20.0, 24.0, Lean::Auto)];
        let rehearsal = Rehearsal::plan(&spec, None, &polls, None);
        let total = |time: f64| -> u32 { rehearsal.results_at(time).counts["q1"].iter().sum() };
        assert_eq!(total(20.0), 0);
        assert!(total(22.0) > 0 && total(22.0) < 24);
        assert_eq!(total(24.0), 24);
        let at_stop = rehearsal.results_at(24.0);
        assert_eq!(at_stop.remaining["q1"], 0.0);
        assert_eq!(rehearsal.results_at(22.0).remaining["q1"], 10.0);
        assert_eq!(rehearsal.results_at(10.0).remaining["q1"], 20.0);
        // The best got the correct answer fast; nobody gets more than all.
        let (_, best) = &at_stop.leaderboard[0];
        assert!(*best > 500 && *best <= 1000);
        assert!(
            at_stop
                .leaderboard
                .windows(2)
                .all(|pair| pair[0].1 >= pair[1].1)
        );
    }

    #[test]
    fn a_lean_shapes_the_answers_and_the_seed_changes_them() {
        let spec = RehearsalSpec {
            names: RehearsalSpec::names(100),
            ..Default::default()
        };
        let easy = Rehearsal::plan(&spec, None, &[quiz("q", 0.0, 5.0, Lean::Right(0.95))], None);
        let hard = Rehearsal::plan(&spec, None, &[quiz("q", 0.0, 5.0, Lean::Right(0.1))], None);
        let right = |rehearsal: &Rehearsal| rehearsal.results_at(5.0).counts["q"][1];
        assert!(right(&easy) > 80, "{}", right(&easy));
        assert!(right(&hard) < 30, "{}", right(&hard));
        let poll = PlannedPoll {
            id: "p".into(),
            answers: 3,
            open: 0.0,
            due: 5.0,
            quiz: None,
            multiple: false,
            lean: Lean::Weights(vec![0.0, 1.0, 3.0]),
        };
        let counts = &Rehearsal::plan(&spec, None, std::slice::from_ref(&poll), None)
            .results_at(5.0)
            .counts["p"];
        assert_eq!(counts[0], 0);
        assert!(counts[2] > counts[1]);
        // The same spec plans the same session; another seed, another one.
        assert_eq!(
            Rehearsal::plan(&spec, None, std::slice::from_ref(&poll), None),
            Rehearsal::plan(&spec, None, std::slice::from_ref(&poll), None)
        );
        let other = RehearsalSpec {
            seed: 9,
            ..spec.clone()
        };
        assert_ne!(
            Rehearsal::plan(&spec, None, std::slice::from_ref(&poll), None),
            Rehearsal::plan(&other, None, &[poll], None)
        );
    }

    #[test]
    fn teams_are_dealt_evenly_and_score_what_their_players_earn() {
        let spec = RehearsalSpec {
            names: RehearsalSpec::names(9),
            team_skill: vec![0.95, 0.05],
            ..Default::default()
        };
        let teams = Some(RehearsalTeams {
            count: 2,
            choose: false,
        });
        let polls = [quiz("q", 0.0, 5.0, Lean::Auto)];
        let rehearsal = Rehearsal::plan(&spec, None, &polls, teams);
        let dealt: Vec<usize> = rehearsal.players.iter().map(|player| player.team).collect();
        assert_eq!(dealt, [0, 1, 0, 1, 0, 1, 0, 1, 0]);
        let results = rehearsal.results_at(5.0);
        assert_eq!(results.teams.len(), 2);
        assert_eq!((results.teams[0].players, results.teams[1].players), (5, 4));
        let total: u64 = results.leaderboard.iter().map(|(_, score)| score).sum();
        assert_eq!(results.teams[0].score + results.teams[1].score, total);
        // The skilled team wins.
        assert!(results.teams[0].score > results.teams[1].score);
        assert_eq!(results.leading_team(), 0);
        assert_eq!(results.player_teams["Beto"], 1);
        // A question's own share keeps the skilled team ahead.
        let spec = RehearsalSpec {
            names: RehearsalSpec::names(40),
            skill: 0.6,
            team_skill: vec![0.8, 0.4],
            ..Default::default()
        };
        let polls = [quiz("q", 0.0, 5.0, Lean::Right(0.6))];
        let results = Rehearsal::plan(&spec, None, &polls, teams).results_at(5.0);
        assert!(results.teams[0].score > results.teams[1].score);
    }

    #[test]
    fn multiple_choice_answers_streaks_and_who_answered_what() {
        let spec = RehearsalSpec {
            names: RehearsalSpec::names(6),
            ..Default::default()
        };
        let several = PlannedPoll {
            id: "m".into(),
            answers: 4,
            open: 0.0,
            due: 4.0,
            quiz: Some((0b1011, 20, 1000)),
            multiple: true,
            lean: Lean::Right(1.0),
        };
        let after = PlannedPoll {
            id: "s".into(),
            open: 5.0,
            due: 9.0,
            ..quiz("s", 5.0, 9.0, Lean::Right(1.0))
        };
        let rehearsal = Rehearsal::plan(&spec, None, &[several, after], None);
        let results = rehearsal.results_at(9.0);
        // Everyone chose all three right answers, and nothing else.
        assert_eq!(results.counts["m"], [6, 6, 0, 6]);
        assert_eq!(results.respondents["m"], 6);
        let ana = results.answers["m"]["Ana"];
        assert_eq!((ana.options, ana.right), (0b1011, Some(true)));
        assert!(ana.points > 0 && ana.elapsed > 0.0 && ana.elapsed <= 20.0);
        assert_eq!(results.stats["Ana"].streak, 2);
        assert_eq!(results.stats["Ana"].correct, 2);
        assert_eq!(results.latest.as_deref(), Some("s"));
        // Before the second quiz is over, the run is one long.
        assert_eq!(rehearsal.results_at(4.5).stats["Ana"].streak, 1);
        assert_eq!(rehearsal.results_at(4.5).latest.as_deref(), Some("m"));
        // At the instant the next poll opens, the stop before it still shows
        // the last one's answers.
        assert_eq!(rehearsal.results_at(5.0).latest.as_deref(), Some("m"));
        assert_eq!(rehearsal.latest_at(5.0), Some(0));
        assert!(rehearsal.answer_of(0, 0, 5.0).is_some());
    }

    #[test]
    fn many_players_reuse_names_with_a_number() {
        let names = RehearsalSpec::names(32);
        assert_eq!(names[29], "Emma");
        assert_eq!(names[30], "Ana 2");
    }
}
