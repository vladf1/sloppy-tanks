//! What the shells show for a drained event besides its sound: the respawn card's report
//! of who wrecked the viewer, the kill feed's row for every wreck, and the toast over
//! the field for the viewer's pickups, promotions and death. The engine attaches them to
//! the events ([`crate::events::PendingEvent`]); the page and the desktop app only lay
//! them out and time them.
//!
//! An event is the viewer's when its tank was the viewer's in the frame that showed it:
//! the rule the event's `own` flag, damage angle and hit tick already follow.

use serde::Serialize;
use sloppy_core::sim::data::vehicle;
use sloppy_core::sim::{DamageCause, SimEvent, SimEventType, Team};

use crate::battle_report::ordinal;
use crate::hud::Scoreboard;

/// The kill feed's name for the viewer, and for ordnance nobody owns (the yard's drums,
/// or a player who has left).
pub const VIEWER_NAME: &str = "YOU";
pub const YARD_NAME: &str = "YARD";

/// What destroyed a tank, as the respawn card and the death sentence name it.
pub fn damage_label(cause: DamageCause) -> &'static str {
    match cause {
        DamageCause::Standard => "Standard shell",
        DamageCause::Spread => "Spread shot",
        DamageCause::Rocket => "Rocket blast",
        DamageCause::Tow => "TOW missile",
        DamageCause::Ricochet => "Ricochet shell",
        DamageCause::Piercing => "Piercing shell",
        DamageCause::Mine => "Mine explosion",
        DamageCause::Drum => "Exploding barrel",
        DamageCause::Interception => "Shell collision blast",
        DamageCause::Explosion => "Explosion",
    }
}

/// The icon for each way to destroy a tank, as an id each shell maps to its own art (the
/// page's `hud-` symbols, the desktop's icons). Every kill shows one, so every feed row
/// reads the same way; a kill without a reported weapon shows the generic wreck mark.
pub fn cause_icon(cause: Option<DamageCause>) -> &'static str {
    match cause {
        Some(DamageCause::Standard) => "standard",
        Some(DamageCause::Spread) => "spread",
        Some(DamageCause::Rocket | DamageCause::Tow) => "rocket",
        Some(DamageCause::Ricochet) => "ricochet",
        Some(DamageCause::Piercing) => "piercing",
        Some(DamageCause::Mine) => "mine",
        Some(DamageCause::Drum) => "drum",
        Some(DamageCause::Interception | DamageCause::Explosion) => "blast",
        None => "skull",
    }
}

/// What wrecked the viewer's tank, for the respawn card, the death toast and a solo
/// battle report.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeathReport {
    /// The killer's name; `None` for the yard, a departed player or the viewer's own
    /// ordnance.
    pub killer: Option<String>,
    /// The viewer's own ordnance did it.
    #[serde(rename = "self")]
    pub own_goal: bool,
    pub cause: Option<DamageCause>,
    /// The weapon's label, such as "Rocket blast", when the engine reported one.
    pub weapon: Option<&'static str>,
    /// The weapon's icon ([`cause_icon`]).
    pub icon: &'static str,
    /// The killer's vehicle name, team and kills so far, when the scoreboard knows them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub killer_vehicle: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub killer_team: Option<Team>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub killer_kills: Option<u32>,
    /// The killer's kills as the respawn card tags them, "Their 3rd kill", once they
    /// have one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub killer_tally: Option<String>,
    /// The whole of it as a sentence, such as "Bob destroyed you with a rocket blast.";
    /// an unreported weapon is left out rather than called unknown.
    pub sentence: String,
}

/// Which side a kill-feed name is drawn in: a team's colour, or neutral for the yard.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FeedSide {
    Blue,
    Red,
    Neutral,
}

impl FeedSide {
    pub fn of(team: Option<Team>) -> Self {
        match team {
            Some(Team::Blue) => FeedSide::Blue,
            Some(Team::Red) => FeedSide::Red,
            None => FeedSide::Neutral,
        }
    }

    pub fn team(self) -> Option<Team> {
        match self {
            FeedSide::Blue => Some(Team::Blue),
            FeedSide::Red => Some(Team::Red),
            FeedSide::Neutral => None,
        }
    }
}

/// One kill-feed row: killer and victim with the weapon's icon between.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedRow {
    /// Killer and victim: [`VIEWER_NAME`] for the viewer, [`YARD_NAME`] for nobody.
    pub names: [String; 2],
    /// Each name's side.
    pub sides: [FeedSide; 2],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cause: Option<DamageCause>,
    /// The weapon's icon ([`cause_icon`]).
    pub icon: &'static str,
    /// The viewer destroyed someone else. Decided by tank identity rather than by the
    /// names shown, since another player may be called "YOU".
    pub own_kill: bool,
    /// The viewer's own wreck, which the feed highlights like the viewer's kills.
    pub own_death: bool,
}

/// How a toast reads: its colours, and how long the shells keep it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ToastTone {
    Pickup,
    Promotion,
    Death,
}

/// A notice over the field.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Toast {
    pub text: String,
    pub tone: ToastTone,
}

/// An event's feedback; most events have none.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventFeedback {
    /// The viewer's death: who and what wrecked it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub death_report: Option<DeathReport>,
    /// Every tank's death.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feed: Option<FeedRow>,
    /// The viewer's pickups, promotions and death.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toast: Option<Toast>,
}

/// The feedback for `event` as the viewer, tank `viewer`, saw it; `own` says the event
/// happened to that tank. `scoreboard` names the tanks as they were in that frame.
pub fn event_feedback(
    event: &SimEvent,
    own: bool,
    viewer: u32,
    scoreboard: &Scoreboard,
) -> EventFeedback {
    let death = event.kind == SimEventType::Death;
    let death_report = (death && own).then(|| death_report(event, scoreboard));
    let label = || event.label.clone().unwrap_or_default();
    let toast = match event.kind {
        _ if !own => None,
        SimEventType::Pickup => Some(Toast {
            text: label(),
            tone: ToastTone::Pickup,
        }),
        SimEventType::Promotion => Some(Toast {
            text: label(),
            tone: ToastTone::Promotion,
        }),
        SimEventType::Death => death_report.as_ref().map(|report| Toast {
            text: report.sentence.clone(),
            tone: ToastTone::Death,
        }),
        _ => None,
    };
    EventFeedback {
        feed: death.then(|| feed_row(event, own, viewer, scoreboard)),
        death_report,
        toast,
    }
}

/// What wrecked the tank of a death event, as its driver hears it.
fn death_report(event: &SimEvent, scoreboard: &Scoreboard) -> DeathReport {
    let own_goal = event.owner.is_some() && event.owner == event.id;
    let killer = if own_goal {
        None
    } else {
        event.owner.and_then(|owner| scoreboard.row(owner))
    };
    let cause = event.damage_source.map(|source| source.cause);
    let weapon = cause.map(damage_label);
    let with = weapon.map(|label| {
        let vowel = label
            .chars()
            .next()
            .is_some_and(|first| "aeiouAEIOU".contains(first));
        format!(
            "{} {}",
            if vowel { "an" } else { "a" },
            label.to_lowercase()
        )
    });
    let name = killer
        .as_ref()
        .map(|row| row.name.as_ref())
        .filter(|name| !name.is_empty());
    let sentence = match (own_goal, name, &with) {
        (true, _, Some(with)) => format!("You destroyed yourself with {with}."),
        (true, _, None) => "You destroyed yourself.".to_owned(),
        (false, Some(name), Some(with)) => format!("{name} destroyed you with {with}."),
        (false, Some(name), None) => format!("{name} destroyed you."),
        (false, None, Some(with)) => format!("You were destroyed by {with}."),
        (false, None, None) => "You were destroyed.".to_owned(),
    };
    DeathReport {
        killer_vehicle: killer.as_ref().map(|row| vehicle(row.kind).name),
        killer_team: killer.as_ref().map(|row| row.team),
        killer_kills: killer.as_ref().map(|row| row.kills),
        killer_tally: killer.as_ref().filter(|row| row.kills > 0).map(|row| {
            format!(
                "Their {} kill",
                ordinal(u64::from(row.kills)).to_ascii_lowercase()
            )
        }),
        killer: killer.map(|row| row.name.into_owned()),
        own_goal,
        cause,
        weapon,
        icon: cause_icon(cause),
        sentence,
    }
}

/// The kill-feed row of a death event.
fn feed_row(event: &SimEvent, own: bool, viewer: u32, scoreboard: &Scoreboard) -> FeedRow {
    let name = |id: Option<u32>| match id {
        Some(id) if id == viewer => VIEWER_NAME.to_owned(),
        Some(id) => scoreboard
            .row(id)
            .map_or_else(|| YARD_NAME.to_owned(), |row| row.name.into_owned()),
        None => YARD_NAME.to_owned(),
    };
    // The victim's team comes with the event; allies take no hull damage from each
    // other, so the killer is on the other side unless the tank destroyed itself or the
    // yard did it.
    let victim = event.team;
    let killer = match event.owner {
        None => None,
        Some(owner) if Some(owner) == event.id => victim,
        Some(_) => victim.map(Team::opponent),
    };
    let cause = event.damage_source.map(|source| source.cause);
    FeedRow {
        names: [name(event.owner), name(event.id)],
        sides: [FeedSide::of(killer), FeedSide::of(victim)],
        cause,
        icon: cause_icon(cause),
        own_kill: event.owner == Some(viewer) && event.id != Some(viewer),
        own_death: own,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud::HudScore;
    use serde_json::json;
    use sloppy_core::sim::types::DamageSource;
    use sloppy_core::sim::{Vec2, VehicleKind};
    use std::borrow::Cow;

    const VIEWER: u32 = 4;
    const RIVAL: u32 = 8;

    fn scoreboard() -> Scoreboard<'static> {
        let row = |id, name: &'static str, kills, team, kind| HudScore {
            id,
            name: Cow::Borrowed(name),
            kills,
            deaths: 0,
            team,
            kind,
        };
        Scoreboard::Kept(vec![
            row(VIEWER, "Player", 0, Team::Red, VehicleKind::Balanced),
            row(
                RIVAL,
                "Bob <b>literal</b>",
                3,
                Team::Blue,
                VehicleKind::Heavy,
            ),
        ])
    }

    /// The viewer's tank wrecked by `owner` with `cause`.
    fn death(owner: Option<u32>, cause: Option<DamageCause>) -> SimEvent {
        SimEvent {
            id: Some(VIEWER),
            owner,
            team: Some(Team::Red),
            damage_source: cause.map(|cause| DamageSource {
                cause,
                origin: Vec2::new(1.0, 2.0),
            }),
            ..SimEvent::at(SimEventType::Death, 0.0, 0.0)
        }
    }

    fn report(event: &SimEvent, scoreboard: &Scoreboard) -> DeathReport {
        event_feedback(event, true, VIEWER, scoreboard)
            .death_report
            .expect("the viewer's death has a report")
    }

    fn sentence(owner: Option<u32>, cause: Option<DamageCause>, scoreboard: &Scoreboard) -> String {
        report(&death(owner, cause), scoreboard).sentence
    }

    #[test]
    fn the_death_sentence_names_another_players_weapon_self_damage_and_the_yard() {
        let board = scoreboard();
        let rocket = Some(DamageCause::Rocket);
        assert_eq!(
            sentence(Some(RIVAL), rocket, &board),
            "Bob <b>literal</b> destroyed you with a rocket blast."
        );
        assert_eq!(
            sentence(Some(VIEWER), rocket, &board),
            "You destroyed yourself with a rocket blast."
        );
        assert_eq!(
            sentence(None, Some(DamageCause::Drum), &board),
            "You were destroyed by an exploding barrel."
        );
        assert_eq!(
            sentence(None, Some(DamageCause::Tow), &board),
            "You were destroyed by a tow missile."
        );
    }

    #[test]
    fn missing_details_and_departed_killers_still_explain_the_death() {
        let board = scoreboard();
        let rocket = Some(DamageCause::Rocket);
        assert_eq!(
            sentence(Some(RIVAL), rocket, &Scoreboard::Kept(Vec::new())),
            "You were destroyed by a rocket blast."
        );
        assert_eq!(
            sentence(Some(RIVAL), None, &board),
            "Bob <b>literal</b> destroyed you."
        );
        assert_eq!(
            sentence(Some(VIEWER), None, &board),
            "You destroyed yourself."
        );
        assert_eq!(sentence(None, None, &board), "You were destroyed.");
    }

    #[test]
    fn the_respawn_card_names_the_killer_and_weapon_apart_and_nobody_for_self_damage() {
        let board = scoreboard();
        let rival = report(&death(Some(RIVAL), Some(DamageCause::Rocket)), &board);
        assert_eq!(
            serde_json::to_value(&rival).unwrap(),
            json!({
                "killer": "Bob <b>literal</b>",
                "self": false,
                "cause": "rocket",
                "weapon": "Rocket blast",
                "icon": "rocket",
                "killerVehicle": "BIG RIG",
                "killerTeam": 0,
                "killerKills": 3,
                "killerTally": "Their 3rd kill",
                "sentence": "Bob <b>literal</b> destroyed you with a rocket blast.",
            })
        );
        let own_goal = report(&death(Some(VIEWER), None), &board);
        assert_eq!(
            serde_json::to_value(&own_goal).unwrap(),
            json!({
                "killer": null,
                "self": true,
                "cause": null,
                "weapon": null,
                "icon": "skull",
                "sentence": "You destroyed yourself.",
            })
        );
    }

    #[test]
    fn the_kill_feed_names_the_viewer_you_and_ownerless_kills_the_yard() {
        let board = scoreboard();
        let row = |event: &SimEvent, own| {
            event_feedback(event, own, VIEWER, &board)
                .feed
                .expect("every death has a feed row")
        };
        let viewer_wrecked = row(&death(Some(RIVAL), Some(DamageCause::Rocket)), true);
        assert_eq!(viewer_wrecked.names, ["Bob <b>literal</b>", "YOU"]);
        assert_eq!(viewer_wrecked.sides, [FeedSide::Blue, FeedSide::Red]);
        assert_eq!(viewer_wrecked.icon, "rocket");
        assert!(viewer_wrecked.own_death && !viewer_wrecked.own_kill);
        let yard = SimEvent {
            id: Some(RIVAL),
            owner: None,
            team: Some(Team::Blue),
            ..SimEvent::at(SimEventType::Death, 0.0, 0.0)
        };
        let yard = row(&yard, false);
        assert_eq!(yard.names, ["YARD", "Bob <b>literal</b>"]);
        assert_eq!(yard.sides, [FeedSide::Neutral, FeedSide::Blue]);
        // A kill without a reported weapon shows the wreck mark.
        assert_eq!(yard.icon, "skull");
        assert!(!yard.own_death && !yard.own_kill);
    }

    #[test]
    fn an_own_kill_follows_tank_identity_not_a_player_named_you() {
        let board = Scoreboard::Kept(vec![HudScore {
            id: 7,
            name: Cow::Borrowed("YOU"),
            kills: 0,
            deaths: 0,
            team: Team::Blue,
            kind: VehicleKind::Scout,
        }]);
        let kill = |id: u32, owner: Option<u32>| {
            let event = SimEvent {
                id: Some(id),
                owner,
                team: Some(Team::Blue),
                ..SimEvent::at(SimEventType::Death, 0.0, 0.0)
            };
            event_feedback(&event, id == VIEWER, VIEWER, &board)
                .feed
                .unwrap()
                .own_kill
        };
        assert!(kill(7, Some(VIEWER)));
        assert!(!kill(VIEWER, Some(VIEWER)));
        assert!(!kill(VIEWER, Some(7)));
        assert!(!kill(7, None));
        // Self-destruction keeps the victim's side for both names.
        let event = SimEvent {
            id: Some(7),
            owner: Some(7),
            team: Some(Team::Blue),
            ..SimEvent::at(SimEventType::Death, 0.0, 0.0)
        };
        let row = event_feedback(&event, false, VIEWER, &board).feed.unwrap();
        assert_eq!(row.sides, [FeedSide::Blue, FeedSide::Blue]);
    }

    #[test]
    fn toasts_are_the_viewers_pickups_promotions_and_death() {
        let board = scoreboard();
        let labelled = |kind, id| SimEvent {
            id: Some(id),
            label: Some("+3 ROCKETS".into()),
            ..SimEvent::at(kind, 0.0, 0.0)
        };
        let toast = |event: &SimEvent| {
            event_feedback(event, event.id == Some(VIEWER), VIEWER, &board).toast
        };
        assert_eq!(
            toast(&labelled(SimEventType::Pickup, VIEWER)),
            Some(Toast {
                text: "+3 ROCKETS".into(),
                tone: ToastTone::Pickup
            })
        );
        assert_eq!(
            toast(&labelled(SimEventType::Promotion, VIEWER)).map(|toast| toast.tone),
            Some(ToastTone::Promotion)
        );
        assert_eq!(
            toast(&death(Some(RIVAL), Some(DamageCause::Mine))),
            Some(Toast {
                text: "Bob <b>literal</b> destroyed you with a mine explosion.".into(),
                tone: ToastTone::Death
            })
        );
        // Other tanks' pickups, and the viewer's ammunition notices, raise none.
        assert_eq!(toast(&labelled(SimEventType::Pickup, RIVAL)), None);
        assert_eq!(toast(&labelled(SimEventType::Notice, VIEWER)), None);
        // Another tank's death has a feed row and nothing else.
        let other = SimEvent {
            id: Some(RIVAL),
            ..death(Some(VIEWER), Some(DamageCause::Standard))
        };
        let feedback = event_feedback(&other, false, VIEWER, &board);
        assert!(feedback.death_report.is_none() && feedback.toast.is_none());
        assert!(feedback.feed.is_some_and(|row| row.own_kill));
        assert_eq!(
            event_feedback(
                &SimEvent::at(SimEventType::Shot, 0.0, 0.0),
                true,
                VIEWER,
                &board
            ),
            EventFeedback::default()
        );
    }
}
