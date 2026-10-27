//! Pictures of every screen for looking at them without playing to them: each scenario
//! fabricates a HUD state, lays the screens out with egui and draws them with egui-wgpu
//! into a PNG over a flat arena-coloured backdrop. It needs a GPU, so it is ignored by
//! default: `SLOPPY_UI_SNAPSHOTS=<dir> cargo test -p sloppy-desktop --release
//! snapshots -- --ignored`.

use egui_wgpu::ScreenDescriptor;
use serde_json::json;
use sloppy_render::wgpu;

use super::{DeathReport, KillFeedEntry, PauseTab, Toast, ToastTone, UiState, View};
use crate::engine_api::{HudState, Phase};
use crate::preferences::Preferences;

/// The window's size in points, and its pixels per point.
const SIZE: [f32; 2] = [1280.0, 800.0];
const SCALE: f32 = 2.0;

fn hud(phase: &str, solo: bool, alive: bool, ratio: f64) -> HudState {
    const NAMES: [&str; 12] = [
        "YOU", "Rusty", "Dozer", "Spud", "Gravel", "Boomer", "Mortar", "Tusk", "Crank", "Biscuit",
        "Hatch", "Dent",
    ];
    const KILLS: [u32; 12] = [7, 9, 3, 2, 1, 0, 8, 4, 4, 2, 1, 0];
    const DEATHS: [u32; 12] = [3, 2, 4, 5, 2, 6, 3, 3, 4, 5, 2, 7];
    const KINDS: [&str; 3] = ["balanced", "heavy", "scout"];
    let rows: Vec<_> = (0..12)
        .map(|index| {
            let (name, kills, deaths) = (NAMES[index], KILLS[index], DEATHS[index]);
            let (team, kind) = (u8::from(index >= 6), KINDS[index % 3]);
            json!({
                "id": index + 1,
                "name": name,
                "kills": kills,
                "deaths": deaths,
                "team": team,
                "kind": kind,
                "human": index == 0,
            })
        })
        .collect();
    let value = json!({
        "match": {
            "phase": phase,
            "time": 187.4,
            "scores": [if solo { 0 } else { 42 }, if solo { 0 } else { 37 }],
            "overtime": false,
            "endedEarly": phase == "results" && !solo,
            "winner": if solo { serde_json::Value::Null } else { json!(0) },
            "round": 1,
        },
        "elapsed": 312.0,
        "gameMode": if solo { "solo" } else { "team" },
        "endlessMatch": false,
        "mapMode": "village",
        "mapName": "Pine Village",
        "difficulty": "normal",
        "humanTeam": 0,
        "scoreLimit": 100,
        "teamNames": ["BLUE", "RED"],
        "activeEnemies": 6,
        "speedTuning": {"tank-speed": 1.0, "bullet-speed": 1.0},
        "human": {
            "id": 1,
            "kind": "balanced",
            "vehicleName": "BRUISER",
            "alive": alive,
            "hp": 100.0 * ratio,
            "maxHp": 100.0,
            "healthRatio": ratio,
            "rank": 2,
            "rankName": "Veteran",
            "rankDamage": 1.1,
            "rankFireRate": 1.08,
            "rankHealth": 1.1,
            "rankRepair": 0.02,
            "repairDelay": 4,
            "ammo": [
                {"weapon": "standard", "count": null, "selected": false, "available": true},
                {"weapon": "spread", "count": 6, "selected": false, "available": true},
                {"weapon": "rocket", "count": 3, "selected": true, "available": true},
                {"weapon": "ricochet", "count": 0, "selected": false, "available": false},
                {"weapon": "piercing", "count": 0, "selected": false, "available": false},
            ],
            "mineCooldown": 4.2,
            "protection": 0.0,
            "shield": 9.0,
            "shieldPoints": 24.0,
            "rapid": 6.0,
            "speed": 0.0,
            "laser": 0.0,
            "respawn": 2.4,
            "kills": 7,
            "deaths": 3,
            "selfRepair": true,
        },
        "scoreboard": rows,
        "recap": if phase == "results" { json!({
            "stats": {"kills": 7, "damage": 1840, "bestLife": 4, "rank": 2, "longestLife": 154,
                      "multikill": 3, "clutchKills": 1, "mineKills": 1, "pickups": 5},
            "best": {"kills": 11, "damage": 1500, "bestLife": 5, "longestLife": 200},
            "improved": ["damage"],
            "persisted": true,
            "feats": [{"title": "Last stand", "detail": "A kill at 12% hull"}],
            "shots": 31, "directHits": 19, "damageTaken": 260, "shieldAbsorbed": 40,
            "rankNames": ["Rookie", "Gunner", "Veteran", "Ace"],
        }) } else { serde_json::Value::Null },
    });
    serde_json::from_value(value).expect("a HUD state")
}

fn feedback(state: &mut UiState) {
    state.kill_feed = vec![
        KillFeedEntry {
            killer: "YOU".into(),
            victim: "Tusk".into(),
            killer_team: Some(0),
            victim_team: Some(1),
            cause: Some("rocket".into()),
            own_kill: true,
            own_death: false,
            age: 0.8,
        },
        KillFeedEntry {
            killer: "Crank".into(),
            victim: "Dozer".into(),
            killer_team: Some(1),
            victim_team: Some(0),
            cause: Some("standard".into()),
            own_kill: false,
            own_death: false,
            age: 2.0,
        },
        KillFeedEntry {
            killer: "YARD".into(),
            victim: "Biscuit".into(),
            killer_team: None,
            victim_team: Some(1),
            cause: Some("drum".into()),
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
    phase: Phase,
    state: UiState,
}

fn scenarios() -> Vec<Scenario> {
    let mut list = Vec::new();
    let mut setup = UiState {
        hud: Some(hud("ready", false, true, 1.0)),
        status: "Shaders loaded: 31 of 40".into(),
        ..UiState::default()
    };
    setup.ready = false;
    list.push(Scenario {
        name: "battle-setup",
        in_round: false,
        phase: Phase::Ready,
        state: setup,
    });
    let mut playing = UiState {
        hud: Some(hud("playing", false, true, 0.48)),
        ..UiState::default()
    };
    feedback(&mut playing);
    list.push(Scenario {
        name: "hud",
        in_round: true,
        phase: Phase::Playing,
        state: playing,
    });
    let mut critical = UiState {
        hud: Some(hud("playing", true, true, 0.18)),
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
        phase: Phase::Playing,
        state: critical,
    });
    let mut dead = UiState {
        hud: Some(hud("playing", false, false, 0.0)),
        ..UiState::default()
    };
    feedback(&mut dead);
    dead.respawn_total = 4.0;
    dead.toast = None;
    dead.death = Some(DeathReport {
        killer: Some("Crank".into()),
        own_goal: false,
        cause: Some("rocket".into()),
        weapon: Some("Rocket blast".into()),
        killer_vehicle: Some("BIG RIG".into()),
        killer_team: Some(1),
        killer_kills: Some(3.0),
        sentence: "Crank destroyed you with a rocket blast.".into(),
    });
    list.push(Scenario {
        name: "respawn",
        in_round: true,
        phase: Phase::Playing,
        state: dead,
    });
    list.push(Scenario {
        name: "pause",
        in_round: true,
        phase: Phase::Paused,
        state: UiState {
            hud: Some(hud("paused", false, true, 0.7)),
            ..UiState::default()
        },
    });
    list.push(Scenario {
        name: "pause-controls",
        in_round: true,
        phase: Phase::Paused,
        state: UiState {
            hud: Some(hud("paused", false, true, 0.7)),
            pause_tab: PauseTab::Controls,
            ..UiState::default()
        },
    });
    list.push(Scenario {
        name: "settings",
        in_round: true,
        phase: Phase::Paused,
        state: UiState {
            hud: Some(hud("paused", false, true, 0.7)),
            settings_draft: Some(Preferences {
                nerd_stats: true,
                ..Preferences::default()
            }),
            ..UiState::default()
        },
    });
    list.push(Scenario {
        name: "results-team",
        in_round: true,
        phase: Phase::Results,
        state: UiState {
            hud: Some(hud("results", false, true, 0.7)),
            ..UiState::default()
        },
    });
    list.push(Scenario {
        name: "results-solo",
        in_round: true,
        phase: Phase::Results,
        state: UiState {
            hud: Some(hud("results", true, false, 0.0)),
            ..UiState::default()
        },
    });
    let stats = json!({
        "fps": 60, "frameMs": 16.7, "simMs": 0.82, "renderMs": 2.31, "graphicsApi": "WebGPU",
        "quality": {"setting": "auto", "tier": "high", "scale": 1.0, "scene": [2560, 1600], "gpuMs": 4.2},
        "drawCalls": 412, "shadowDrawCalls": 96, "reflectionDrawCalls": 0, "triangles": 812345,
        "shadowTriangles": 210000, "reflectionTriangles": 0, "mainTriangles": 602345,
        "pipelines": 58, "latePipelines": 0, "meshes": 340, "textures": 61,
        "gpuBytes": 187654321.0, "meshSlackBytes": 3456789.0, "bodies": 412, "fixedBodies": 300,
        "dynamicBodies": 112, "sleepingBodies": 70, "colliders": 640, "tanksAlive": 11, "tanks": 12,
        "mines": 3, "pickupsReady": 4, "pickups": 6, "shots": 9, "particles": 230,
        "fragments": 12, "maxFragments": 80, "elapsed": 187.4, "pixelRatio": 2,
    });
    let mut nerd = UiState {
        hud: Some(hud("playing", false, true, 0.9)),
        show_nerd_stats: true,
        stats: Some(stats),
        ..UiState::default()
    };
    nerd.hull_ghost = 0.9;
    list.push(Scenario {
        name: "hud-nerd-stats",
        in_round: true,
        phase: Phase::Playing,
        state: nerd,
    });
    list
}

#[test]
#[ignore = "needs a GPU; writes PNGs to SLOPPY_UI_SNAPSHOTS"]
fn snapshots() {
    let Ok(directory) = std::env::var("SLOPPY_UI_SNAPSHOTS") else {
        return;
    };
    std::fs::create_dir_all(&directory).expect("the snapshot folder");
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .expect("a GPU adapter");
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("a GPU device");
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let width = (SIZE[0] * SCALE) as u32;
    let height = (SIZE[1] * SCALE) as u32;
    let prefs = Preferences {
        nerd_stats: true,
        ..Preferences::default()
    };
    for mut scenario in scenarios() {
        let context = egui::Context::default();
        super::theme::install(&context);
        let mut painter = egui_wgpu::Renderer::new(&device, format, Default::default());
        let view = View {
            in_round: scenario.in_round,
            phase: scenario.phase,
            prefs: &prefs,
            build: "v0.1.0 · dev",
            debug: false,
            room: false,
        };
        // A few passes: new areas size themselves on their first frame.
        let mut output = None;
        for frame in 0..30 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(SIZE[0], SIZE[1]),
                )),
                time: Some(10.0 + f64::from(frame) / 30.0),
                ..Default::default()
            };
            context.set_pixels_per_point(SCALE);
            output = Some(context.run_pass(input, |context| {
                super::draw(context, &mut scenario.state, &view);
            }));
            if let Some(output) = &output {
                for (id, deltas) in &output.textures_delta.set {
                    for delta in deltas {
                        painter.update_texture(&device, &queue, *id, delta);
                    }
                }
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
