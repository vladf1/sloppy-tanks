//! What a pilot experienced, for the bots control page and the command line: its battle
//! record, and the room's network feel as the browser's statistics measure it (round
//! trip, prediction corrections, fire-to-flash time, view delay, where hits were drawn).
//! Run beside a person, these are what that person's seat would have felt from the
//! pilot's region.

use serde::Serialize;
use sloppy_core::net::client::NetworkStats;
use sloppy_core::net::feel_readings::{FeelReadings, Samples};
use sloppy_core::net::own_shots::OwnShotStats;

/// The battle as the pilot fought it, counted from the events it displayed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tally {
    /// Battles that started while the pilot was in the room.
    pub rounds: u32,
    pub wins: u32,
    pub losses: u32,
    /// Shells and blasts of the pilot's that hurt or killed an enemy, as the host judged.
    pub hits: u32,
    pub kills: u32,
    /// Damage the pilot took from anything, and the deaths it ended in.
    pub hits_taken: u32,
    pub deaths: u32,
    /// Of the hits, those between the pilot and players (people or other pilots), whose
    /// shells the host judges where their shooter saw the target.
    pub player_hits: u32,
    pub player_hits_taken: u32,
    /// Decisions to fire the brain made; the host fires at most once per reload.
    pub trigger_pulls: u32,
    /// Times the pilot changed sides to fight against, or beside, its player.
    pub team_changes: u32,
}

/// A mean, a 95th percentile and a count of a reading.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    pub mean: f64,
    pub p95: f64,
    pub count: u64,
}

impl From<&Samples> for Reading {
    fn from(samples: &Samples) -> Self {
        Self {
            mean: round(samples.mean()),
            p95: round(samples.percentile(0.95)),
            count: samples.count,
        }
    }
}

fn round(value: f64) -> f64 {
    if value.is_finite() {
        (value * 1000.0).round() / 1000.0
    } else {
        0.0
    }
}

/// The network as the pilot's seat felt it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Feel {
    pub rtt_ms: f64,
    /// Fire decision to the drawn muzzle flash, and to the batch reporting the host's
    /// shot, ms.
    pub fire_to_flash_ms: Reading,
    pub fire_to_report_ms: Reading,
    /// How far behind the predicted present other tanks were drawn, ms.
    pub view_delay_ms: Reading,
    /// Per hit the pilot scored: how far the host's impact was from the target as drawn,
    /// metres (zero while the host judges shells where the shooter saw the target).
    pub hit_offset_m: Reading,
    /// Per enemy shell that hit the pilot: how far from its hull it was drawn.
    pub incoming_offset_m: Reading,
    pub unseen_incoming: u64,
    /// Shots the prediction fired; those the host fired too; predicted shots it never
    /// fired; host shots the prediction missed.
    pub predicted_shots: u64,
    pub confirmed_shots: u64,
    pub unconfirmed_shots: u64,
    pub unpredicted_shots: u64,
    /// Own shells drawn striking a tank that the host flew past: hits a player would have
    /// seen and not got.
    pub phantom_strikes: u64,
    /// Enemy shells drawn striking the pilot's hull that the host flew past.
    pub phantom_incoming: u64,
    /// Snapshots that corrected the own hull's prediction, the 95th percentile and
    /// metres per second of prediction.
    pub corrections: u64,
    pub correction_p95_m: f64,
    pub correction_m_per_s: f64,
    pub prediction_lead_ms: f64,
    /// Snapshot batches that arrived late, and the longest gap between two, ms.
    pub late_batches: u64,
    pub longest_batch_gap_ms: f64,
}

impl Feel {
    pub fn read(stats: &NetworkStats, feel: &FeelReadings, shots: &OwnShotStats) -> Self {
        Self {
            rtt_ms: round(stats.rtt_ms),
            fire_to_flash_ms: (&feel.fire_to_flash).into(),
            fire_to_report_ms: (&feel.fire_to_report).into(),
            view_delay_ms: (&feel.view_delay).into(),
            hit_offset_m: (&feel.hit_offset).into(),
            incoming_offset_m: (&feel.incoming_offset).into(),
            unseen_incoming: feel.unseen_incoming,
            predicted_shots: shots.predicted,
            confirmed_shots: shots.confirmed,
            unconfirmed_shots: shots.unconfirmed_shells,
            unpredicted_shots: shots.unpredicted,
            phantom_strikes: shots.phantom_strikes,
            phantom_incoming: stats.phantom_strikes,
            corrections: stats.corrections,
            correction_p95_m: round(stats.correction_p95_m),
            correction_m_per_s: round(stats.correction_m_per_s),
            prediction_lead_ms: round(stats.prediction_lead_ms),
            late_batches: stats.late_batches,
            longest_batch_gap_ms: round(stats.longest_batch_gap_ms),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PilotReport {
    pub name: String,
    pub room: String,
    pub mission: String,
    /// `connecting`, `lobby`, `playing`, `results`, `ended` or `left`.
    pub phase: &'static str,
    /// 0 blue, 1 red, once seated.
    pub team: Option<u8>,
    /// Whether the pilot hosts the room (it then starts the next battles).
    pub host: bool,
    /// The connection's latest status line and server notice.
    pub status: String,
    pub notice: Option<String>,
    /// Why the connection gave up, once it has (`lost`, `room-ended`, `outdated`, …).
    pub ended: Option<String>,
    pub ended_text: Option<String>,
    /// The host's kills and deaths for the pilot's seat this battle.
    pub score_kills: u32,
    pub score_deaths: u32,
    pub tally: Tally,
    pub feel: Feel,
}
