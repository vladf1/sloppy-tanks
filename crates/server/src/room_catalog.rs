//! The public room directory behind `/rooms`.

use std::collections::HashMap;

use sloppy_core::net::room_list::{MAX_LISTED_ROOMS, ROOM_LIST_TTL_MS, RoomListing};
use sloppy_core::sim::map_options::{MapId, is_extra_level};

struct ListedRoom {
    entry: RoomListing,
    updated_ms: u64,
}

/// Discovery metadata only; room authority and seat credentials never live here.
#[derive(Default)]
pub struct RoomCatalog {
    rooms: HashMap<String, ListedRoom>,
}

impl RoomCatalog {
    /// Publishes a room's listing; a room without connected players is removed at once.
    /// Returns whether the room appeared in or left the list.
    pub fn update(&mut self, entry: RoomListing, now_ms: u64) -> bool {
        self.prune(now_ms);
        let was_listed = self.rooms.remove(&entry.room).is_some();
        if entry.players == 0 {
            return was_listed;
        }
        if self.rooms.len() >= MAX_LISTED_ROOMS
            && let Some(oldest) = self
                .rooms
                .iter()
                .min_by_key(|(_, room)| room.updated_ms)
                .map(|(code, _)| code.clone())
        {
            self.rooms.remove(&oldest);
        }
        self.rooms.insert(
            entry.room.clone(),
            ListedRoom {
                entry,
                updated_ms: now_ms,
            },
        );
        !was_listed
    }

    /// Unlists a room that ended; returns whether it was listed.
    pub fn remove(&mut self, room: &str) -> bool {
        self.rooms.remove(room).is_some()
    }

    fn prune(&mut self, now_ms: u64) {
        self.rooms
            .retain(|_, room| now_ms.saturating_sub(room.updated_ms) < ROOM_LIST_TTL_MS);
    }

    /// Rooms on standard maps, plus rooms on extra levels when asked for; the fullest
    /// rooms first, then by code.
    pub fn list(&mut self, now_ms: u64, extra_levels: bool) -> Vec<RoomListing> {
        self.prune(now_ms);
        let mut rooms: Vec<RoomListing> = self
            .rooms
            .values()
            .filter(|room| {
                extra_levels || !MapId::parse(&room.entry.map_mode).is_some_and(is_extra_level)
            })
            .map(|room| room.entry.clone())
            .collect();
        rooms.sort_by(|a, b| b.players.cmp(&a.players).then_with(|| a.room.cmp(&b.room)));
        rooms
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sloppy_core::net::room_list::RoomPhase;

    fn entry() -> RoomListing {
        RoomListing {
            room: "ABCDEFGH".into(),
            content_version: "test".into(),
            map_mode: "harbor".into(),
            difficulty: "normal".into(),
            humans_only: true,
            round_minutes: 10,
            players: 1,
            reserved: 1,
            phase: RoomPhase::Playing,
            round_id: 1,
            time: 280,
            scores: [0, 0],
        }
    }

    #[test]
    fn listings_sort_by_players_expire_and_disappear_with_the_last_player() {
        let mut catalog = RoomCatalog::default();
        catalog.update(entry(), 0);
        catalog.update(
            RoomListing {
                room: "BCDEFGHJ".into(),
                players: 2,
                reserved: 2,
                ..entry()
            },
            1,
        );
        let players: Vec<u32> = catalog
            .list(2, false)
            .iter()
            .map(|room| room.players)
            .collect();
        assert_eq!(players, [2, 1]);
        catalog.update(
            RoomListing {
                players: 0,
                ..entry()
            },
            3,
        );
        let codes: Vec<String> = catalog
            .list(3, false)
            .into_iter()
            .map(|room| room.room)
            .collect();
        assert_eq!(codes, ["BCDEFGHJ"]);
        assert_eq!(catalog.list(ROOM_LIST_TTL_MS, false).len(), 1);
        assert_eq!(catalog.list(ROOM_LIST_TTL_MS + 1, false).len(), 0);
    }

    #[test]
    fn reports_when_a_room_enters_or_leaves_the_list() {
        let mut catalog = RoomCatalog::default();
        assert!(catalog.update(entry(), 0), "a room's first player lists it");
        assert!(
            !catalog.update(
                RoomListing {
                    players: 2,
                    ..entry()
                },
                1
            ),
            "a refreshed listing"
        );
        assert!(
            catalog.update(
                RoomListing {
                    players: 0,
                    ..entry()
                },
                2
            ),
            "the last player left"
        );
        assert!(!catalog.remove("ABCDEFGH"), "an unlisted room ended");
        catalog.update(entry(), 3);
        assert!(catalog.remove("ABCDEFGH"), "a listed room ended");
        assert!(catalog.list(4, true).is_empty());
    }

    #[test]
    fn capacity_is_bounded_and_evicts_the_least_recently_refreshed_listing() {
        let mut catalog = RoomCatalog::default();
        for index in 0..=MAX_LISTED_ROOMS {
            catalog.update(
                RoomListing {
                    room: index.to_string(),
                    ..entry()
                },
                index as u64,
            );
        }
        let now = MAX_LISTED_ROOMS as u64;
        assert_eq!(catalog.list(now, false).len(), MAX_LISTED_ROOMS);
        assert!(!catalog.list(now, false).iter().any(|room| room.room == "0"));
        catalog.update(
            RoomListing {
                room: "1".into(),
                players: 4,
                ..entry()
            },
            now + 1,
        );
        assert_eq!(catalog.list(now + 1, false)[0].players, 4);
    }

    #[test]
    fn extra_level_rooms_are_listed_only_when_asked_for() {
        let mut catalog = RoomCatalog::default();
        catalog.update(entry(), 0);
        catalog.update(
            RoomListing {
                room: "YARDROOM".into(),
                map_mode: "superstress".into(),
                ..entry()
            },
            0,
        );
        let codes = |catalog: &mut RoomCatalog, extra| -> Vec<String> {
            catalog
                .list(1, extra)
                .into_iter()
                .map(|room| room.room)
                .collect()
        };
        assert_eq!(codes(&mut catalog, false), ["ABCDEFGH"]);
        assert_eq!(codes(&mut catalog, true), ["ABCDEFGH", "YARDROOM"]);
    }

    #[test]
    fn listing_json_uses_the_typescript_field_names() {
        let json = serde_json::to_string(&entry()).unwrap();
        assert_eq!(
            json,
            r#"{"room":"ABCDEFGH","contentVersion":"test","mapMode":"harbor","difficulty":"normal","humansOnly":true,"roundMinutes":10,"players":1,"reserved":1,"phase":"playing","roundId":1,"time":280,"scores":[0,0]}"#
        );
    }
}
