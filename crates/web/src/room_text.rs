//! The room menu's words, which the page (`src/net/network-ui.ts`) and the desktop app
//! (`ui/room_menu.rs`) only lay out: the connection dialog while the socket is down or
//! after the room stopped, the players who join or come back, and each lobby's title,
//! hint and rules. `NetGame` keeps a [`RoomText`] beside its client and attaches these
//! to the notices they belong to, so they cross to the page only when they change.

use serde::Serialize;
use sloppy_core::net::client::EndCause;
use sloppy_core::net::protocol::{Lobby, RoomPhase, RoomSettings};
use sloppy_core::sim::map_options::map_option_for;
use sloppy_core::sim::{Match, Team};

use crate::Shell;
use crate::catalog::difficulty_name;

/// The connection dialog's title before the socket was ever live; the joining screen
/// covers the dialog then.
const FIRST_CONNECTION: &str = "CONNECTING";
/// The title when a live room's socket drops.
const CONNECTION_LOST: &str = "CONNECTION LOST";
/// The title once the player asks to join a stopped room again.
const RECONNECTING: &str = "RECONNECTING";

/// The connection dialog while the socket is down and the client reconnects by itself.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ConnectionDialog {
    pub title: &'static str,
    /// The client's progress and, during a battle, what the tank does meanwhile.
    pub message: String,
}

/// The connection dialog once the room stopped for good, and the ways back it offers.
#[derive(Clone, Debug, PartialEq)]
pub struct Ending {
    /// Why the client stopped retrying; `None` when the shell's renderer stopped.
    pub cause: Option<EndCause>,
    pub text: String,
    pub title: &'static str,
    /// The button that joins the room again, for the causes where that can work.
    pub retry: Option<&'static str>,
    /// The button that leaves for Battle Setup, which the page names RELOAD where only a
    /// new page recovers.
    pub setup: &'static str,
}

/// A player who joined the room or came back, as a kill feed line.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Arrival {
    pub text: String,
    /// The team whose colour the desktop app draws the line in; the page shows the text
    /// alone.
    #[serde(skip)]
    pub team: Team,
}

/// The room menu's words for one lobby.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct MenuText {
    /// LOBBY, BATTLE IN PROGRESS, or how the last round ended.
    pub title: &'static str,
    /// What happens to the tank while the menu is open, or who starts the next battle.
    pub hint: String,
    /// The rules as chips: the arena, the bots and the match length.
    pub summary: Vec<String>,
}

/// What the room menu showed so far, which the words of the next notice depend on. The
/// notices reach the shell in the order they pass through here.
#[derive(Debug)]
pub struct RoomText {
    /// The last lobby: who is new in the next one, and whether a drop interrupts a battle.
    lobby: Option<Lobby>,
    /// The finished round as the viewer's tank played it, until the room leaves its results.
    outcome: Option<(Match, Team)>,
    /// The round this player ended as host; its seat had the menu open, so no final state
    /// came for it.
    ended_round: Option<u64>,
    /// The connection dialog's title while the socket is down; `None` while it is live.
    dropped: Option<&'static str>,
}

impl Default for RoomText {
    fn default() -> Self {
        Self {
            lobby: None,
            outcome: None,
            ended_round: None,
            dropped: Some(FIRST_CONNECTION),
        }
    }
}

impl RoomText {
    /// Connection progress: the dialog while the socket is down, `None` once it is back.
    pub fn status(&mut self, text: &str, connected: bool) -> Option<ConnectionDialog> {
        if connected {
            self.dropped = None;
            return None;
        }
        // A drop from a live room is news; later attempts keep the dialog's title, an
        // ending's included.
        let title = *self.dropped.get_or_insert(CONNECTION_LOST);
        Some(ConnectionDialog {
            title,
            message: connecting_message(text, self.lobby.as_ref()),
        })
    }

    /// The client gave up for `cause`.
    pub fn ended(&mut self, cause: EndCause, text: String, shell: Shell) -> Ending {
        self.end(ending(Some(cause), text, shell))
    }

    /// The shell's renderer failed (a GPU error or a lost device).
    pub fn renderer_stopped(&mut self, shell: Shell) -> Ending {
        let text = match shell {
            Shell::Page => "The renderer stopped. Reload the page to play again.",
            Shell::Desktop => "The renderer stopped. Go back to Battle Setup to play again.",
        };
        self.end(ending(None, text.into(), shell))
    }

    fn end(&mut self, ending: Ending) -> Ending {
        self.dropped = Some(ending.title);
        ending
    }

    /// The player asked to join the stopped room again.
    pub fn rejoining(&mut self) {
        self.dropped = Some(RECONNECTING);
    }

    /// The host ended the battle from its menu.
    pub fn ending_round(&mut self) {
        self.ended_round = self.lobby.as_ref().map(|lobby| lobby.round_id);
    }

    /// The finished round as the viewer's tank played it; its results lobby follows.
    pub fn result(&mut self, outcome: &Match, team: Team) {
        self.outcome = Some((outcome.clone(), team));
    }

    /// A new lobby for `player_id`: the menu's words for it, and who joined or came back
    /// since the last one.
    pub fn lobby(&mut self, lobby: &Lobby, player_id: &str) -> (MenuText, Vec<Arrival>) {
        let arrivals = arrivals(self.lobby.as_ref(), lobby, player_id);
        if lobby.phase != RoomPhase::Results {
            self.outcome = None;
        }
        let menu = menu_text(lobby, player_id, self.outcome.as_ref(), self.ended_round);
        self.lobby = Some(lobby.clone());
        (menu, arrivals)
    }
}

/// The dialog for a room that stopped for `cause` (`None`: the renderer stopped).
pub fn ending(cause: Option<EndCause>, text: String, shell: Shell) -> Ending {
    let (title, retry) = match cause {
        Some(EndCause::Lost) => ("CONNECTION LOST", Some("TRY AGAIN")),
        Some(EndCause::Rejected) => ("DISCONNECTED", Some("TRY AGAIN")),
        Some(EndCause::SeatExpired) => ("SEAT EXPIRED", Some("JOIN AGAIN")),
        Some(EndCause::OtherTab) => (
            match shell {
                Shell::Page => "PLAYING IN ANOTHER TAB",
                Shell::Desktop => "PLAYING ELSEWHERE",
            },
            Some("PLAY HERE"),
        ),
        Some(EndCause::RoomEnded) => ("ROOM CLOSED", None),
        Some(EndCause::Outdated) => ("GAME UPDATED", None),
        None => ("RENDERER STOPPED", None),
    };
    // A new engine or renderer takes a new page; the desktop app builds both again from
    // its own Battle Setup.
    let reload = shell == Shell::Page && matches!(cause, Some(EndCause::Outdated) | None);
    Ending {
        cause,
        text,
        title,
        retry,
        setup: if reload { "RELOAD" } else { "BATTLE SETUP" },
    }
}

/// The dialog's message while reconnecting: the client's `text` and, during a battle,
/// what the tank does until the player is back.
pub fn connecting_message(text: &str, lobby: Option<&Lobby>) -> String {
    match lobby.filter(|lobby| lobby.phase == RoomPhase::Playing) {
        Some(lobby) if lobby.settings.humans_only => {
            format!("{text} Your tank sits idle until you're back.")
        }
        Some(_) => format!("{text} A bot drives your tank until you're back."),
        None => text.to_owned(),
    }
}

/// Players other than `player_id` who connected since `previous` in the same room
/// instance: a new seat joined its team, a known one reconnected.
pub fn arrivals(previous: Option<&Lobby>, lobby: &Lobby, player_id: &str) -> Vec<Arrival> {
    let Some(previous) = previous.filter(|previous| previous.room_epoch == lobby.room_epoch) else {
        return Vec::new();
    };
    lobby
        .players
        .iter()
        .filter(|player| player.player_id != player_id && player.connected)
        .filter_map(|player| {
            let before = previous
                .players
                .iter()
                .find(|item| item.player_id == player.player_id);
            let text = match before {
                Some(before) if before.connected => return None,
                Some(_) => format!("{} reconnected", player.name),
                None => format!("{} joined {} team", player.name, team_word(player.team)),
            };
            Some(Arrival {
                text,
                team: player.team,
            })
        })
        .collect()
}

fn team_word(team: Team) -> &'static str {
    match team {
        Team::Blue => "Blue",
        Team::Red => "Red",
    }
}

/// The room menu's words for `lobby` as `player_id` sees it. `outcome` is the finished
/// round, and `ended_round` the round this player ended as host.
pub fn menu_text(
    lobby: &Lobby,
    player_id: &str,
    outcome: Option<&(Match, Team)>,
    ended_round: Option<u64>,
) -> MenuText {
    let title = match lobby.phase {
        RoomPhase::Playing => "BATTLE IN PROGRESS",
        RoomPhase::Results => results_title(outcome, ended_round == Some(lobby.round_id)),
        RoomPhase::Lobby => "LOBBY",
    };
    MenuText {
        title,
        hint: hint(lobby, player_id),
        summary: summary(&lobby.settings),
    }
}

/// Victory or defeat when the final state arrived. A player who joined during the
/// results, or had the menu open as the round ended, gets a neutral title; a host who
/// ended it (`ended_it`) reads that it ended.
fn results_title(outcome: Option<&(Match, Team)>, ended_it: bool) -> &'static str {
    match outcome {
        None if ended_it => "BATTLE ENDED",
        None => "ROUND COMPLETE",
        Some((outcome, _)) if outcome.ended_early == Some(true) || outcome.winner.is_none() => {
            "BATTLE ENDED"
        }
        Some((outcome, team)) if outcome.winner == Some(*team) => "VICTORY",
        Some(_) => "DEFEAT",
    }
}

/// The line under the title: what the open menu costs during a battle, and between
/// battles who starts the next one.
fn hint(lobby: &Lobby, player_id: &str) -> String {
    let next = if lobby.phase == RoomPhase::Results {
        "the next battle"
    } else {
        "the battle"
    };
    match lobby.phase {
        RoomPhase::Playing if lobby.settings.humans_only => {
            "The battle keeps going. Your tank sits idle and vulnerable while this menu is open."
                .into()
        }
        RoomPhase::Playing => {
            "The battle keeps going. A bot drives your tank while this menu is open.".into()
        }
        _ if lobby.host_id != player_id => {
            let host = lobby
                .players
                .iter()
                .find(|player| player.player_id == lobby.host_id)
                .map_or("the host", |player| player.name.as_str());
            format!("You stay in this room with everyone. Waiting for {host} to start {next}.")
        }
        _ if lobby.players.len() == 1 => {
            format!("You're the only one here. Share the invite link, or start {next} on your own.")
        }
        RoomPhase::Results => {
            "Everyone stays in this room. Play again to keep the teams together.".into()
        }
        RoomPhase::Lobby => {
            "Everyone stays in this room. Start the battle to keep the teams together.".into()
        }
    }
}

/// The room's rules as read-only chips: the arena, the bots and the match length.
pub fn summary(settings: &RoomSettings) -> Vec<String> {
    let map = map_option_for(settings.map_mode);
    let arena = if map.extra {
        format!("{} · Extra level", map.name)
    } else {
        map.name.to_owned()
    };
    let bots = if settings.humans_only {
        "No bots".to_owned()
    } else {
        format!("{} bots", difficulty_name(settings.difficulty))
    };
    vec![arena, bots, format!("{} min", settings.round_minutes)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use sloppy_core::net::protocol::Player;
    use sloppy_core::sim::difficulty::Difficulty;
    use sloppy_core::sim::map_options::MapId;
    use sloppy_core::sim::{MatchPhase, VehicleKind};

    fn player(id: &str, name: &str, team: Team, connected: bool) -> Player {
        Player {
            player_id: id.into(),
            name: name.into(),
            kind: VehicleKind::Balanced,
            team,
            slot: 0,
            connected,
            kills: 0,
            deaths: 0,
            tank_id: None,
        }
    }

    fn lobby(phase: RoomPhase, players: Vec<Player>) -> Lobby {
        Lobby {
            room_epoch: "e1".into(),
            round_id: 3,
            phase,
            host_id: "me".into(),
            scoreboard: Vec::new(),
            players,
            settings: RoomSettings::default(),
        }
    }

    fn outcome(winner: Option<Team>, ended_early: Option<bool>) -> Match {
        Match {
            phase: MatchPhase::Results,
            time: 0.0,
            scores: [10, 8],
            overtime: false,
            ended_early,
            winner,
            round: 3,
        }
    }

    #[test]
    fn each_ending_offers_the_way_back_that_can_work() {
        let words = |cause, shell| {
            let ending = ending(cause, String::new(), shell);
            (ending.title, ending.retry, ending.setup)
        };
        let page = Shell::Page;
        assert_eq!(
            words(Some(EndCause::Lost), page),
            ("CONNECTION LOST", Some("TRY AGAIN"), "BATTLE SETUP")
        );
        assert_eq!(
            words(Some(EndCause::Rejected), page),
            ("DISCONNECTED", Some("TRY AGAIN"), "BATTLE SETUP")
        );
        assert_eq!(
            words(Some(EndCause::SeatExpired), page),
            ("SEAT EXPIRED", Some("JOIN AGAIN"), "BATTLE SETUP")
        );
        assert_eq!(
            words(Some(EndCause::RoomEnded), page),
            ("ROOM CLOSED", None, "BATTLE SETUP")
        );
        assert_eq!(
            words(Some(EndCause::OtherTab), page),
            ("PLAYING IN ANOTHER TAB", Some("PLAY HERE"), "BATTLE SETUP")
        );
        assert_eq!(
            words(Some(EndCause::OtherTab), Shell::Desktop),
            ("PLAYING ELSEWHERE", Some("PLAY HERE"), "BATTLE SETUP")
        );
        // Only a new page loads a new engine or renderer; the app rebuilds both itself.
        assert_eq!(
            words(Some(EndCause::Outdated), page),
            ("GAME UPDATED", None, "RELOAD")
        );
        assert_eq!(words(None, page), ("RENDERER STOPPED", None, "RELOAD"));
        assert_eq!(
            words(Some(EndCause::Outdated), Shell::Desktop),
            ("GAME UPDATED", None, "BATTLE SETUP")
        );
        assert_eq!(
            words(None, Shell::Desktop),
            ("RENDERER STOPPED", None, "BATTLE SETUP")
        );
        let mut text = RoomText::default();
        assert_eq!(
            text.renderer_stopped(Shell::Page).text,
            "The renderer stopped. Reload the page to play again."
        );
        assert_eq!(
            text.renderer_stopped(Shell::Desktop).text,
            "The renderer stopped. Go back to Battle Setup to play again."
        );
    }

    #[test]
    fn the_connection_dialog_names_the_drop_once() {
        let mut text = RoomText::default();
        let title = |dialog: Option<ConnectionDialog>| dialog.map(|dialog| dialog.title);
        // The first connection's dialog stays behind the joining screen.
        assert_eq!(
            title(text.status("Connecting to the room…", false)),
            Some("CONNECTING")
        );
        assert_eq!(text.status("Connected", true), None);
        assert_eq!(
            title(text.status("Reconnecting…", false)),
            Some("CONNECTION LOST")
        );
        assert_eq!(
            title(text.status("Still trying", false)),
            Some("CONNECTION LOST")
        );
        let ending = text.ended(EndCause::Lost, "Gone".into(), Shell::Page);
        assert_eq!(ending.title, "CONNECTION LOST");
        text.ended(EndCause::SeatExpired, "Expired".into(), Shell::Page);
        // A later drop keeps the ending's title until the player asks to join again.
        assert_eq!(title(text.status("Retrying", false)), Some("SEAT EXPIRED"));
        text.rejoining();
        assert_eq!(
            title(text.status("Connecting to the room…", false)),
            Some("RECONNECTING")
        );
        assert_eq!(text.status("Connected", true), None);
        assert_eq!(
            title(text.status("Reconnecting…", false)),
            Some("CONNECTION LOST")
        );
    }

    #[test]
    fn a_drop_during_a_battle_says_who_drives_the_tank() {
        let mut text = RoomText::default();
        let message = |text: &mut RoomText| text.status("Reconnecting…", false).unwrap().message;
        assert_eq!(message(&mut text), "Reconnecting…");
        text.lobby(&lobby(RoomPhase::Lobby, Vec::new()), "me");
        assert_eq!(message(&mut text), "Reconnecting…");
        let mut playing = lobby(RoomPhase::Playing, Vec::new());
        text.lobby(&playing, "me");
        assert_eq!(
            message(&mut text),
            "Reconnecting… A bot drives your tank until you're back."
        );
        playing.settings.humans_only = true;
        text.lobby(&playing, "me");
        assert_eq!(
            message(&mut text),
            "Reconnecting… Your tank sits idle until you're back."
        );
    }

    #[test]
    fn the_feed_names_who_joins_or_comes_back() {
        let before = lobby(
            RoomPhase::Lobby,
            vec![
                player("me", "Me", Team::Blue, true),
                player("back", "Tusk", Team::Red, false),
                player("there", "Crank", Team::Blue, true),
            ],
        );
        let mut after = lobby(
            RoomPhase::Lobby,
            vec![
                player("me", "Me", Team::Blue, true),
                player("back", "Tusk", Team::Red, true),
                player("there", "Crank", Team::Blue, true),
                player("new", "Nitro", Team::Red, true),
                player("late", "Slow", Team::Blue, false),
            ],
        );
        assert_eq!(
            arrivals(Some(&before), &after, "me"),
            [
                Arrival {
                    text: "Tusk reconnected".into(),
                    team: Team::Red
                },
                Arrival {
                    text: "Nitro joined Red team".into(),
                    team: Team::Red
                },
            ]
        );
        // The viewer's own return is no news, nor is a first lobby or a new room instance.
        assert!(arrivals(Some(&before), &before, "back").is_empty());
        assert!(arrivals(None, &after, "me").is_empty());
        after.room_epoch = "e2".into();
        assert!(arrivals(Some(&before), &after, "me").is_empty());
        let mut text = RoomText::default();
        text.lobby(&before, "me");
        after.room_epoch = before.room_epoch.clone();
        let (_, feed) = text.lobby(&after, "me");
        assert_eq!(feed.len(), 2);
        assert!(text.lobby(&after, "me").1.is_empty());
    }

    #[test]
    fn the_results_title_follows_how_the_round_ended() {
        let title =
            |text: &mut RoomText, phase| text.lobby(&lobby(phase, Vec::new()), "me").0.title;
        let mut text = RoomText::default();
        assert_eq!(title(&mut text, RoomPhase::Lobby), "LOBBY");
        assert_eq!(title(&mut text, RoomPhase::Results), "ROUND COMPLETE");
        text.result(&outcome(Some(Team::Blue), Some(false)), Team::Blue);
        assert_eq!(title(&mut text, RoomPhase::Results), "VICTORY");
        text.result(&outcome(Some(Team::Blue), None), Team::Red);
        assert_eq!(title(&mut text, RoomPhase::Results), "DEFEAT");
        text.result(&outcome(Some(Team::Blue), Some(true)), Team::Blue);
        assert_eq!(title(&mut text, RoomPhase::Results), "BATTLE ENDED");
        text.result(&outcome(None, None), Team::Blue);
        assert_eq!(title(&mut text, RoomPhase::Results), "BATTLE ENDED");
        // Leaving the results forgets the round; the next results without a final state
        // read neutral, unless this host ended that round from its menu.
        assert_eq!(title(&mut text, RoomPhase::Playing), "BATTLE IN PROGRESS");
        assert_eq!(title(&mut text, RoomPhase::Results), "ROUND COMPLETE");
        title(&mut text, RoomPhase::Playing);
        text.ending_round();
        assert_eq!(title(&mut text, RoomPhase::Results), "BATTLE ENDED");
        let mut next_round = lobby(RoomPhase::Results, Vec::new());
        next_round.round_id += 1;
        assert_eq!(text.lobby(&next_round, "me").0.title, "ROUND COMPLETE");
    }

    #[test]
    fn the_hint_says_who_starts_the_next_battle() {
        let players = vec![
            player("me", "Me", Team::Blue, true),
            player("friend", "Nitro", Team::Red, true),
        ];
        let hint = |phase, players: &[Player], viewer: &str, humans_only| {
            let mut lobby = lobby(phase, players.to_vec());
            lobby.settings.humans_only = humans_only;
            menu_text(&lobby, viewer, None, None).hint
        };
        assert_eq!(
            hint(RoomPhase::Playing, &players, "me", false),
            "The battle keeps going. A bot drives your tank while this menu is open."
        );
        assert_eq!(
            hint(RoomPhase::Playing, &players, "friend", true),
            "The battle keeps going. Your tank sits idle and vulnerable while this menu is open."
        );
        assert_eq!(
            hint(RoomPhase::Lobby, &players, "friend", false),
            "You stay in this room with everyone. Waiting for Me to start the battle."
        );
        assert_eq!(
            hint(RoomPhase::Results, &players[1..], "friend", false),
            "You stay in this room with everyone. Waiting for the host to start the next battle."
        );
        assert_eq!(
            hint(RoomPhase::Lobby, &players[..1], "me", false),
            "You're the only one here. Share the invite link, or start the battle on your own."
        );
        assert_eq!(
            hint(RoomPhase::Results, &players[..1], "me", false),
            "You're the only one here. Share the invite link, or start the next battle on your own."
        );
        assert_eq!(
            hint(RoomPhase::Lobby, &players, "me", false),
            "Everyone stays in this room. Start the battle to keep the teams together."
        );
        assert_eq!(
            hint(RoomPhase::Results, &players, "me", false),
            "Everyone stays in this room. Play again to keep the teams together."
        );
    }

    #[test]
    fn the_summary_names_the_arena_bots_and_length() {
        let mut settings = RoomSettings {
            map_mode: MapId::Harbor,
            difficulty: Difficulty::Hard,
            humans_only: false,
            round_minutes: 8,
        };
        assert_eq!(summary(&settings), ["Harbor Havoc", "Hard bots", "8 min"]);
        settings.humans_only = true;
        settings.map_mode = MapId::StressTest;
        assert_eq!(
            summary(&settings),
            ["Stress Grid · Extra level", "No bots", "8 min"]
        );
    }
}
