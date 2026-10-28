//! Battle Setup's choices: which map a saved or requested one becomes, and the format an
//! extra level plays.

use serde_json::json;
use sloppy_core::sim::difficulty::Difficulty;
use sloppy_core::sim::game_options::{GameOptions, initial_game_options};
use sloppy_core::sim::map_options::MapId;
use sloppy_core::sim::{DamageCause, GameMode, Team, VehicleKind};

fn choices(map_mode: MapId, game_mode: GameMode) -> GameOptions {
    GameOptions {
        human_kind: VehicleKind::Heavy,
        human_team: Team::Red,
        game_mode,
        map_mode,
        difficulty: Difficulty::Hard,
    }
}

#[test]
fn an_extra_level_stands_only_where_offered_and_plays_its_team_battle() {
    let saved = choices(MapId::Superstress, GameMode::Solo);
    assert_eq!(
        saved.offered(false),
        choices(MapId::Village, GameMode::Solo),
        "without extra levels the village stands in, in the player's format"
    );
    assert_eq!(
        saved.offered(true),
        choices(MapId::Superstress, GameMode::Team)
    );
    let standard = choices(MapId::Quarry, GameMode::Solo);
    assert_eq!(standard.offered(false), standard);
    assert_eq!(standard.offered(true), standard);
}

#[test]
fn a_requested_map_and_difficulty_stand_unless_the_map_is_an_unoffered_extra_level() {
    let options = initial_game_options(7.0, Some(MapId::Harbor), false, Some(Difficulty::Easy));
    assert_eq!(
        (options.map_mode, options.difficulty),
        (MapId::Harbor, Difficulty::Easy)
    );
    let options = initial_game_options(7.0, Some(MapId::StressTest), false, None);
    assert_eq!(
        (options.map_mode, options.difficulty),
        (MapId::Village, Difficulty::Normal)
    );
    let options = initial_game_options(7.0, Some(MapId::StressTest), true, None);
    assert_eq!(options.map_mode, MapId::StressTest);
}

#[test]
fn names_match_the_serialized_ones() {
    for mode in GameMode::ALL {
        assert_eq!(json!(mode), json!(mode.as_str()));
    }
    for cause in [
        DamageCause::Standard,
        DamageCause::Spread,
        DamageCause::Rocket,
        DamageCause::Ricochet,
        DamageCause::Piercing,
        DamageCause::Tow,
        DamageCause::Mine,
        DamageCause::Drum,
        DamageCause::Interception,
        DamageCause::Explosion,
    ] {
        assert_eq!(json!(cause), json!(cause.as_str()));
    }
}
