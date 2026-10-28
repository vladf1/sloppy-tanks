//! The screens' artwork: the page's SVG icons (`HUD_ICONS` and `CAUSE_ICONS` in
//! `ui-markup.ts`, Battle Setup's symbols and map art) rasterized once into egui
//! textures, and Battle Setup's tank renders (`previews/tanks.webp`). Shapes rather
//! than font glyphs, since the system's text faces lack most symbols.

use std::collections::HashMap;

use egui::{Color32, ColorImage, Pos2, Rect, TextureHandle, TextureOptions, pos2};

/// How a layer of an icon is painted.
#[derive(Clone, Copy)]
enum Paint {
    /// Filled with SVG's default nonzero rule.
    Fill,
    /// Filled with `fill-rule="evenodd"` (holes cut by inner rings).
    EvenOdd,
    /// Stroked this wide, with round joins and caps.
    Stroke(f32),
}

/// One `<path>` of an icon, in its colour (white for icons the screens tint).
struct Layer {
    paint: Paint,
    color: u32,
    path: &'static str,
}

const fn fill(path: &'static str) -> Layer {
    Layer {
        paint: Paint::Fill,
        color: 0xffffff,
        path,
    }
}

const fn even_odd(path: &'static str) -> Layer {
    Layer {
        paint: Paint::EvenOdd,
        color: 0xffffff,
        path,
    }
}

const fn stroke(width: f32, path: &'static str) -> Layer {
    Layer {
        paint: Paint::Stroke(width),
        color: 0xffffff,
        path,
    }
}

const fn colored(color: u32, path: &'static str) -> Layer {
    Layer {
        paint: Paint::Fill,
        color,
        path,
    }
}

/// An icon: its name, SVG view box (x, y, width, height), texels per unit and layers.
struct Source {
    name: &'static str,
    view: [f32; 4],
    scale: f32,
    layers: &'static [Layer],
}

/// Texels per unit of the 24-unit icons: crisp at twice their largest size in points.
const ICON_SCALE: f32 = 3.0;
const ICON_VIEW: [f32; 4] = [0.0, 0.0, 24.0, 24.0];

const fn icon(name: &'static str, layers: &'static [Layer]) -> Source {
    Source {
        name,
        view: ICON_VIEW,
        scale: ICON_SCALE,
        layers,
    }
}

/// A map card's landscape (`.map-row .choice-card` in `style.css`), right half only.
const MAP_ART_VIEW: [f32; 4] = [60.0, 0.0, 180.0, 80.0];

const SOURCES: &[Source] = &[
    // The ammunition and kill-feed weapons: every one points right, the way the feed
    // reads from killer to victim.
    icon(
        "standard",
        &[fill(
            "M5 9h8.6c3.2 0 6 1.4 7.9 3-1.9 1.6-4.7 3-7.9 3H5zM2 8.4h2v7.2H2z",
        )],
    ),
    icon(
        "spread",
        &[fill(
            "M4 10.6h7.3c2.3 0 4.2.6 5.6 1.4-1.4.8-3.3 1.4-5.6 1.4H4zM5.1 3.6l6.8 2.6c2.2.8 3.8 2 4.8 3.2-1.6.3-3.6.2-5.7-.6L4.2 6.2zM4.2 17.8l6.8-2.6c2.1-.8 4.1-.9 5.7-.6-1 1.2-2.6 2.4-4.8 3.2l-6.8 2.6z",
        )],
    ),
    icon(
        "rocket",
        &[fill(
            "M6 10h8l1.6-1.4h1.2c2.4 0 4.4 1.6 5.7 3.4-1.3 1.8-3.3 3.4-5.7 3.4h-1.2L14 14H6zM7 10 4 5h3.4L11 10zM7 14l-3 5h3.4L11 14zM1.5 11h3.5v2H1.5z",
        )],
    ),
    icon(
        "ricochet",
        &[fill(
            "M2.6 5.4 8.4 15l5.2-7.6 3.5 5.1 1.6-1.1.9 6.3-5.9-2.2 1.6-1.1-1.7-2.5-5.3 7.7L.7 6.6z",
        )],
    ),
    icon(
        "piercing",
        &[fill(
            "M2 10.8h13.2V7.4L22.5 12l-7.3 4.6v-3.4H2zM8.2 3h2.6v6.3H8.2zM8.2 14.7h2.6V21H8.2z",
        )],
    ),
    icon(
        "mine",
        &[fill(
            "M12 6.2a6.3 6.3 0 1 1 0 12.6 6.3 6.3 0 0 1 0-12.6zM10.9 2.4h2.2v3.2h-2.2zM10.9 19.4h2.2v3.2h-2.2zM2 11.4h3.2v2.2H2zM18.8 11.4H22v2.2h-3.2zM4.3 5.3l1.6-1.6 2.3 2.3-1.6 1.6zM15.8 17l1.6-1.6 2.3 2.3-1.6 1.6zM4.3 19.7 6.6 17.4l1.6 1.6-2.3 2.3zM15.8 8l2.3-2.3 1.6 1.6-2.3 2.3z",
        )],
    ),
    icon(
        "drum",
        &[even_odd(
            "M6 3h12l1 2.2v13.6L18 21H6l-1-2.2V5.2zM5 8h14v1.6H5zm0 6.4h14V16H5z",
        )],
    ),
    icon(
        "blast",
        &[fill(
            "m12 1.5 2.3 5.6 5.4-2.7-1.9 5.8 5.7 1.8-5.6 2.4 2.8 5.3-5.8-1.8-1.7 5.8-2.4-5.5-5.4 2.8 1.8-5.8L1.5 13l5.6-2.4-2.7-5.3 5.7 1.8z",
        )],
    ),
    // The corner toolbar.
    icon(
        "eye",
        &[
            stroke(
                2.0,
                "M2 12s3.6-6.2 10-6.2S22 12 22 12s-3.6 6.2-10 6.2S2 12 2 12z",
            ),
            fill("M15 12a3 3 0 1 1-6 0 3 3 0 1 1 6 0z"),
        ],
    ),
    icon(
        "expand",
        &[stroke(
            2.2,
            "M3.5 9V3.5H9M15 3.5h5.5V9M20.5 15v5.5H15M9 20.5H3.5V15",
        )],
    ),
    icon(
        "collapse",
        &[stroke(
            2.2,
            "M9 3.5V9H3.5M20.5 9H15V3.5M15 20.5V15h5.5M3.5 15H9v5.5",
        )],
    ),
    icon("pause", &[fill("M6.5 4h4v16h-4zM13.5 4h4v16h-4z")]),
    icon(
        "gear",
        &[
            stroke(
                1.8,
                "M10.23 4.61L10.8 1.87h2.4l.57 2.74 2.2.91 2.34-1.53 1.7 1.7-1.53 2.34.91 2.2 2.74.57v2.4l-2.74.57-.91 2.2 1.53 2.34-1.7 1.7-2.34-1.53-2.2.91-.57 2.74h-2.4l-.57-2.74-2.2-.91-2.34 1.53-1.7-1.7 1.53-2.34-.91-2.2-2.74-.57v-2.4l2.74-.57.91-2.2-1.53-2.34 1.7-1.7 2.34 1.53z",
            ),
            stroke(1.8, "M15.2 12a3.2 3.2 0 1 1-6.4 0 3.2 3.2 0 1 1 6.4 0z"),
        ],
    ),
    icon(
        "close",
        &[fill(
            "M6.4 5 12 10.6 17.6 5 19 6.4 13.4 12l5.6 5.6-1.4 1.4-5.6-5.6L6.4 19 5 17.6l5.6-5.6L5 6.4z",
        )],
    ),
    // Effects and the tank panel.
    icon(
        "target",
        &[stroke(
            2.2,
            "M12 4.5a7.5 7.5 0 1 1 0 15 7.5 7.5 0 0 1 0-15zM12 1v6M12 17v6M1 12h6M17 12h6",
        )],
    ),
    icon(
        "shield",
        &[fill(
            "M12 1.8 21 5v6.4c0 5.4-3.7 9.4-9 11-5.3-1.6-9-5.6-9-11V5z",
        )],
    ),
    icon(
        "hull",
        &[fill(
            "M9.2 2.5h5.6a1 1 0 0 1 1 1v5.7h5.7a1 1 0 0 1 1 1v5.6a1 1 0 0 1-1 1h-5.7v5.7a1 1 0 0 1-1 1H9.2a1 1 0 0 1-1-1v-5.7H2.5a1 1 0 0 1-1-1v-5.6a1 1 0 0 1 1-1h5.7V3.5a1 1 0 0 1 1-1z",
        )],
    ),
    icon(
        "rapid",
        &[fill(
            "M2.5 5h4.2l6 7-6 7H2.5l6-7zm8.5 0h4.2l6 7-6 7H11l6-7z",
        )],
    ),
    icon(
        "boost",
        &[fill("M14.2 1.5 4 13.6h6.4L8.8 22.5 20 9.8h-6.6z")],
    ),
    icon(
        "repair",
        &[fill(
            "M15.6 2.2a6 6 0 0 0-5.9 7.6L2.6 16.9a2.2 2.2 0 0 0 3.1 3.1l7.1-7.1a6 6 0 0 0 7.6-5.9l-3.4 3.4-3.1-.9-.9-3.1z",
        )],
    ),
    icon(
        "lock",
        &[even_odd(
            "M12 2.5a5 5 0 0 1 5 5V10h1.2c.9 0 1.6.7 1.6 1.6v8.3c0 .9-.7 1.6-1.6 1.6H5.8c-.9 0-1.6-.7-1.6-1.6v-8.3c0-.9.7-1.6 1.6-1.6H7V7.5a5 5 0 0 1 5-5zm0 2.6a2.4 2.4 0 0 0-2.4 2.4V10h4.8V7.5A2.4 2.4 0 0 0 12 5.1zm0 8.2a1.7 1.7 0 0 0-.9 3.1v2h1.8v-2a1.7 1.7 0 0 0-.9-3.1z",
        )],
    ),
    icon(
        "skull",
        &[even_odd(
            "M12 2c5 0 8.5 3.4 8.5 8.1 0 2.6-1.1 4.5-3 5.6V19a1.5 1.5 0 0 1-1.5 1.5h-1.2V18h-1.6v2.5h-2.4V18H9.2v2.5H8A1.5 1.5 0 0 1 6.5 19v-3.3c-1.9-1.1-3-3-3-5.6C3.5 5.4 7 2 12 2zM8.6 9.2a2 2 0 1 0 0 4 2 2 0 0 0 0-4zm6.8 0a2 2 0 1 0 0 4 2 2 0 0 0 0-4zM12 13.4l-1.3 2.3h2.6z",
        )],
    ),
    icon(
        "star",
        &[fill(
            "M12 2l2.9 6.9 7.1.6-5.4 4.7 1.7 7.3L12 17.6l-6.3 3.9 1.7-7.3L2 9.5l7.1-.6z",
        )],
    ),
    icon(
        "infinity",
        &[stroke(
            2.4,
            "M12 12c-2-2.7-3.6-4-5.5-4a4 4 0 0 0 0 8c1.9 0 3.5-1.3 5.5-4s3.6-4 5.5-4a4 4 0 0 1 0 8c-1.9 0-3.5-1.3-5.5-4z",
        )],
    ),
    // The dialogs' button marks (`ACTION_ICONS`).
    icon("resume", &[fill("M7 4.5v15l12.5-7.5z")]),
    icon(
        "sliders",
        &[fill(
            "M4 6.5h9.2a2.8 2.8 0 0 1 5.6 0H20v2h-1.2a2.8 2.8 0 0 1-5.6 0H4zm0 9h1.2a2.8 2.8 0 0 1 5.6 0H20v2h-9.2a2.8 2.8 0 0 1-5.6 0H4z",
        )],
    ),
    icon("end", &[fill("M5 3.5h2v17H5zM8.5 4h10l-2.5 4 2.5 4h-10z")]),
    icon(
        "again",
        &[fill(
            "M12 4.5a7.5 7.5 0 1 1-7.1 9.9l2.1-.7A5.3 5.3 0 1 0 8.4 8L11 10.5H4V3.5l2.9 2.9A7.5 7.5 0 0 1 12 4.5z",
        )],
    ),
    icon(
        "setup",
        &[fill("M4 4h7v7H4zm9 0h7v7h-7zM4 13h7v7H4zm9 0h7v7h-7z")],
    ),
    icon(
        "reset",
        &[stroke(2.0, "M4 12a8 8 0 1 0 2.4-5.7M4 4v4.5h4.5")],
    ),
    icon(
        "power",
        &[stroke(2.2, "M12 3v8M7.1 6.2a7.5 7.5 0 1 0 9.8 0")],
    ),
    // Battle Setup's symbols.
    icon("team", &[stroke(2.0, "M6 21V4h11l-2.5 4 2.5 4H6")]),
    icon(
        "solo",
        &[stroke(
            2.0,
            "M19 12a7 7 0 1 1-14 0 7 7 0 1 1 14 0zM12 2v5M12 17v5M2 12h5M17 12h5",
        )],
    ),
    icon(
        "village",
        &[stroke(2.0, "M12 3 7 10h3l-4 6h12l-4-6h3zM12 16v5")],
    ),
    icon(
        "harbor",
        &[stroke(
            2.0,
            "M14 5a2 2 0 1 1-4 0 2 2 0 1 1 4 0zM12 7v14M8 11h8M5 14a7 7 0 0 0 14 0",
        )],
    ),
    icon("quarry", &[stroke(2.0, "M2 20 9 8l4 6 3-4 6 10z")]),
    icon(
        "stress",
        &[stroke(
            2.0,
            "M4 4h16v16H4zM4 9.3h16M4 14.7h16M9.3 4v16M14.7 4v16",
        )],
    ),
    icon(
        "yard",
        &[stroke(
            2.0,
            "M3 20h18M4 20v-6.5h7.5V20M12.5 20v-6.5H20V20M8.2 13.5V7h7.5v6.5",
        )],
    ),
    // The setup backdrop's two tanks on watch.
    Source {
        name: "backdrop-tank",
        view: [0.0, 0.0, 132.0, 52.0],
        scale: 3.0,
        layers: &[fill(
            "M6 36h108a8 8 0 0 1 0 16H6a8 8 0 0 1 0-16zM0 36l9-11h102l9 11zM34 25l6-13 37-2 9 6v9zM84 13h46v4H84z",
        )],
    },
    Source {
        name: "art-village",
        view: MAP_ART_VIEW,
        scale: 2.0,
        layers: &[
            colored(0x3c8a52, "M60 80c20-22 50-30 90-28s70 12 90 4v24z"),
            colored(0x2a6a40, "M100 80c25-14 60-18 90-12s40 4 50 0v12z"),
            colored(
                0x1f5534,
                "M118 32l7.14 13.5h-3.57l7.65 10.5h-4.08l6.12 6h-26.52l6.12-6h-4.08l7.65-10.5h-3.57z",
            ),
            colored(
                0x1a4a2d,
                "M132 24l8.568 16.2h-4.284l9.18 12.6h-4.896l7.344 7.2h-31.824l7.344-7.2h-4.896l9.18-12.6h-4.284z",
            ),
            colored(
                0x1a4a2d,
                "M226 24l8.092 15.3h-4.046l8.67 11.9h-4.624l6.936 6.8h-30.056l6.936-6.8h-4.624l8.67-11.9h-4.046z",
            ),
            colored(
                0x1f5534,
                "M214 36l6.188 11.7h-3.094l6.63 9.1h-3.536l5.304 5.2h-22.984l5.304-5.2h-3.536l6.63-9.1h-3.094z",
            ),
            colored(0xefe6cf, "M160 52h34v18h-34z"),
            colored(0xb8463a, "M155 53l22-15 22 15z"),
            colored(0x5a3b28, "M173 60h7v10h-7z"),
            colored(0x7fb3d9, "M163 56h6v5h-6zM185 56h6v5h-6z"),
        ],
    },
    Source {
        name: "art-harbor",
        view: MAP_ART_VIEW,
        scale: 2.0,
        layers: &[
            colored(
                0xe0aa35,
                "M150 18h4v50h-4zM176 18h4v50h-4zM144 16h66v5h-66zM196 21h3v14h-3z",
            ),
            colored(0x2b2f3a, "M190 35h14v8h-14z"),
            colored(0xc2502f, "M118 52h30v14h-30z"),
            colored(0x2f7f70, "M150 52h30v14h-30z"),
            colored(0x3b5fa8, "M134 38h30v14h-30z"),
            colored(0xd48a2a, "M182 52h30v14h-30z"),
            colored(0x1f5f99, "M60 80V68h180v12z"),
            colored(
                0x5aa6dd,
                "M90 70h22v2H90zM150 74h30v2h-30zM200 69h20v2h-20z",
            ),
        ],
    },
    Source {
        name: "art-quarry",
        view: MAP_ART_VIEW,
        scale: 2.0,
        layers: &[
            colored(0xc98f4e, "M110 80V44l14-6h30l8 10h20l10-14h28l20 12v34z"),
            colored(0xa8703a, "M110 80V58h30l6-6h40l8 8h46v20z"),
            colored(0x8a5a2e, "M150 80l6-10h24l6 4h54v6z"),
            colored(0xe3b072, "M124 38h30l-6 4h-20zM192 34h28l-8 4h-18z"),
            colored(0x7a4e28, "M128 70h8l3 4h-14zM206 66h10l2 5h-14z"),
        ],
    },
];

/// The kill-feed and notice icon for each way to destroy a tank (`CAUSE_ICONS`); a
/// kill without a reported weapon shows the generic wreck mark.
pub fn cause_icon(cause: Option<&str>) -> &'static str {
    match cause {
        Some("standard") => "standard",
        Some("spread") => "spread",
        Some("rocket" | "tow") => "rocket",
        Some("ricochet") => "ricochet",
        Some("piercing") => "piercing",
        Some("mine") => "mine",
        Some("drum") => "drum",
        Some("interception" | "explosion") => "blast",
        _ => "skull",
    }
}

/// The tank sheet's size, and one tank's cell in it: a column per tank, a row per team.
const SHEET_SIZE: [f32; 2] = [1920.0, 800.0];
const SHEET_CELL: [f32; 2] = [640.0, 400.0];
/// The window round each tank's visible bounds (Battle Setup's `viewBox`), so the
/// chassis sizes stay comparable.
const PREVIEW_SIZE: [f32; 2] = [560.0, 300.0];

/// The textures the screens draw: icons by name and the tank sheet.
pub struct Art {
    icons: HashMap<&'static str, TextureHandle>,
    tanks: Option<TextureHandle>,
}

impl Art {
    pub fn new(context: &egui::Context) -> Self {
        let icons = SOURCES
            .iter()
            .map(|source| {
                let image = rasterize(source);
                let texture = context.load_texture(
                    format!("icon {}", source.name),
                    image,
                    TextureOptions::LINEAR,
                );
                (source.name, texture)
            })
            .collect();
        let tanks = crate::assets::file("previews/tanks.webp")
            .and_then(|bytes| image::load_from_memory(bytes).ok())
            .map(|image| {
                let image = image.into_rgba8();
                let size = [image.width() as usize, image.height() as usize];
                let pixels = ColorImage::from_rgba_unmultiplied(size, image.as_raw());
                context.load_texture("tank previews", pixels, TextureOptions::LINEAR)
            });
        Self { icons, tanks }
    }

    /// Paint icon `name` into `rect`, tinted (white keeps a coloured icon's colours).
    pub fn icon(&self, painter: &egui::Painter, name: &str, rect: Rect, tint: Color32) {
        if let Some(texture) = self.icons.get(name) {
            painter.image(
                texture.id(),
                rect,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                tint,
            );
        }
    }

    /// Paint icon `name` mirrored left to right.
    pub fn icon_flipped(&self, painter: &egui::Painter, name: &str, rect: Rect, tint: Color32) {
        if let Some(texture) = self.icons.get(name) {
            let mut mesh = egui::Mesh::with_texture(texture.id());
            mesh.add_rect_with_uv(
                rect,
                Rect::from_min_max(pos2(1.0, 0.0), pos2(0.0, 1.0)),
                tint,
            );
            painter.add(egui::Shape::mesh(mesh));
        }
    }

    /// The width over height of icon `name`.
    pub fn aspect(&self, name: &str) -> f32 {
        SOURCES
            .iter()
            .find(|source| source.name == name)
            .map_or(1.0, |source| source.view[2] / source.view[3])
    }

    /// Paint the render of `kind` ("scout", "balanced", "heavy") in `team`'s paint, fit
    /// inside `rect` at `scale` of its height and centred.
    pub fn tank(&self, painter: &egui::Painter, kind: &str, team: u8, rect: Rect, scale: f32) {
        self.tank_image(painter, kind, team, rect, scale, false);
    }

    /// A tank facing the other side of a setup poster, like CSS `scaleX(-1)`.
    pub fn tank_mirrored(
        &self,
        painter: &egui::Painter,
        kind: &str,
        team: u8,
        rect: Rect,
        scale: f32,
    ) {
        self.tank_image(painter, kind, team, rect, scale, true);
    }

    fn tank_image(
        &self,
        painter: &egui::Painter,
        kind: &str,
        team: u8,
        rect: Rect,
        scale: f32,
        mirrored: bool,
    ) {
        let Some(texture) = &self.tanks else { return };
        let (column, origin) = match kind {
            "scout" => (0.0, [64.0, 38.0]),
            "heavy" => (2.0, [58.5, 32.5]),
            _ => (1.0, [75.5, 31.0]),
        };
        let min = [
            column * SHEET_CELL[0] + origin[0],
            f32::from(team.min(1)) * SHEET_CELL[1] + origin[1],
        ];
        let mut uv = Rect::from_min_max(
            pos2(min[0] / SHEET_SIZE[0], min[1] / SHEET_SIZE[1]),
            pos2(
                (min[0] + PREVIEW_SIZE[0]) / SHEET_SIZE[0],
                (min[1] + PREVIEW_SIZE[1]) / SHEET_SIZE[1],
            ),
        );
        if mirrored {
            std::mem::swap(&mut uv.min.x, &mut uv.max.x);
        }
        let aspect = PREVIEW_SIZE[0] / PREVIEW_SIZE[1];
        let mut height = rect.height() * scale;
        if height * aspect > rect.width() * scale {
            height = rect.width() * scale / aspect;
        }
        let size = egui::vec2(height * aspect, height);
        painter.image(
            texture.id(),
            Rect::from_center_size(rect.center(), size),
            uv,
            Color32::WHITE,
        );
    }
}

/// An icon's coverage, layer over layer, as an egui image.
fn rasterize(source: &Source) -> ColorImage {
    let [x, y, width, height] = source.view;
    let size = [
        (width * source.scale).ceil() as usize,
        (height * source.scale).ceil() as usize,
    ];
    let mut rgba = vec![[0.0f32; 4]; size[0] * size[1]];
    for layer in source.layers {
        let mut polygons = parse(layer.path);
        let even_odd = matches!(layer.paint, Paint::EvenOdd);
        if let Paint::Stroke(width) = layer.paint {
            polygons = outline(&polygons, width / 2.0);
        }
        let transformed: Vec<Vec<Pos2>> = polygons
            .into_iter()
            .map(|(points, _)| {
                points
                    .into_iter()
                    .map(|point| pos2((point.x - x) * source.scale, (point.y - y) * source.scale))
                    .collect()
            })
            .collect();
        let coverage = fill_coverage(&transformed, even_odd, size);
        let color = [
            ((layer.color >> 16) & 0xff) as f32 / 255.0,
            ((layer.color >> 8) & 0xff) as f32 / 255.0,
            (layer.color & 0xff) as f32 / 255.0,
        ];
        for (pixel, alpha) in rgba.iter_mut().zip(coverage) {
            let alpha = alpha.min(1.0);
            if alpha <= 0.0 {
                continue;
            }
            // Straight-alpha "over".
            let out = alpha + pixel[3] * (1.0 - alpha);
            for channel in 0..3 {
                pixel[channel] = (color[channel] * alpha
                    + pixel[channel] * pixel[3] * (1.0 - alpha))
                    / out.max(1e-6);
            }
            pixel[3] = out;
        }
    }
    let bytes: Vec<u8> = rgba
        .iter()
        .flat_map(|pixel| pixel.map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8))
        .collect();
    ColorImage::from_rgba_unmultiplied(size, &bytes)
}

/// Sub-rows per texel row: vertical antialiasing (horizontal coverage is exact).
const SUBROWS: usize = 4;

/// Each texel's covered share by `polygons` (closed implicitly).
fn fill_coverage(polygons: &[Vec<Pos2>], even_odd: bool, size: [usize; 2]) -> Vec<f32> {
    let [width, height] = size;
    let mut edges = Vec::new();
    for polygon in polygons {
        for (index, &from) in polygon.iter().enumerate() {
            let to = polygon[(index + 1) % polygon.len()];
            if from.y != to.y {
                edges.push((from, to));
            }
        }
    }
    let mut coverage = vec![0.0f32; width * height];
    let mut crossings: Vec<(f32, i32)> = Vec::new();
    for row in 0..height {
        for sub in 0..SUBROWS {
            let y = row as f32 + (sub as f32 + 0.5) / SUBROWS as f32;
            crossings.clear();
            for &(from, to) in &edges {
                let (low, high) = if from.y < to.y {
                    (from, to)
                } else {
                    (to, from)
                };
                if y >= low.y && y < high.y {
                    let x = low.x + (y - low.y) * (high.x - low.x) / (high.y - low.y);
                    crossings.push((x, if to.y > from.y { 1 } else { -1 }));
                }
            }
            crossings.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut winding = 0;
            for pair in 0..crossings.len().saturating_sub(1) {
                winding += crossings[pair].1;
                let inside = if even_odd {
                    winding % 2 != 0
                } else {
                    winding != 0
                };
                if inside {
                    let line = &mut coverage[row * width..(row + 1) * width];
                    add_span(line, crossings[pair].0, crossings[pair + 1].0);
                }
            }
        }
    }
    coverage
}

/// Add a sub-row's span from `start` to `end` to a texel row.
fn add_span(line: &mut [f32], start: f32, end: f32) {
    let start = start.max(0.0);
    let end = end.min(line.len() as f32);
    if end <= start {
        return;
    }
    let share = 1.0 / SUBROWS as f32;
    let first = start.floor() as usize;
    let last = (end.ceil() as usize).min(line.len());
    for (texel, value) in line.iter_mut().enumerate().take(last).skip(first) {
        let overlap = end.min(texel as f32 + 1.0) - start.max(texel as f32);
        if overlap > 0.0 {
            *value += overlap * share;
        }
    }
}

/// A stroke as fill polygons: a quad per segment and a disc at every point (round
/// joins and caps), all wound the same way so the nonzero rule unites them.
fn outline(paths: &[(Vec<Pos2>, bool)], half: f32) -> Vec<(Vec<Pos2>, bool)> {
    let mut polygons = Vec::new();
    for (points, closed) in paths {
        let count = points.len();
        let segments = if *closed {
            count
        } else {
            count.saturating_sub(1)
        };
        for index in 0..segments {
            let from = points[index];
            let to = points[(index + 1) % count];
            let direction = to - from;
            if direction.length() < 1e-4 {
                continue;
            }
            let normal = direction.normalized().rot90() * half;
            polygons.push(clockwise(vec![
                from + normal,
                to + normal,
                to - normal,
                from - normal,
            ]));
        }
        for &point in points {
            let disc = (0..12)
                .map(|step| {
                    let angle = step as f32 / 12.0 * std::f32::consts::TAU;
                    point + egui::vec2(angle.cos(), angle.sin()) * half
                })
                .collect();
            polygons.push(clockwise(disc));
        }
    }
    polygons
        .into_iter()
        .map(|polygon| (polygon, true))
        .collect()
}

/// `polygon` wound one fixed way.
fn clockwise(mut polygon: Vec<Pos2>) -> Vec<Pos2> {
    let area: f32 = polygon
        .iter()
        .zip(polygon.iter().cycle().skip(1))
        .map(|(a, b)| a.x * b.y - b.x * a.y)
        .sum();
    if area < 0.0 {
        polygon.reverse();
    }
    polygon
}

/// The SVG path data's numbers and commands.
struct Tokens<'a> {
    text: &'a [u8],
    at: usize,
}

impl Tokens<'_> {
    fn skip_separators(&mut self) {
        while self.at < self.text.len()
            && (self.text[self.at].is_ascii_whitespace() || self.text[self.at] == b',')
        {
            self.at += 1;
        }
    }

    fn command(&mut self) -> Option<u8> {
        self.skip_separators();
        let byte = *self.text.get(self.at)?;
        if byte.is_ascii_alphabetic() {
            self.at += 1;
            Some(byte)
        } else {
            None
        }
    }

    fn at_number(&mut self) -> bool {
        self.skip_separators();
        self.text
            .get(self.at)
            .is_some_and(|byte| byte.is_ascii_digit() || matches!(byte, b'-' | b'+' | b'.'))
    }

    fn number(&mut self) -> f32 {
        self.skip_separators();
        let start = self.at;
        if matches!(self.text.get(self.at), Some(b'-' | b'+')) {
            self.at += 1;
        }
        let mut dot = false;
        while let Some(&byte) = self.text.get(self.at) {
            if byte.is_ascii_digit() {
                self.at += 1;
            } else if byte == b'.' && !dot {
                dot = true;
                self.at += 1;
            } else if byte == b'e' || byte == b'E' {
                self.at += 1;
                if matches!(self.text.get(self.at), Some(b'-' | b'+')) {
                    self.at += 1;
                }
            } else {
                break;
            }
        }
        std::str::from_utf8(&self.text[start..self.at])
            .ok()
            .and_then(|text| text.parse().ok())
            .unwrap_or(0.0)
    }

    /// An arc's flag, which may be written without a separator ("011").
    fn flag(&mut self) -> bool {
        self.skip_separators();
        let byte = self.text.get(self.at).copied();
        self.at += 1;
        byte == Some(b'1')
    }
}

/// Flattened subpaths of SVG path data, each with whether it was closed.
fn parse(data: &str) -> Vec<(Vec<Pos2>, bool)> {
    let mut tokens = Tokens {
        text: data.as_bytes(),
        at: 0,
    };
    let mut paths: Vec<(Vec<Pos2>, bool)> = Vec::new();
    let mut current = Pos2::ZERO;
    let mut start = Pos2::ZERO;
    let mut last_control: Option<Pos2> = None;
    let mut command = b'M';
    let mut open = false;
    loop {
        if let Some(next) = tokens.command() {
            command = next;
        } else if !tokens.at_number() {
            break;
        }
        let relative = command.is_ascii_lowercase();
        let base = if relative { current } else { Pos2::ZERO };
        let point = |tokens: &mut Tokens, base: Pos2| {
            let x = tokens.number();
            let y = tokens.number();
            pos2(base.x + x, base.y + y)
        };
        let mut control = None;
        match command.to_ascii_uppercase() {
            b'M' => {
                current = point(&mut tokens, base);
                start = current;
                paths.push((vec![current], false));
                open = true;
                // Further pairs are lines.
                command = if relative { b'l' } else { b'L' };
            }
            b'Z' => {
                if let Some(path) = paths.last_mut() {
                    path.1 = true;
                }
                current = start;
                open = false;
            }
            upper => {
                if !open {
                    paths.push((vec![current], false));
                    start = current;
                    open = true;
                }
                let path = &mut paths.last_mut().expect("a subpath is open").0;
                match upper {
                    b'L' => current = point(&mut tokens, base),
                    b'H' => current.x = base.x + tokens.number(),
                    b'V' => current.y = base.y + tokens.number(),
                    b'C' | b'S' => {
                        let first = if upper == b'C' {
                            point(&mut tokens, base)
                        } else {
                            last_control.map_or(current, |previous| current + (current - previous))
                        };
                        let second = point(&mut tokens, base);
                        let end = point(&mut tokens, base);
                        cubic(path, current, first, second, end);
                        control = Some(second);
                        current = end;
                    }
                    b'Q' => {
                        let middle = point(&mut tokens, base);
                        let end = point(&mut tokens, base);
                        let first = current + (middle - current) * (2.0 / 3.0);
                        let second = end + (middle - end) * (2.0 / 3.0);
                        cubic(path, current, first, second, end);
                        current = end;
                    }
                    b'A' => {
                        let radii = egui::vec2(tokens.number(), tokens.number());
                        let rotation = tokens.number();
                        let large = tokens.flag();
                        let sweep = tokens.flag();
                        let end = point(&mut tokens, base);
                        arc(path, current, radii, rotation, large, sweep, end);
                        current = end;
                    }
                    _ => {
                        // An unknown command: stop rather than loop on it.
                        break;
                    }
                }
                if !matches!(upper, b'C' | b'S' | b'Q' | b'A') {
                    path.push(current);
                }
            }
        }
        last_control = control;
    }
    paths.retain(|(points, _)| points.len() > 1);
    paths
}

const CURVE_STEPS: usize = 12;

fn cubic(path: &mut Vec<Pos2>, from: Pos2, first: Pos2, second: Pos2, to: Pos2) {
    for step in 1..=CURVE_STEPS {
        let t = step as f32 / CURVE_STEPS as f32;
        let u = 1.0 - t;
        let point = from.to_vec2() * (u * u * u)
            + first.to_vec2() * (3.0 * u * u * t)
            + second.to_vec2() * (3.0 * u * t * t)
            + to.to_vec2() * (t * t * t);
        path.push(point.to_pos2());
    }
}

/// An SVG elliptical arc (endpoint form, SVG 1.1 F.6.5) as points.
fn arc(
    path: &mut Vec<Pos2>,
    from: Pos2,
    radii: egui::Vec2,
    rotation_degrees: f32,
    large: bool,
    sweep: bool,
    to: Pos2,
) {
    let (mut rx, mut ry) = (radii.x.abs(), radii.y.abs());
    if rx < 1e-6 || ry < 1e-6 || from == to {
        path.push(to);
        return;
    }
    let (sin, cos) = rotation_degrees.to_radians().sin_cos();
    let half = (from - to) / 2.0;
    let x1 = cos * half.x + sin * half.y;
    let y1 = -sin * half.x + cos * half.y;
    let lambda = x1 * x1 / (rx * rx) + y1 * y1 / (ry * ry);
    if lambda > 1.0 {
        rx *= lambda.sqrt();
        ry *= lambda.sqrt();
    }
    let numerator = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let denominator = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let sign = if large == sweep { -1.0 } else { 1.0 };
    let coefficient = sign * (numerator / denominator).max(0.0).sqrt();
    let cx1 = coefficient * rx * y1 / ry;
    let cy1 = -coefficient * ry * x1 / rx;
    let middle = (from.to_vec2() + to.to_vec2()) / 2.0;
    let center = egui::vec2(cos * cx1 - sin * cy1, sin * cx1 + cos * cy1) + middle;
    let angle = |ux: f32, uy: f32, vx: f32, vy: f32| (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
    let start = angle(1.0, 0.0, (x1 - cx1) / rx, (y1 - cy1) / ry);
    let mut delta = angle(
        (x1 - cx1) / rx,
        (y1 - cy1) / ry,
        (-x1 - cx1) / rx,
        (-y1 - cy1) / ry,
    );
    if !sweep && delta > 0.0 {
        delta -= std::f32::consts::TAU;
    } else if sweep && delta < 0.0 {
        delta += std::f32::consts::TAU;
    }
    let steps = ((delta.abs() / (std::f32::consts::PI / 16.0)).ceil() as usize).max(2);
    for step in 1..=steps {
        let theta = start + delta * step as f32 / steps as f32;
        let (s, c) = theta.sin_cos();
        path.push(pos2(
            center.x + rx * c * cos - ry * s * sin,
            center.y + rx * c * sin + ry * s * cos,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_parses_and_covers_some_texels() {
        for source in SOURCES {
            let image = rasterize(source);
            let covered = image.pixels.iter().filter(|pixel| pixel.a() > 128).count();
            assert!(covered > 20, "{} covers {covered} texels", source.name);
        }
    }

    #[test]
    fn even_odd_rings_leave_holes() {
        // The drum's bands cut through its body: the middle of a band is empty.
        let drum = SOURCES.iter().find(|source| source.name == "drum").unwrap();
        let image = rasterize(drum);
        let at = |x: f32, y: f32| {
            image.pixels[(y * ICON_SCALE) as usize * image.size[0] + (x * ICON_SCALE) as usize].a()
        };
        assert!(at(12.0, 5.5) > 200);
        assert!(at(12.0, 8.8) < 40);
    }
}
