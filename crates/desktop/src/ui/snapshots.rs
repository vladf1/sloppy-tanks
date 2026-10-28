//! Pictures of every screen for looking at them without playing to them: each scenario
//! fabricates a HUD state, lays the screens out with egui and draws them with egui-wgpu
//! into a PNG over a flat arena-coloured backdrop. It needs a GPU, so it is ignored by
//! default: `SLOPPY_UI_SNAPSHOTS=<dir> cargo test -p sloppy-desktop --release
//! snapshots -- --ignored`.

use std::borrow::Cow;

use egui_wgpu::ScreenDescriptor;
use serde_json::json;
use sloppy_core::sim::ammunition::AMMO_ORDER;
use sloppy_core::sim::round_recap::{Feat, Metric, RecapStats};
use sloppy_core::sim::{DamageCause, GameMode, MatchPhase, Team, VehicleKind, Weapon};
use sloppy_render::wgpu;
use sloppy_web::Shell;
use sloppy_web::battle_report::{battle_report, pause_report};
use sloppy_web::hud::{Hud, HudAmmo, HudHuman, HudScore, Scoreboard};
use sloppy_web::recap::RoundRecap;

use super::{DeathReport, KillFeedEntry, PauseTab, Toast, ToastTone, UiState, View};
use crate::preferences::Preferences;

/// The window's size in points, and its pixels per point.
const SIZE: [f32; 2] = [1280.0, 800.0];
const SCALE: f32 = 2.0;

fn metrics(values: &[(Metric, f64)]) -> RecapStats {
    let mut stats = RecapStats::default();
    for &(metric, value) in values {
        stats.set(metric, value);
    }
    stats
}

fn hud(phase: MatchPhase, solo: bool, alive: bool, ratio: f64) -> Hud<'static> {
    const NAMES: [&str; 12] = [
        "YOU", "Rusty", "Dozer", "Spud", "Gravel", "Boomer", "Mortar", "Tusk", "Crank", "Biscuit",
        "Hatch", "Dent",
    ];
    const KILLS: [u32; 12] = [7, 9, 3, 2, 1, 0, 8, 4, 4, 2, 1, 0];
    const DEATHS: [u32; 12] = [3, 2, 4, 5, 2, 6, 3, 3, 4, 5, 2, 7];
    const KINDS: [VehicleKind; 3] = [
        VehicleKind::Balanced,
        VehicleKind::Heavy,
        VehicleKind::Scout,
    ];
    let rows = (0..12)
        .map(|index| HudScore {
            id: index as u32 + 1,
            name: Cow::Borrowed(NAMES[index]),
            kills: KILLS[index],
            deaths: DEATHS[index],
            team: Team::from_index(usize::from(index >= 6)),
            kind: KINDS[index % 3],
            human: index == 0,
        })
        .collect();
    let results = phase == MatchPhase::Results;
    let mut ammo = AMMO_ORDER.map(|weapon| HudAmmo {
        weapon,
        count: weapon.special().map(|_| 0.0),
        selected: false,
        available: weapon == Weapon::Standard,
    });
    ammo[1] = HudAmmo {
        count: Some(6.0),
        available: true,
        ..ammo[1]
    };
    ammo[2] = HudAmmo {
        count: Some(3.0),
        selected: true,
        available: true,
        ..ammo[2]
    };
    let mut hud = Hud {
        elapsed: 312.0,
        game_mode: if solo { GameMode::Solo } else { GameMode::Team },
        map_name: "Pine Village".into(),
        active_enemies: 6,
        human: HudHuman {
            id: 1,
            kind: VehicleKind::Balanced,
            vehicle_name: "BRUISER",
            alive,
            hp: 100.0 * ratio,
            max_hp: 100.0,
            health_ratio: ratio,
            rank: 2,
            rank_name: "Veteran",
            rank_damage: 1.1,
            rank_fire_rate: 1.08,
            rank_health: 1.1,
            rank_repair: 0.02,
            repair_delay: 4.0,
            ammo,
            mine_cooldown: 4.2,
            shield: 9.0,
            shield_points: 24.0,
            rapid: 6.0,
            respawn: 2.4,
            kills: 7,
            deaths: 3,
            self_repair: true,
            ..HudHuman::default()
        },
        scoreboard: Scoreboard::Kept(rows),
        ..Hud::default()
    };
    hud.match_state.phase = phase;
    hud.match_state.time = 187.4;
    hud.match_state.scores = if solo { [0, 0] } else { [42, 37] };
    hud.match_state.ended_early = Some(results && !solo);
    hud.match_state.winner = (!solo).then_some(Team::Blue);
    hud.match_state.round = 1;
    if results {
        let mut recap = RoundRecap {
            stats: metrics(&[
                (Metric::Kills, 7.0),
                (Metric::Damage, 1840.0),
                (Metric::BestLife, 4.0),
                (Metric::Rank, 2.0),
                (Metric::LongestLife, 154.0),
                (Metric::Multikill, 3.0),
                (Metric::ClutchKills, 1.0),
                (Metric::MineKills, 1.0),
                (Metric::Pickups, 5.0),
            ]),
            best: metrics(&[
                (Metric::Kills, 11.0),
                (Metric::Damage, 1500.0),
                (Metric::BestLife, 5.0),
                (Metric::LongestLife, 200.0),
            ]),
            improved: vec![Metric::Damage],
            persisted: true,
            feats: vec![Feat {
                title: "Last stand".into(),
                detail: "A kill at 12% hull".into(),
            }],
            shots: 31,
            direct_hits: 19,
            damage_taken: 260.0,
            shield_absorbed: 40.0,
            rank_names: vec!["Rookie", "Gunner", "Veteran", "Ace"],
            ..RoundRecap::default()
        };
        recap.report = battle_report(&hud, &recap, Shell::Desktop);
        hud.recap = Some(Cow::Owned(recap));
    }
    hud
}

fn feedback(state: &mut UiState) {
    state.kill_feed = vec![
        KillFeedEntry {
            killer: "YOU".into(),
            victim: "Tusk".into(),
            killer_team: Some(Team::Blue),
            victim_team: Some(Team::Red),
            cause: Some(DamageCause::Rocket),
            own_kill: true,
            own_death: false,
            age: 0.8,
        },
        KillFeedEntry {
            killer: "Crank".into(),
            victim: "Dozer".into(),
            killer_team: Some(Team::Red),
            victim_team: Some(Team::Blue),
            cause: Some(DamageCause::Standard),
            own_kill: false,
            own_death: false,
            age: 2.0,
        },
        KillFeedEntry {
            killer: "YARD".into(),
            victim: "Biscuit".into(),
            killer_team: None,
            victim_team: Some(Team::Red),
            cause: Some(DamageCause::Drum),
            own_kill: false,
            own_death: false,
            age: 3.0,
        },
    ];
    state.toast = Some(Toast {
        text: "+3 ROCKETS".into(),
        tone: ToastTone::Pickup,
        age: 0.5,
        duration: 1.5,
    });
    state.ammo_notice = Some(("Out of spread shells: back to Standard".into(), 0.5));
    state.damage_ring = Some((0.9, 0.3));
    state.viewer = Some((0.0, -0.1));
    state.effect_longest = [0.0, 15.0, 10.0, 0.0, 0.0];
    state.mine_total = 8.0;
    state.hull_ghost = 0.62;
}

struct Scenario {
    name: &'static str,
    in_round: bool,
    phase: MatchPhase,
    state: UiState,
}

fn scenarios() -> Vec<Scenario> {
    let mut list = Vec::new();
    let mut setup = UiState {
        hud: Some(hud(MatchPhase::Ready, false, true, 1.0)),
        status: "Shaders loaded: 31 of 40".into(),
        ..UiState::default()
    };
    setup.ready = false;
    list.push(Scenario {
        name: "battle-setup",
        in_round: false,
        phase: MatchPhase::Ready,
        state: setup,
    });
    list.push(Scenario {
        name: "battle-setup-ready",
        in_round: false,
        phase: MatchPhase::Ready,
        state: UiState {
            hud: Some(hud(MatchPhase::Ready, false, true, 1.0)),
            ready: true,
            ..UiState::default()
        },
    });
    let room = |code: &str, map: &str, players: u32, reserved: u32, phase: &str| {
        crate::directory::RoomListing {
            room: code.into(),
            content_version: sloppy_core::net::protocol::CONTENT_VERSION.into(),
            map_mode: map.into(),
            difficulty: "normal".into(),
            humans_only: players > 3,
            round_minutes: 20,
            players,
            reserved,
            phase: phase.into(),
            round_id: 1,
            time: 754.0,
            scores: vec![14, 9],
            server: "wss://example.test".into(),
        }
    };
    for (name, rooms) in [
        ("multiplayer-empty", Vec::new()),
        (
            "multiplayer-rooms",
            vec![
                room("KQ7M2XHP", "harbor", 8, 8, "playing"),
                room("TR4WNB9C", "village", 3, 3, "playing"),
                room("ZP2HDA7L", "quarry", 1, 1, "lobby"),
            ],
        ),
    ] {
        list.push(Scenario {
            name,
            in_round: false,
            phase: MatchPhase::Ready,
            state: UiState {
                hud: Some(hud(MatchPhase::Ready, false, true, 1.0)),
                ready: true,
                rooms: super::rooms::answered(rooms),
                ..UiState::default()
            },
        });
    }
    let mut playing = UiState {
        hud: Some(hud(MatchPhase::Playing, false, true, 0.48)),
        ..UiState::default()
    };
    feedback(&mut playing);
    list.push(Scenario {
        name: "hud",
        in_round: true,
        phase: MatchPhase::Playing,
        state: playing,
    });
    let mut critical = UiState {
        hud: Some(hud(MatchPhase::Playing, true, true, 0.18)),
        ..UiState::default()
    };
    feedback(&mut critical);
    critical.cockpit = true;
    critical.hull_angle = 0.6;
    critical.hit_flash = Some(0.1);
    critical.toast = None;
    list.push(Scenario {
        name: "hud-solo-first-person-critical",
        in_round: true,
        phase: MatchPhase::Playing,
        state: critical,
    });
    let mut dead = UiState {
        hud: Some(hud(MatchPhase::Playing, false, false, 0.0)),
        ..UiState::default()
    };
    feedback(&mut dead);
    dead.respawn_total = 4.0;
    dead.toast = None;
    dead.death = Some(DeathReport {
        killer: Some("Crank".into()),
        own_goal: false,
        cause: Some(DamageCause::Rocket),
        weapon: Some("Rocket blast"),
        killer_vehicle: Some("BIG RIG"),
        killer_team: Some(Team::Red),
        killer_kills: Some(3),
        sentence: "Crank destroyed you with a rocket blast.".into(),
    });
    list.push(Scenario {
        name: "respawn",
        in_round: true,
        phase: MatchPhase::Playing,
        state: dead,
    });
    list.push(Scenario {
        name: "pause",
        in_round: true,
        phase: MatchPhase::Paused,
        state: UiState {
            hud: Some(hud(MatchPhase::Paused, false, true, 0.7)),
            ..UiState::default()
        },
    });
    list.push(Scenario {
        name: "pause-controls",
        in_round: true,
        phase: MatchPhase::Paused,
        state: UiState {
            hud: Some(hud(MatchPhase::Paused, false, true, 0.7)),
            pause_tab: PauseTab::Controls,
            ..UiState::default()
        },
    });
    list.push(Scenario {
        name: "settings",
        in_round: true,
        phase: MatchPhase::Paused,
        state: UiState {
            hud: Some(hud(MatchPhase::Paused, false, true, 0.7)),
            settings_draft: Some(Preferences::default()),
            ..UiState::default()
        },
    });
    list.push(Scenario {
        name: "results-team",
        in_round: true,
        phase: MatchPhase::Results,
        state: UiState {
            hud: Some(hud(MatchPhase::Results, false, true, 0.7)),
            ..UiState::default()
        },
    });
    list.push(Scenario {
        name: "results-solo",
        in_round: true,
        phase: MatchPhase::Results,
        state: UiState {
            hud: Some(hud(MatchPhase::Results, true, false, 0.0)),
            ..UiState::default()
        },
    });
    let stats = json!({
        "fps": 60, "frameMs": 16.7, "simMs": 0.82, "renderMs": 2.31, "graphicsApi": "WebGPU",
        "quality": {"setting": "auto", "tier": "high", "scale": 1.0, "scene": [2560, 1600], "gpuMs": 4.2, "renderRatio": 2.0, "changes": 0},
        "drawCalls": 412, "shadowDrawCalls": 96, "reflectionDrawCalls": 0, "triangles": 812345,
        "shadowTriangles": 210000, "reflectionTriangles": 0, "mainTriangles": 602345,
        "pipelines": 58, "latePipelines": 0, "meshes": 340, "textures": 61,
        "gpuBytes": 187654321.0, "meshSlackBytes": 3456789.0, "bodies": 412, "fixedBodies": 300,
        "dynamicBodies": 112, "sleepingBodies": 70, "colliders": 640, "tanksAlive": 11, "tanks": 12,
        "mines": 3, "pickupsReady": 4, "pickups": 6, "shots": 9, "particles": 230,
        "fragments": 12, "maxFragments": 80, "elapsed": 187.4, "pixelRatio": 2,
    });
    let mut nerd = UiState {
        hud: Some(hud(MatchPhase::Playing, false, true, 0.9)),
        show_nerd_stats: true,
        stats: Some(stats.clone()),
        ..UiState::default()
    };
    nerd.hull_ghost = 0.9;
    list.push(Scenario {
        name: "hud-nerd-stats",
        in_round: true,
        phase: MatchPhase::Playing,
        state: nerd,
    });
    let summary = |count: u32, mean: f64, p95: f64, max: f64| serde_json::json!({"count": count, "mean": mean, "p50": mean, "p95": p95, "max": max});
    let mut room_stats = stats.clone();
    room_stats["scene"] = serde_json::json!({
        "tanks": 8, "alive": 7, "mines": 2, "pickups": 6, "pickupsReady": 5, "shots": 4,
        "fragments": 9, "elapsed": 412.6,
    });
    room_stats["network"] = serde_json::json!({
        "rttMs": 38.4, "receivedUpdates": 12408, "snapshotAgeMs": 21.0, "bufferMs": 62.0,
        "marginMs": 18.0, "underrun": 0.004, "serverTick": 24756, "inputSeq": 24790,
        "inputAck": 24781, "connected": true, "lateBatches": 3, "longestBatchGapMs": 71.0,
        "predictionLeadMs": 54.0, "correctionMPerS": 0.031, "correctionP95M": 0.042,
        "inputStarts": 211, "lateInputStarts": 2, "receiveMs": 2480.0, "corrections": 96,
        "confirmedSnapshots": 12390, "phantomStrikes": 0,
        "correctionCauses": {
            "blast": {"count": 12, "mPerS": 0.012}, "tank": {"count": 30, "mPerS": 0.009},
            "cover": {"count": 4, "mPerS": 0.003}, "debris": {"count": 9, "mPerS": 0.004},
            "crate": {"count": 1, "mPerS": 0.001}, "lateInput": {"count": 2, "mPerS": 0.001},
            "other": {"count": 38, "mPerS": 0.001},
        },
    });
    room_stats["feel"] = serde_json::json!({
        "viewDelayMs": summary(240, 91.0, 104.0, 130.0),
        "fireToFlashMs": summary(46, 17.0, 25.0, 33.0),
        "fireToReportMs": summary(46, 58.0, 70.0, 88.0),
        "unansweredPresses": 0,
        "hitOffsetM": summary(21, 0.08, 0.19, 0.31),
        "incomingOffsetM": summary(14, 0.11, 0.24, 0.4),
        "incomingBehindM": summary(14, 0.35, 0.62, 0.9),
        "unseenIncoming": 1,
        "ownShots": {"predicted": 46, "confirmed": 45, "unconfirmedShells": 1, "unpredicted": 0,
            "phantomStrikes": 0, "errorM": summary(45, 0.03, 0.07, 0.12)},
    });
    let mut room_nerd = UiState {
        hud: Some(hud(MatchPhase::Playing, false, true, 0.9)),
        show_nerd_stats: true,
        stats: Some(room_stats),
        ..UiState::default()
    };
    room_nerd.hull_ghost = 0.9;
    // Two readings a second apart give the per-second rows their rates.
    let latest = room_nerd.stats.clone();
    let mut earlier = latest.clone().expect("the room has stats");
    earlier["network"]["receivedUpdates"] = 12_378.into();
    earlier["network"]["receiveMs"] = 2_478.0.into();
    room_nerd.stats = Some(earlier);
    super::nerd_stats::sample_network(&mut room_nerd, 0.0);
    room_nerd.stats = latest;
    super::nerd_stats::sample_network(&mut room_nerd, 1_000.0);
    list.push(Scenario {
        name: "room-nerd-stats",
        in_round: true,
        phase: MatchPhase::Playing,
        state: room_nerd,
    });
    for name in [
        "room-lobby",
        "room-lobby-guest",
        "room-lobby-rules",
        "room-lobby-choice",
        "room-pause",
        "room-results",
        "room-joining",
        "room-reconnecting",
        "room-connection-ended",
        "room-respawn",
        "room-controls",
        "room-settings",
    ] {
        let menu = super::room_menu::snapshot_menu(name);
        let phase = super::room_menu::hud_phase(&menu);
        list.push(Scenario {
            name,
            in_round: menu.in_battle(),
            phase,
            state: UiState {
                hud: Some(hud(
                    phase,
                    false,
                    name != "room-respawn",
                    if name == "room-respawn" { 0.0 } else { 0.7 },
                )),
                settings_draft: (name == "room-settings").then(Preferences::default),
                ..UiState::default()
            },
        });
    }
    list
}

#[test]
#[ignore = "needs a GPU; writes PNGs to SLOPPY_UI_SNAPSHOTS"]
fn snapshots() {
    let Ok(directory) = std::env::var("SLOPPY_UI_SNAPSHOTS") else {
        return;
    };
    std::fs::create_dir_all(&directory).expect("the snapshot folder");
    let size = std::env::var("SLOPPY_UI_SIZE")
        .ok()
        .and_then(|value| {
            let (width, height) = value.split_once('x')?;
            Some([width.parse::<f32>().ok()?, height.parse::<f32>().ok()?])
        })
        .unwrap_or(SIZE);
    assert!(
        size.iter()
            .all(|value| value.is_finite() && (320.0..=4096.0).contains(value))
    );
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("a GPU adapter");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("a GPU device");
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let width = (size[0] * SCALE) as u32;
    let height = (size[1] * SCALE) as u32;
    let prefs = Preferences {
        debug: false,
        player_name: "Driver".into(),
        ..Preferences::default()
    };
    for mut scenario in scenarios() {
        let is_room = scenario.name.starts_with("room-");
        let mut room_menu = is_room.then(|| super::room_menu::snapshot_menu(scenario.name));
        let state = &scenario.state;
        let fixture = json!({
            "name": scenario.name,
            "room": room_menu.as_ref().map(super::room_menu::RoomMenu::snapshot_fixture),
            "rooms": state.rooms.snapshot_rooms(),
            "playerName": state.rooms.snapshot_player_name(),
            "size": size,
            "scale": SCALE,
            "hud": state.hud,
            "pause": state.hud.as_ref().map(pause_report),
            "stats": state.stats,
            "networkSample": state.network_sample,
            "ready": state.ready,
            "status": state.status,
            "cockpit": state.cockpit,
            "hullAngle": state.hull_angle,
            "hullGhost": state.hull_ghost,
            "respawnTotal": state.respawn_total,
            "effectLongest": state.effect_longest,
            "mineTotal": state.mine_total,
            "ammoNotice": state.ammo_notice,
            "hitFlash": state.hit_flash,
            "damageRing": state.damage_ring,
            "viewer": state.viewer,
            "death": state.death.as_ref().map(|death| json!({
                "killer": death.killer, "self": death.own_goal, "cause": death.cause,
                "weapon": death.weapon, "killerVehicle": death.killer_vehicle,
                "killerTeam": death.killer_team, "killerKills": death.killer_kills,
            })),
            "toast": state.toast.as_ref().map(|toast| json!({
                "text": toast.text, "tone": match toast.tone {
                    ToastTone::Pickup => "pickup", ToastTone::Promotion => "promotion", ToastTone::Death => "death",
                }, "age": toast.age, "duration": toast.duration,
            })),
            "feed": state.kill_feed.iter().map(|entry| json!({
                "killer": entry.killer, "victim": entry.victim, "killerTeam": entry.killer_team,
                "victimTeam": entry.victim_team, "cause": entry.cause, "ownKill": entry.own_kill,
                "ownDeath": entry.own_death, "age": entry.age,
            })).collect::<Vec<_>>(),
        });
        std::fs::write(
            std::path::Path::new(&directory).join(format!("{}.json", scenario.name)),
            serde_json::to_vec_pretty(&fixture).expect("a fixture"),
        )
        .expect("the fixture saved");
        let context = egui::Context::default();
        super::theme::install(&context);
        let mut painter = egui_wgpu::Renderer::new(&device, format, Default::default());
        let view = View {
            in_round: scenario.in_round,
            phase: scenario.phase,
            prefs: &prefs,
            build: "v0.1.0 · dev",
            // Stats for nerds show only with the debug tools on.
            debug: scenario.name.ends_with("nerd-stats"),
            room: is_room,
        };
        // A few passes: new areas size themselves on their first frame.
        let mut output = None;
        for frame in 0..30 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(size[0], size[1]),
                )),
                time: Some(10.0 + f64::from(frame) / 30.0),
                ..Default::default()
            };
            context.set_pixels_per_point(SCALE);
            output = Some(context.run_pass(input, |context| {
                if let Some(menu) = &mut room_menu {
                    super::room_menu::draw(context, menu, &mut scenario.state, &view, 0.0);
                } else {
                    super::draw(context, &mut scenario.state, &view);
                }
            }));
            if let Some(output) = &mut output {
                for (id, deltas) in &output.textures_delta.set {
                    for delta in deltas {
                        painter.update_texture(&device, &queue, *id, delta);
                    }
                }
                output.textures_delta.clear();
            }
        }
        let output = output.expect("a pass ran");
        let primitives = context.tessellate(output.shapes, SCALE);
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("snapshot"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let texture_view = target.create_view(&Default::default());
        let screen = ScreenDescriptor {
            size_in_pixels: [width, height],
            pixels_per_point: SCALE,
        };
        let mut encoder = device.create_command_encoder(&Default::default());
        let mut buffers =
            painter.update_buffers(&device, &queue, &mut encoder, &primitives, &screen);
        {
            // The arena's stand-in: a muted field colour.
            let arena = if scenario.in_round {
                wgpu::Color {
                    r: 0.16,
                    g: 0.22,
                    b: 0.13,
                    a: 1.0,
                }
            } else {
                wgpu::Color {
                    r: 0.004,
                    g: 0.008,
                    b: 0.02,
                    a: 1.0,
                }
            };
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("snapshot"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &texture_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(arena),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            painter.render(&mut pass.forget_lifetime(), &primitives, &screen);
        }
        let row = (width * 4).next_multiple_of(256);
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("snapshot readback"),
            size: u64::from(row * height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        buffers.push(encoder.finish());
        queue.submit(buffers);
        readback.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            })
            .expect("the GPU finished");
        let mapped = readback
            .slice(..)
            .get_mapped_range()
            .expect("the readback mapped");
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            let start = (y * row) as usize;
            pixels.extend_from_slice(&mapped[start..start + (width * 4) as usize]);
        }
        let path = std::path::Path::new(&directory).join(format!("{}.png", scenario.name));
        image::RgbaImage::from_raw(width, height, pixels)
            .expect("a whole image")
            .save(&path)
            .expect("the PNG saved");
    }
}
