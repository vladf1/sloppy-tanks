//! One seat's input on the host (`src/net/player-controls.ts`), and the wire form of the
//! client's `input` message.
//!
//! Each input drives from a well-defined tick: the tick the client asked for, or the
//! tick after its arrival if that came later. Inputs waiting for their tick queue in
//! order, and the newest one due wins. Held input lasts a 250 ms lease; one-shot actions
//! (mines, ammo choices) run once, in order, each within its own lease. A control epoch changes on every death, respawn,
//! suspension and reconnect, and input from an older epoch is refused, so stale held
//! input or queued clicks never carry into a new life.

use std::collections::VecDeque;

use serde_json::Value;

use super::fixed_step_clock::SIMULATION_STEP_MS;
use super::json::{self, ObjectWriter};
use super::multiplayer_simulation::set_driver;
use super::schema::MAX_SAFE_INTEGER;
use crate::sim::ammunition::AMMO_ORDER;
use crate::sim::simulation::Simulation;
use crate::sim::types::{AmmoSelection, Driver, VehicleCommand, Weapon};

pub const INPUT_LEASE_MS: f64 = 250.0;
pub const BOT_TAKEOVER_MS: f64 = 5000.0;
pub const MAX_QUEUED_ACTIONS: usize = 8;
pub const MAX_INPUT_LAG_TICKS: u64 = 30;
/// Furthest past its arrival an input may ask to start driving (half a second).
pub const MAX_INPUT_LEAD_TICKS: u64 = 30;
/// Most inputs waiting for their tick: a second of input at the rate limit.
const MAX_WAITING_INPUTS: usize = MAX_INPUTS_PER_SECOND as usize;
const MAX_AIM_COORDINATE: f64 = 1024.0;
const MAX_INPUTS_PER_SECOND: u32 = 60;

/// A one-shot action queued with held input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Mine,
    Ammo(Weapon),
}

/// Where the turret points: a world point (recomputed from the authoritative hull) or an
/// absolute angle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Aim {
    Point { x: f64, z: f64 },
    Angle(f64),
}

/// One `input` message's control fields.
#[derive(Clone, Debug, PartialEq)]
pub struct ControlInput {
    pub control_epoch: u64,
    pub seq: i64,
    pub observed_tick: i64,
    /// The tick the client predicted this input from; it drives no earlier than arrival.
    pub tick: Option<u64>,
    pub move_x: f64,
    pub move_z: f64,
    pub aim: Aim,
    pub fire: bool,
    pub actions: Vec<Action>,
}

impl ControlInput {
    /// Whether the tank visibly stops when this input's lease runs out. Aim alone does
    /// not: an idle command keeps the turret where it points.
    fn drives(&self) -> bool {
        self.move_x != 0.0 || self.move_z != 0.0 || self.fire
    }
}

fn write_action(out: &mut String, action: Action) {
    let mut writer = ObjectWriter::new(out);
    match action {
        Action::Mine => {
            writer.string("type", "mine");
        }
        Action::Ammo(weapon) => {
            writer
                .string("type", "ammo")
                .string("weapon", weapon.as_str());
        }
    }
    writer.finish();
}

/// The `input` message for `round_id`: rounded like snapshots, with idle defaults
/// (`fire: false`, no actions) omitted, in the browser's key order.
pub fn encode_input(input: &ControlInput, round_id: u64) -> String {
    json::object(|writer| {
        writer
            .string("type", "input")
            .int("roundId", round_id)
            .int("controlEpoch", input.control_epoch)
            .number("moveX", json::value(input.move_x))
            .number("moveZ", json::value(input.move_z))
            .number("seq", input.seq as f64)
            .number("observedTick", input.observed_tick as f64);
        if let Some(tick) = input.tick {
            writer.int("tick", tick);
        }
        writer.nested("aim", |aim| match input.aim {
            Aim::Angle(angle) => {
                let angle =
                    json::rotation(angle).clamp(-std::f64::consts::PI, std::f64::consts::PI);
                aim.number("angle", angle);
            }
            Aim::Point { x, z } => {
                aim.number("x", json::position(x))
                    .number("z", json::position(z));
            }
        });
        if input.fire {
            writer.boolean("fire", true);
        }
        if !input.actions.is_empty() {
            let list = writer.key("actions");
            list.push('[');
            for (index, action) in input.actions.iter().enumerate() {
                if index > 0 {
                    list.push(',');
                }
                write_action(list, *action);
            }
            list.push(']');
        }
    })
}

fn finite(value: Option<&Value>, bound: f64) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|number| number.is_finite() && number.abs() <= bound)
}

fn safe_integer(value: Option<&Value>) -> Option<i64> {
    value
        .and_then(Value::as_f64)
        .filter(|number| number.trunc() == *number && number.abs() <= MAX_SAFE_INTEGER)
        .map(|number| number as i64)
}

/// `validInput`: omitted (or `null`) fire and actions mean "not firing" and "no actions".
pub fn read_input(value: &Value) -> Option<ControlInput> {
    let message = value.as_object()?;
    let aim = message.get("aim")?.as_object()?;
    let actions = match message.get("actions") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(items)) if items.len() <= MAX_QUEUED_ACTIONS => items
            .iter()
            .map(|item| {
                let action = item.as_object()?;
                match action.get("type").and_then(Value::as_str) {
                    Some("mine") if action.len() == 1 => Some(Action::Mine),
                    Some("ammo") if action.len() == 2 => {
                        let weapon = action.get("weapon").and_then(Value::as_str)?;
                        // Players select from AMMO_ORDER; the TOW is bot-only.
                        AMMO_ORDER
                            .into_iter()
                            .find(|candidate| candidate.as_str() == weapon)
                            .map(Action::Ammo)
                    }
                    _ => None,
                }
            })
            .collect::<Option<Vec<_>>>()?,
        Some(_) => return None,
    };
    let fire = match message.get("fire") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(fire)) => *fire,
        Some(_) => return None,
    };
    let aim = if aim.len() == 1 {
        Aim::Angle(finite(aim.get("angle"), std::f64::consts::PI)?)
    } else if aim.len() == 2 {
        Aim::Point {
            x: finite(aim.get("x"), MAX_AIM_COORDINATE)?,
            z: finite(aim.get("z"), MAX_AIM_COORDINATE)?,
        }
    } else {
        return None;
    };
    let control_epoch = safe_integer(message.get("controlEpoch"))?;
    let tick = match message.get("tick") {
        None | Some(Value::Null) => None,
        some => Some(u64::try_from(safe_integer(some)?).ok()?),
    };
    Some(ControlInput {
        // A negative epoch never matches; keep it distinguishable from every real one.
        control_epoch: u64::try_from(control_epoch).unwrap_or(u64::MAX),
        seq: safe_integer(message.get("seq"))?,
        observed_tick: safe_integer(message.get("observedTick"))?,
        tick,
        move_x: finite(message.get("moveX"), 1.0)?,
        move_z: finite(message.get("moveZ"), 1.0)?,
        aim,
        fire,
        actions,
    })
}

/// The latest input sequence applied to the simulation, the tick it first drove, and
/// the first tick it could have driven when it arrived. Client prediction replays from
/// the first and times its requests by the second.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ack {
    pub input_seq: i64,
    pub applied_tick: u64,
    pub arrival_tick: u64,
}

/// An accepted input waiting for its tick.
#[derive(Clone, Debug)]
struct WaitingInput {
    input: ControlInput,
    tick: u64,
    arrival_tick: u64,
    received_ms: f64,
}

/// One assigned seat, independent of sockets and wall-clock APIs. Room ownership is
/// checked by the host. Refers to its tank by id; a missing tank reads as gone.
#[derive(Clone, Debug)]
pub struct PlayerControls {
    pub tank_id: u32,
    pub control_epoch: u64,
    pub ack: Ack,
    life: u32,
    alive: bool,
    input: Option<ControlInput>,
    /// The arrival tick of `input`.
    input_arrival: u64,
    waiting: VecDeque<WaitingInput>,
    last_seq: i64,
    last_received_ms: f64,
    /// When the held input's lease started: its receipt, plus any ticks it waited for.
    lease_from_ms: f64,
    rate_window_ms: f64,
    received_in_window: u32,
    actions: VecDeque<(Action, f64)>,
    suspended: bool,
    bot_takeover: bool,
    /// Input lapses not yet collected by [`take_lapses`](Self::take_lapses).
    lapses: u64,
    /// The current held input already lapsed; fresh input re-arms the count.
    lapsed: bool,
}

impl PlayerControls {
    /// Controls for the player-owned tank `tank_id`. With `bot_takeover` off (humans-only
    /// rooms), a silent or suspended seat idles instead of handing its tank to a bot.
    pub fn new(
        simulation: &Simulation,
        tank_id: u32,
        now_ms: f64,
        bot_takeover: bool,
    ) -> Result<Self, String> {
        let tank = simulation
            .tank_index(tank_id)
            .map(|index| &simulation.tanks[index])
            .filter(|tank| tank.human)
            .ok_or("Controls require a player-owned tank")?;
        Ok(Self {
            tank_id,
            control_epoch: 1,
            ack: Ack::default(),
            life: tank.life,
            alive: tank.alive,
            input: None,
            input_arrival: 0,
            waiting: VecDeque::new(),
            last_seq: 0,
            last_received_ms: now_ms,
            lease_from_ms: now_ms,
            rate_window_ms: now_ms,
            received_in_window: 0,
            actions: VecDeque::new(),
            suspended: false,
            bot_takeover,
            lapses: 0,
            lapsed: false,
        })
    }

    /// Input lapses since the previous call: held movement or fire whose lease ran out
    /// before fresh input arrived, so the tank stopped while the player still held the
    /// controls. A stalled upload (TCP head-of-line blocking) or a frozen page causes it;
    /// suspension and disconnects do not count.
    pub fn take_lapses(&mut self) -> u64 {
        std::mem::take(&mut self.lapses)
    }

    /// Accepts one `input` message that arrived after tick `server_tick` was stepped, to
    /// drive from the next tick at the earliest. See [`accept_at`](Self::accept_at).
    pub fn accept(
        &mut self,
        simulation: &Simulation,
        value: &Value,
        server_tick: u64,
        now_ms: f64,
    ) -> bool {
        self.accept_at(simulation, value, server_tick, server_tick + 1, now_ms)
    }

    /// Accepts one `input` message whose earliest tick is `arrival_tick`. Returns false
    /// without partially applying a malformed or stale message, or extending its lease.
    pub fn accept_at(
        &mut self,
        simulation: &Simulation,
        value: &Value,
        server_tick: u64,
        arrival_tick: u64,
        now_ms: f64,
    ) -> bool {
        self.refresh_life(simulation);
        let Some(index) = simulation.tank_index(self.tank_id) else {
            return false;
        };
        let tank = &simulation.tanks[index];
        if self.suspended || !tank.alive || tank.driver != Driver::Human {
            return false;
        }
        if now_ms - self.rate_window_ms >= 1000.0 {
            self.rate_window_ms = now_ms;
            self.received_in_window = 0;
        }
        self.received_in_window += 1;
        if self.received_in_window > MAX_INPUTS_PER_SECOND {
            return false;
        }
        self.expire_actions(now_ms);
        let Some(input) = read_input(value) else {
            return false;
        };
        let server_tick = server_tick as i64;
        let queued_actions = self.actions.len()
            + self
                .waiting
                .iter()
                .map(|waiting| waiting.input.actions.len())
                .sum::<usize>();
        if input.control_epoch != self.control_epoch
            || input.seq <= self.last_seq
            || input.observed_tick < 0
            || input.observed_tick > server_tick
            || server_tick - input.observed_tick > MAX_INPUT_LAG_TICKS as i64
            || queued_actions + input.actions.len() > MAX_QUEUED_ACTIONS
            || self.waiting.len() >= MAX_WAITING_INPUTS
        {
            return false;
        }
        // Inputs drive in sequence order, so none may start before the one queued ahead.
        let after = self
            .waiting
            .back()
            .map_or(arrival_tick, |waiting| waiting.tick);
        let tick = input
            .tick
            .unwrap_or(arrival_tick)
            .clamp(arrival_tick, arrival_tick + MAX_INPUT_LEAD_TICKS)
            .max(after);
        self.last_seq = input.seq;
        self.last_received_ms = now_ms;
        self.lease_from_ms = now_ms;
        self.waiting.push_back(WaitingInput {
            input,
            tick,
            arrival_tick,
            received_ms: now_ms,
        });
        self.lapsed = false;
        true
    }

    /// This tick's command for the seat: `None` while a bot drives or the seat is
    /// suspended. Held input idles once its lease runs out; silence past
    /// [`BOT_TAKEOVER_MS`] suspends the seat.
    pub fn command(
        &mut self,
        simulation: &mut Simulation,
        tick: u64,
        now_ms: f64,
    ) -> Option<VehicleCommand> {
        self.refresh_life(simulation);
        if !self.suspended && now_ms - self.last_received_ms >= BOT_TAKEOVER_MS {
            self.suspend(simulation);
        }
        let index = simulation.tank_index(self.tank_id)?;
        let tank = &simulation.tanks[index];
        if self.suspended || tank.driver == Driver::Bot {
            return None;
        }
        let mut command = VehicleCommand::idle_aiming(tank.aim);
        if !tank.alive {
            return Some(command);
        }
        // The newest input due by this tick takes over; any it overtook never drives.
        while self
            .waiting
            .front()
            .is_some_and(|waiting| waiting.tick <= tick)
        {
            let waiting = self.waiting.pop_front().expect("checked above");
            for action in &waiting.input.actions {
                self.actions.push_back((*action, waiting.received_ms));
            }
            // An input asked to wait keeps its whole lease for when it starts driving.
            let wait_ms = (waiting.tick - waiting.arrival_tick) as f64 * SIMULATION_STEP_MS;
            self.lease_from_ms = self.lease_from_ms.max(waiting.received_ms + wait_ms);
            self.input_arrival = waiting.arrival_tick;
            self.input = Some(waiting.input);
        }
        self.expire_actions(now_ms);
        let leased = now_ms - self.lease_from_ms < INPUT_LEASE_MS;
        if !leased && !self.lapsed && self.input.as_ref().is_some_and(ControlInput::drives) {
            self.lapsed = true;
            self.lapses += 1;
        }
        let held = self
            .input
            .as_ref()
            .filter(|_| leased)
            .map(|input| (input.move_x, input.move_z, input.fire, input.aim, input.seq));
        let Some((move_x, move_z, fire, aim, seq)) = held else {
            self.actions.clear();
            return Some(command);
        };
        let origin = simulation.body_translation(tank.body);
        command.move_x = move_x;
        command.move_z = move_z;
        command.fire = fire;
        command.aim = match aim {
            Aim::Angle(angle) => angle,
            Aim::Point { x, z } => (x - origin.x).atan2(z - origin.z),
        };
        if self.ack.input_seq != seq {
            self.ack = Ack {
                input_seq: seq,
                applied_tick: tick,
                arrival_tick: self.input_arrival,
            };
        }
        match self.actions.pop_front().map(|(action, _)| action) {
            Some(Action::Mine) => command.mine = true,
            Some(Action::Ammo(weapon)) => {
                command.ammo_selection = Some(AmmoSelection::Weapon(weapon));
            }
            None => {}
        }
        Some(command)
    }

    /// Hands the tank to a bot (or idles it) and starts a new control epoch.
    pub fn suspend(&mut self, simulation: &mut Simulation) {
        if self.suspended {
            return;
        }
        self.suspended = true;
        if let Some(index) = simulation.tank_index(self.tank_id) {
            let driver = if self.bot_takeover {
                Driver::Bot
            } else {
                Driver::Idle
            };
            set_driver(simulation, index, driver).expect("any tank can be driven by a bot");
        }
        self.new_epoch();
    }

    /// Gives the tank back to the player under a new control epoch.
    pub fn resume(&mut self, simulation: &mut Simulation, now_ms: f64) {
        self.refresh_life(simulation);
        self.suspended = false;
        if let Some(index) = simulation.tank_index(self.tank_id) {
            set_driver(simulation, index, Driver::Human).expect("controls drive a player tank");
        }
        self.last_received_ms = now_ms;
        self.lease_from_ms = now_ms;
        self.new_epoch();
    }

    /// Picks up post-step deaths and respawns before the next control epoch is published.
    pub fn refresh_life(&mut self, simulation: &Simulation) {
        let Some(index) = simulation.tank_index(self.tank_id) else {
            return;
        };
        let tank = &simulation.tanks[index];
        if self.life == tank.life && self.alive == tank.alive {
            return;
        }
        self.life = tank.life;
        self.alive = tank.alive;
        self.new_epoch();
    }

    fn new_epoch(&mut self) {
        self.control_epoch += 1;
        self.input = None;
        self.waiting.clear();
        self.actions.clear();
        self.last_seq = 0;
        self.ack = Ack::default();
    }

    fn expire_actions(&mut self, now_ms: f64) {
        self.actions
            .retain(|(_, received)| now_ms - received < INPUT_LEASE_MS);
    }
}
