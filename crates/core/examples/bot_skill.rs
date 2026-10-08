//! Bot skill in seeded bots-only team matches, per bot role:
//! `cargo run --release -p sloppy-core --example bot_skill -- [options]`.
//!
//! Options: `--maps village,harbor,quarry` (standard maps by default), `--seeds 8` (rounds
//! per map, each seeded `n * SEED_STRIDE`), `--out <path>` (default
//! `artifacts/performance/bot-skill.json`) and `--baseline <path>`, an earlier output to
//! print differences against. Every tank plays with the bot brain; the autoplayed human
//! slot is left out of the roles. Manual evidence for AI changes, not a CI gate: an AI
//! change alters every seeded match, so compare means over many rounds, never one match.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use serde_json::{Value, json};
use sloppy_core::sim::bot_personalities::BotPersonality;
use sloppy_core::sim::data::STEP;
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::{
    BotMode, MatchPhase, SimEventType, Simulation, SimulationSetup, VehicleCommand, VehicleKind,
};

const DEFAULT_MAPS: [MapId; 3] = [MapId::Village, MapId::Harbor, MapId::Quarry];
const DEFAULT_SEEDS: u32 = 8;
const SEED_STRIDE: f64 = 79.0;
/// A round stops after six simulated minutes even without a result.
const MAX_STEPS: u32 = 60 * 360;
/// A bot this long without progress counts as stalled; stuck recovery starts at 1.2 s.
const STALL_SECONDS: f64 = 0.5;
/// A hit from an enemy the victim was not targeting is answered once the victim targets
/// that attacker within this window. A victim tracks one unanswered attacker at a time, and
/// a hit whose victim or attacker dies first counts as neither answered nor unanswered.
const ANSWER_WINDOW_SECONDS: f64 = 5.0;

/// Metrics in report order: (key, label).
const METRICS: [(&str, &str); 10] = [
    ("accuracy", "enemy hits per shot"),
    ("damagePerShot", "hull damage per shot"),
    ("killsPerMinute", "kills per minute alive"),
    ("deathsPerMinute", "deaths per minute alive"),
    ("engaged", "share of time fighting"),
    ("stalled", "share of time stalled"),
    ("recoveriesPerMinute", "stuck recoveries per minute alive"),
    ("blindsided", "share of hits from an untargeted enemy"),
    ("answered", "share of those hits answered"),
    ("answerSeconds", "seconds to answer"),
];

/// One bot's counts over a round.
#[derive(Default)]
struct Tally {
    shots: f64,
    enemy_hits: f64,
    damage_dealt: f64,
    kills: f64,
    deaths: f64,
    alive_ticks: f64,
    fight_ticks: f64,
    stalled_ticks: f64,
    recoveries: f64,
    hits_taken: f64,
    blindsided: f64,
    answered: f64,
    unanswered: f64,
    answer_seconds: f64,
}

impl Tally {
    fn add(&mut self, other: &Tally) {
        self.shots += other.shots;
        self.enemy_hits += other.enemy_hits;
        self.damage_dealt += other.damage_dealt;
        self.kills += other.kills;
        self.deaths += other.deaths;
        self.alive_ticks += other.alive_ticks;
        self.fight_ticks += other.fight_ticks;
        self.stalled_ticks += other.stalled_ticks;
        self.recoveries += other.recoveries;
        self.hits_taken += other.hits_taken;
        self.blindsided += other.blindsided;
        self.answered += other.answered;
        self.unanswered += other.unanswered;
        self.answer_seconds += other.answer_seconds;
    }

    /// Ratios for one round; a metric without a denominator is left out.
    fn metrics(&self) -> BTreeMap<&'static str, f64> {
        let minutes = self.alive_ticks * STEP / 60.0;
        let ratio = |a: f64, b: f64| (b > 0.0).then_some(a / b);
        [
            ("accuracy", ratio(self.enemy_hits, self.shots)),
            ("damagePerShot", ratio(self.damage_dealt, self.shots)),
            ("killsPerMinute", ratio(self.kills, minutes)),
            ("deathsPerMinute", ratio(self.deaths, minutes)),
            ("engaged", ratio(self.fight_ticks, self.alive_ticks)),
            ("stalled", ratio(self.stalled_ticks, self.alive_ticks)),
            ("recoveriesPerMinute", ratio(self.recoveries, minutes)),
            ("blindsided", ratio(self.blindsided, self.hits_taken)),
            (
                "answered",
                ratio(self.answered, self.answered + self.unanswered),
            ),
            ("answerSeconds", ratio(self.answer_seconds, self.answered)),
        ]
        .into_iter()
        .filter_map(|(key, value)| value.map(|value| (key, value)))
        .collect()
    }
}

struct RoundResult {
    map: MapId,
    seed: f64,
    seconds: f64,
    scores: Value,
    breach_shots: u32,
    roles: BTreeMap<String, Tally>,
}

fn role(personality: BotPersonality, kind: VehicleKind) -> String {
    if kind == VehicleKind::Humvee {
        format!("{} (humvee)", personality.as_str())
    } else {
        personality.as_str().to_string()
    }
}

fn play_round(map: MapId, seed: f64) -> RoundResult {
    let setup = SimulationSetup {
        map_mode: Some(map),
        ..SimulationSetup::default()
    };
    let mut simulation = Simulation::new(seed, setup);
    simulation.start();
    let human = simulation.human().id;
    let mut tallies: HashMap<u32, Tally> = HashMap::new();
    let mut recoveries: HashMap<u32, u32> = HashMap::new();
    // Victim id -> (attacker id, elapsed at the hit).
    let mut pending: HashMap<u32, (u32, f64)> = HashMap::new();
    let mut steps = 0;
    while simulation.match_state.phase == MatchPhase::Playing && steps < MAX_STEPS {
        simulation.step(VehicleCommand::idle(), true);
        steps += 1;
        let elapsed = simulation.elapsed;
        let team_of = |simulation: &Simulation, id: u32| {
            simulation.tank_index(id).map(|i| simulation.tanks[i].team)
        };
        for event in std::mem::take(&mut simulation.events) {
            let (Some(id), owner) = (event.id, event.owner) else {
                continue;
            };
            match event.kind {
                SimEventType::Shot if id != human => tallies.entry(id).or_default().shots += 1.0,
                SimEventType::Hurt | SimEventType::Death => {
                    let Some(owner) = owner else { continue };
                    let (Some(victim_team), Some(attacker_team)) =
                        (team_of(&simulation, id), team_of(&simulation, owner))
                    else {
                        continue;
                    };
                    if victim_team == attacker_team {
                        continue;
                    }
                    if owner != human {
                        tallies.entry(owner).or_default().enemy_hits += 1.0;
                    }
                    if id == human {
                        continue;
                    }
                    let tally = tallies.entry(id).or_default();
                    tally.hits_taken += 1.0;
                    let victim = &simulation.tanks[simulation.tank_index(id).expect("victim")];
                    if victim.brain.target != owner {
                        tally.blindsided += 1.0;
                        if event.kind == SimEventType::Hurt {
                            pending.entry(id).or_insert((owner, elapsed));
                        }
                    }
                }
                _ => {}
            }
        }
        pending.retain(|&victim_id, &mut (attacker_id, since)| {
            let victim = &simulation.tanks[simulation.tank_index(victim_id).expect("victim")];
            let attacker_alive = simulation
                .tank_index(attacker_id)
                .is_some_and(|i| simulation.tanks[i].alive);
            let tally = tallies.entry(victim_id).or_default();
            if victim.alive && victim.brain.target == attacker_id {
                tally.answered += 1.0;
                tally.answer_seconds += elapsed - since;
                false
            } else if !victim.alive || !attacker_alive {
                false
            } else if elapsed - since > ANSWER_WINDOW_SECONDS {
                tally.unanswered += 1.0;
                false
            } else {
                true
            }
        });
        for tank in &simulation.tanks {
            if tank.id == human || !tank.alive {
                continue;
            }
            let tally = tallies.entry(tank.id).or_default();
            tally.alive_ticks += 1.0;
            if tank.brain.mode == BotMode::Fight {
                tally.fight_ticks += 1.0;
            }
            if tank.brain.stuck >= STALL_SECONDS {
                tally.stalled_ticks += 1.0;
            }
            // The counter restarts with each life.
            let seen = recoveries.entry(tank.id).or_default();
            if tank.brain.recoveries > *seen {
                tally.recoveries += (tank.brain.recoveries - *seen) as f64;
            }
            *seen = tank.brain.recoveries;
        }
    }
    let mut roles: BTreeMap<String, Tally> = BTreeMap::new();
    for tank in simulation.tanks.iter().filter(|tank| tank.id != human) {
        let mut tally = tallies.remove(&tank.id).unwrap_or_default();
        tally.damage_dealt = tank.damage_dealt;
        tally.kills = tank.kills as f64;
        tally.deaths = tank.deaths as f64;
        for key in [role(tank.brain.personality, tank.kind), "all".to_string()] {
            roles.entry(key).or_default().add(&tally);
        }
    }
    RoundResult {
        map,
        seed,
        seconds: steps as f64 * STEP,
        scores: json!(simulation.match_state.scores),
        breach_shots: simulation.bot_breach_shots,
        roles,
    }
}

/// Mean and two standard errors over rounds.
fn mean_and_error(values: &[f64]) -> (f64, f64) {
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    if values.len() < 2 {
        return (mean, f64::NAN);
    }
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0);
    (mean, 2.0 * (variance / n).sqrt())
}

fn parse_args() -> (Vec<MapId>, u32, PathBuf, Option<PathBuf>) {
    let mut maps = DEFAULT_MAPS.to_vec();
    let mut seeds = DEFAULT_SEEDS;
    let mut out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../artifacts/performance/bot-skill.json");
    let mut baseline = None;
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .unwrap_or_else(|| panic!("{flag} needs a value"));
        match flag.as_str() {
            "--maps" => {
                maps = value
                    .split(',')
                    .map(|id| MapId::parse(id).unwrap_or_else(|| panic!("unknown map {id}")))
                    .collect();
            }
            "--seeds" => seeds = value.parse().expect("--seeds takes a count"),
            "--out" => out = PathBuf::from(value),
            "--baseline" => baseline = Some(PathBuf::from(value)),
            _ => panic!("unknown option {flag}"),
        }
    }
    (maps, seeds, out, baseline)
}

fn main() {
    let (maps, seeds, out, baseline) = parse_args();
    let started = Instant::now();
    let jobs: Vec<(MapId, f64)> = maps
        .iter()
        .flat_map(|&map| (1..=seeds).map(move |n| (map, n as f64 * SEED_STRIDE)))
        .collect();
    // Rounds are independent seeded simulations, so they run in parallel and finish in
    // any order; results are sorted back into job order.
    let next = AtomicUsize::new(0);
    let results = Mutex::new(Vec::new());
    let workers = std::thread::available_parallelism().map_or(4, |n| n.get());
    std::thread::scope(|scope| {
        for _ in 0..workers.min(jobs.len()) {
            scope.spawn(|| {
                loop {
                    let job = next.fetch_add(1, Ordering::Relaxed);
                    let Some(&(map, seed)) = jobs.get(job) else {
                        break;
                    };
                    let result = play_round(map, seed);
                    results.lock().expect("results").push((job, result));
                }
            });
        }
    });
    let mut results = results.into_inner().expect("results");
    results.sort_by_key(|(job, _)| *job);
    let rounds: Vec<RoundResult> = results.into_iter().map(|(_, result)| result).collect();

    let mut samples: BTreeMap<String, BTreeMap<&str, Vec<f64>>> = BTreeMap::new();
    for round in &rounds {
        for (role, tally) in &round.roles {
            for (metric, value) in tally.metrics() {
                samples
                    .entry(role.clone())
                    .or_default()
                    .entry(metric)
                    .or_default()
                    .push(value);
            }
        }
    }
    let summary: BTreeMap<&String, BTreeMap<&str, Value>> = samples
        .iter()
        .map(|(role, metrics)| {
            let metrics = metrics
                .iter()
                .map(|(&metric, values)| {
                    let (mean, error) = mean_and_error(values);
                    (
                        metric,
                        json!({ "mean": mean, "error": error, "rounds": values.len() }),
                    )
                })
                .collect();
            (role, metrics)
        })
        .collect();
    let result = json!({
        "type": "seeded bots-only team matches, accelerated simulation",
        "maps": maps.iter().map(|map| map.as_str()).collect::<Vec<_>>(),
        "seedsPerMap": seeds,
        "wallSeconds": started.elapsed().as_secs_f64(),
        "metrics": METRICS.iter().map(|(key, label)| json!({ "key": key, "label": label })).collect::<Vec<_>>(),
        "summary": summary,
        "rounds": rounds.iter().map(|round| json!({
            "map": round.map.as_str(),
            "seed": round.seed,
            "seconds": round.seconds,
            "scores": round.scores,
            "breachShots": round.breach_shots,
            "roles": round.roles.iter().map(|(role, tally)| (role.clone(), json!(tally.metrics()))).collect::<BTreeMap<_, _>>(),
        })).collect::<Vec<_>>(),
    });
    if let Some(directory) = out.parent() {
        std::fs::create_dir_all(directory).expect("create the output directory");
    }
    std::fs::write(
        &out,
        serde_json::to_string_pretty(&result).expect("serializable results"),
    )
    .expect("write the results");

    let baseline: Option<Value> = baseline.map(|path| {
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read the baseline"))
            .expect("baseline JSON")
    });
    println!(
        "{} rounds ({} maps x {seeds} seeds) in {:.1} s, written to {}",
        rounds.len(),
        maps.len(),
        started.elapsed().as_secs_f64(),
        out.display()
    );
    println!("Values are means over rounds ± two standard errors.");
    for (metric, label) in METRICS {
        println!("\n{metric}: {label}");
        for (role, metrics) in &summary {
            let Some(entry) = metrics.get(metric) else {
                continue;
            };
            let (mean, error) = (entry["mean"].as_f64(), entry["error"].as_f64());
            let (mean, error) = (mean.unwrap_or(f64::NAN), error.unwrap_or(f64::NAN));
            let base = baseline
                .as_ref()
                .and_then(|base| base["summary"][role.as_str()][metric].as_object().cloned());
            match base {
                Some(base) => {
                    let base_mean = base["mean"].as_f64().unwrap_or(f64::NAN);
                    let base_error = base["error"].as_f64().unwrap_or(f64::NAN);
                    println!(
                        "  {role:<18} {mean:>8.3} ± {error:<7.3} base {base_mean:>8.3}  diff {:>+8.3} ± {:.3}",
                        mean - base_mean,
                        error.hypot(base_error)
                    );
                }
                None => println!("  {role:<18} {mean:>8.3} ± {error:.3}"),
            }
        }
    }
}
