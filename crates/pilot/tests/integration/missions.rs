//! Pilots in a room with the real host: one creates a room and fights the fill bots
//! through the browser's client, a hunter takes the other side from its player and
//! closes in, and an escort takes its player's side and stays with them.

use crate::support::{self, Room};

use sloppy_core::net::protocol::RoomPhase;
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::math::distance;
use sloppy_pilot::orders::Mission;

const SECOND: f64 = 1000.0;

#[test]
fn a_pilot_creates_a_room_drives_and_fights_the_fill_bots() {
    let mut room = Room::new(4242);
    let pilot = room.add(
        "pilot",
        support::orders(Mission::Skirmish),
        Some(support::settings(MapId::Village)),
    );
    assert!(
        room.run_until(5.0 * SECOND, |room| room.own_tank(pilot).is_some()),
        "the pilot plays within seconds of creating the room"
    );
    assert_eq!(room.host.phase, RoomPhase::Playing);
    let start = room.host_tank("pilot").unwrap().0;
    let mut travelled: f64 = 0.0;
    room.run_until(40.0 * SECOND, |room| {
        if let Some((at, ..)) = room.host_tank("pilot") {
            travelled = travelled.max(distance(start, at));
        }
        let report = room.seats[pilot].pilot.report(room.now);
        travelled > 10.0 && report.tally.hits > 0
    });
    let report = room.seats[pilot].pilot.report(room.now);
    eprintln!(
        "DEBUG {:.1}s travelled {travelled:.1} {report:#?}",
        room.now / 1000.0
    );
    assert_eq!(report.phase, "playing");
    assert!(report.host, "the creator hosts the room");
    assert!(
        travelled > 10.0,
        "the host saw the pilot drive ({travelled:.1} m)"
    );
    assert!(report.tally.trigger_pulls > 0, "the brain fired");
    assert!(
        report.feel.confirmed_shots > 0,
        "the host fired the pilot's shots: {:?}",
        report.feel
    );
    assert!(
        report.tally.hits > 0,
        "the pilot hit a fill bot: {:?}",
        report.tally
    );
    assert!(report.feel.fire_to_flash_ms.count > 0);
    assert!(room.host.input_lapses() == 0, "input arrived on time");
}

#[test]
fn a_hunter_takes_the_other_side_and_goes_after_its_player() {
    let mut room = Room::new(77);
    let player = room.add(
        "vlad",
        support::orders(Mission::Skirmish),
        Some(support::settings(MapId::Harbor)),
    );
    room.run(3.0 * SECOND);
    let hunter = room.add(
        "hunter",
        support::orders(Mission::Hunt(Some("VLAD".into()))),
        None,
    );
    assert!(
        room.run_until(20.0 * SECOND, |room| {
            let (Some((.., ours)), Some((.., theirs))) =
                (room.host_tank("hunter"), room.host_tank("vlad"))
            else {
                return false;
            };
            ours != theirs && room.own_tank(hunter).is_some_and(|tank| tank.alive)
        }),
        "the hunter plays on the other team"
    );
    // The room's own team balance put the second player on the other side.
    assert_eq!(
        room.seats[hunter].pilot.report(room.now).tally.team_changes,
        0
    );
    // The hunter closes in on its player wherever they are, and shoots them: the only
    // other player in the room.
    let mut closest = f64::INFINITY;
    room.run_until(60.0 * SECOND, |room| {
        if let Some(gap) = room.host_distance("hunter", "vlad") {
            closest = closest.min(gap);
        }
        room.seats[hunter].pilot.report(room.now).tally.player_hits > 0
    });
    let hunted = room.seats[player].pilot.report(room.now).tally;
    let report = room.seats[hunter].pilot.report(room.now).tally;
    assert!(closest < 25.0, "the hunter came within {closest:.1} m");
    assert!(
        report.player_hits > 0,
        "the hunter hit its player: {report:?}"
    );
    assert!(hunted.player_hits_taken >= report.player_hits, "{hunted:?}");
}

#[test]
fn an_escort_takes_its_players_side_and_stays_with_them() {
    let mut room = Room::new(9);
    room.add(
        "vlad",
        support::orders(Mission::Skirmish),
        Some(support::settings(MapId::Quarry)),
    );
    room.run(3.0 * SECOND);
    let escort = room.add(
        "wingman",
        support::orders(Mission::Escort("vlad".into())),
        None,
    );
    assert!(
        room.run_until(20.0 * SECOND, |room| {
            let (Some((.., ours)), Some((.., theirs))) =
                (room.host_tank("wingman"), room.host_tank("vlad"))
            else {
                return false;
            };
            ours == theirs && room.own_tank(escort).is_some_and(|tank| tank.alive)
        }),
        "the escort plays on its player's team"
    );
    // The room put it on the other team, mid-battle: it gave the seat up and joined again.
    let report = room.seats[escort].pilot.report(room.now);
    assert_eq!(report.tally.team_changes, 1);
    assert_eq!(report.tally.rounds, 1);
    // Once it has caught up, the escort stays near its player while both live.
    room.run(15.0 * SECOND);
    let (mut near, mut samples) = (0, 0);
    for _ in 0..40 {
        room.run(0.5 * SECOND);
        if let Some(gap) = room.host_distance("wingman", "vlad") {
            samples += 1;
            near += usize::from(gap < 16.0);
        }
    }
    assert!(samples > 10, "both lived for a while ({samples} samples)");
    assert!(
        near * 3 >= samples * 2,
        "the escort was within 16 m for {near} of {samples} samples"
    );
}
