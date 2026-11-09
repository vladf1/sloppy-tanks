//! Senda's houses drawn from models made in Blender instead of
//! [`super::house_model`]'s boxes, for the styles [`HOUSE_MODELS`] lists. Each model is a
//! binary glTF the page downloads as a hashed runtime file
//! (`public/models/houses/<name>.glb`, [`model_path`]) and hands over with [`install`];
//! until then, and natively without one, the coded house stands. A model drawn for one
//! lot fits every lot of its styles ([`ModelFit`]): mirrored for a garage on the other
//! side, stretched to the lot's box, wall top and ridge.
//!
//! Its meshes are named `<HousePart>__<Surface>` (`FrontWall__Siding`,
//! `LeftRoof__Roof`, `Ground__Stone`): each lies in the plane of one of the coded
//! house's [`HousePart`]s and is painted as that finish, so the importer hands them to
//! the structural house as its [`CottagePieces`], and the coded structure (its colliders,
//! supports and the cells each part is drawn from) cuts them when it breaks exactly as
//! it cuts the coded meshes. Layout, colliders and navigation stay the coded house's.

use std::collections::HashMap;
use std::f64::consts::FRAC_PI_2;
use std::sync::{Arc, Mutex};

use glam::DVec3;

use super::batching::batch;
use super::building_kit::{Surface, uv_per_metre};
use super::house_model::{
    CottagePiece, CottagePieces, cottage_pieces, grime, paint_cottage_whole, palette_for,
};
use super::tank_kit::KitMeshes;
use crate::geometry::Mesh;
use crate::geometry::{narrow, widen};
use crate::scene::Node;
use crate::sim::arena::cover_turn;
use crate::sim::house_layout::{
    HOUSE_MODELS, HouseDesign, HouseModel, HousePart, HousePlan, ModelFit, SENDA_STYLES,
    house_model,
};
use crate::sim::kinds::CoverKind;

/// The runtime file a model downloads as.
pub fn model_path(name: &str) -> String {
    format!("models/houses/{name}.glb")
}

/// Every model's runtime file, by the model's name.
pub fn model_paths() -> impl Iterator<Item = (&'static str, String)> {
    HOUSE_MODELS
        .iter()
        .map(|model| (model.name, model_path(model.name)))
}

/// An imported model: its meshes by part and finish, in the build frame of the lot it was
/// drawn for (ridge along z, front +z, ground at y = 0), or fitted to another lot.
pub struct ImportedHouse {
    meshes: Vec<(HousePart, Surface, Arc<Mesh>)>,
    /// The office and entry before the front (`Entry__<Surface>`), drawn as a cover of
    /// their own ([`standing_entry`]); a fitted house keeps none.
    entry: Vec<(Surface, Arc<Mesh>)>,
}

impl ImportedHouse {
    /// `part`'s meshes by finish, its stucco weathered up to `ridge` as the coded
    /// house's siding is (its material reads vertex colours).
    fn part_meshes(&self, part: HousePart, ridge: f64) -> KitMeshes<Surface> {
        let mut meshes: KitMeshes<Surface> = self
            .meshes
            .iter()
            .filter(|(p, ..)| *p == part)
            .map(|(_, surface, mesh)| (*surface, mesh.clone()))
            .collect();
        grime(&mut meshes, ridge);
        meshes
    }

    /// Each mesh's part (`Entry` for the entry's), finish and triangle count, for
    /// measuring.
    pub fn summary(&self) -> Vec<(String, String, usize)> {
        let triangles =
            |mesh: &Mesh| mesh.indices.as_ref().map_or(mesh.positions.len(), Vec::len) / 3;
        self.meshes
            .iter()
            .map(|(part, surface, mesh)| (format!("{part:?}"), surface, mesh))
            .chain(
                self.entry
                    .iter()
                    .map(|(surface, mesh)| (ENTRY_PART.to_string(), surface, mesh)),
            )
            .map(|(part, surface, mesh)| (part, format!("{surface:?}"), triangles(mesh)))
            .collect()
    }
}

/// The installed models by name, and their fits to other lots (by name and fit).
static MODELS: Mutex<Option<HashMap<&'static str, Arc<ImportedHouse>>>> = Mutex::new(None);
/// A fitted model's key: its name and its fit's measures (the mirror folded in).
type FitKey = (&'static str, [u64; 6]);
static FITTED: Mutex<Option<HashMap<FitKey, Arc<ImportedHouse>>>> = Mutex::new(None);
/// The entries' meshes fitted to their covers, by model and fit, in the cover's frame.
type EntryMeshes = Arc<Vec<(Surface, Arc<Mesh>)>>;
static ENTRIES: Mutex<Option<HashMap<FitKey, EntryMeshes>>> = Mutex::new(None);
/// The node prefix of a model's office and entry.
const ENTRY_PART: &str = "Entry";

/// Install the model `name` from its `.glb` bytes; an error leaves its coded houses.
pub fn install(name: &str, bytes: &[u8]) -> Result<(), String> {
    let model = HOUSE_MODELS
        .iter()
        .find(|model| model.name == name)
        .ok_or_else(|| format!("no house model {name}"))?;
    let house = parse_glb(bytes)?;
    MODELS
        .lock()
        .expect("house models")
        .get_or_insert_with(HashMap::new)
        .insert(model.name, Arc::new(house));
    FITTED.lock().expect("fitted models").take();
    ENTRIES.lock().expect("fitted entries").take();
    Ok(())
}

/// Go back to the coded houses (the render lab compares the two).
pub fn uninstall() {
    MODELS.lock().expect("house models").take();
    FITTED.lock().expect("fitted models").take();
    ENTRIES.lock().expect("fitted entries").take();
}

/// Whether the model `name` is installed.
pub fn installed(name: &str) -> bool {
    MODELS
        .lock()
        .expect("house models")
        .as_ref()
        .is_some_and(|models| models.contains_key(name))
}

/// The imported model for `design`, fitted to its lot, if its style has one installed.
fn model_for(design: &HouseDesign) -> Option<Arc<ImportedHouse>> {
    let (model, fit) = house_model(design)?;
    let source = MODELS
        .lock()
        .expect("house models")
        .as_ref()?
        .get(model.name)?
        .clone();
    if fit.identity() {
        return Some(source);
    }
    let key = fit_key(model.name, &fit);
    let mut fitted = FITTED.lock().expect("fitted models");
    Some(
        fitted
            .get_or_insert_with(HashMap::new)
            .entry(key)
            .or_insert_with(|| Arc::new(source.fitted(&fit)))
            .clone(),
    )
}

/// A fit's cache key: the model's name and the fit's measures (the mirror folded in).
fn fit_key(name: &'static str, fit: &ModelFit) -> FitKey {
    (
        name,
        [
            fit.scale_x,
            fit.scale_z,
            fit.from.0,
            fit.from.1,
            fit.to.0,
            fit.to.1,
        ]
        .map(|v| v.to_bits() ^ u64::from(fit.mirror)),
    )
}

/// `mesh` fitted by `fit`: each point placed, normals with it, a mirror's triangles
/// turned back to face out.
fn fitted_mesh(mesh: &Mesh, fit: &ModelFit) -> Mesh {
    let mut mesh = mesh.clone();
    for (p, n) in mesh.positions.iter_mut().zip(&mut mesh.normals) {
        let at = widen(*p);
        *n = narrow(fit.normal(widen(*n), at.y));
        *p = narrow(fit.point(at));
    }
    if fit.mirror
        && let Some(indices) = &mut mesh.indices
    {
        for triangle in indices.as_chunks_mut::<3>().0 {
            triangle.swap(1, 2);
        }
    }
    mesh
}

impl ImportedHouse {
    /// The model fitted to a lot: each point placed by `fit`, normals with it, a mirror's
    /// triangles turned back to face out and its flanks and slopes swapped. The entry is
    /// fitted for its cover alone ([`standing_entry`]).
    pub fn fitted(&self, fit: &ModelFit) -> ImportedHouse {
        let meshes = self
            .meshes
            .iter()
            .map(|(part, surface, mesh)| {
                let part = if fit.mirror {
                    mirrored(*part, fit.mirror_z)
                } else {
                    *part
                };
                (part, *surface, Arc::new(fitted_mesh(mesh, fit)))
            })
            .collect();
        ImportedHouse {
            meshes,
            entry: Vec::new(),
        }
    }

    /// The entry's meshes fitted by `fit`, then moved and turned by `place` into its
    /// cover's frame.
    fn fitted_entry(&self, fit: &ModelFit, place: impl Fn(DVec3) -> DVec3, turn: f64) -> Vec<(Surface, Arc<Mesh>)> {
        self.entry
            .iter()
            .map(|(surface, mesh)| {
                let mut mesh = fitted_mesh(mesh, fit);
                for (p, n) in mesh.positions.iter_mut().zip(&mut mesh.normals) {
                    *p = narrow(place(widen(*p)));
                    *n = narrow(turned(widen(*n), turn));
                }
                (*surface, Arc::new(mesh))
            })
            .collect()
    }
}

/// The part a mirror across x (or across z, `along_z`) draws `part` as.
fn mirrored(part: HousePart, along_z: bool) -> HousePart {
    use HousePart::{
        BackRoof, BackWall, FrontRoof, FrontWall, LeftRoof, LeftWall, RightRoof, RightWall,
    };
    match (part, along_z) {
        (RightWall, false) => LeftWall,
        (LeftWall, false) => RightWall,
        (RightRoof, false) => LeftRoof,
        (LeftRoof, false) => RightRoof,
        (FrontWall, true) => BackWall,
        (BackWall, true) => FrontWall,
        (FrontRoof, true) => BackRoof,
        (BackRoof, true) => FrontRoof,
        (other, _) => other,
    }
}

/// The structural house's pieces for `design`: its imported model's where it has one,
/// otherwise the coded ones. The wall openings (where torn boards keep clear) are the
/// coded house's, cut where the plan, and so the model, has them.
pub(super) fn house_pieces(design: &HouseDesign, x: f64, z: f64, ground: bool) -> CottagePieces {
    let coded = cottage_pieces(design, x, z, ground);
    let Some(model) = model_for(design) else {
        return coded;
    };
    let palette = palette_for(design, x, z);
    let mut parts = Vec::new();
    let mut yard = Node::group("");
    for part in HousePart::ALL {
        let meshes = model.part_meshes(part, coded.roof.ridge);
        if part == HousePart::Ground {
            continue;
        }
        parts.push(CottagePiece {
            part,
            meshes,
            shingles: None,
            nodes: Vec::new(),
        });
    }
    // The plinth, driveway, path and planting stay the coded house's: they are the lot's.
    if ground {
        yard = coded.ground;
    }
    CottagePieces {
        roof: coded.roof,
        palette,
        parts,
        openings: coded.openings,
        ground: yard,
    }
}

/// The standing house drawn from its imported model, every part whole: one draw per
/// finish, turned into the world as the coded house is; `None` for a house without one.
pub(super) fn standing_imported(design: &HouseDesign, x: f64, z: f64) -> Option<Node> {
    let model = model_for(design)?;
    let palette = palette_for(design, x, z);
    let ridge = design.roof().ridge;
    let mut group = Node::group("");
    for part in HousePart::ALL {
        paint_cottage_whole(&mut group, model.part_meshes(part, ridge), design, &palette);
    }
    // The lot's yard as the coded house draws it (its pieces' ground).
    group
        .children
        .push(cottage_pieces(design, x, z, true).ground);
    batch(&mut group);
    group.set_rotation_euler(0.0, design.yaw(), 0.0);
    Some(group)
}

/// How an office and entry is drawn for its cover: its house's model, the fit that
/// mirrors and stretches it as its house is, where that fit puts the entry's box centre
/// in the house's build frame and the turn (radians) from that frame to the cover's.
struct EntryFit {
    model: &'static HouseModel,
    fit: ModelFit,
    centre: DVec3,
    turn: f64,
}

/// [`EntryFit`] for a cover: `plan` is its house's (the cover carries it) and `(w, d, h)`
/// the cover's box, `w` across the house's front and `d` out from it. The entry stands
/// below the house's wall top, where its house's fit is a stretch along each axis, so the
/// box fixes that fit: the entry is mirrored and stretched exactly as its house is.
fn entry_fit(plan: HousePlan, (w, d, h): (f64, f64, f64)) -> Option<EntryFit> {
    let style = SENDA_STYLES[usize::from(plan.style) % SENDA_STYLES.len()];
    let model = HOUSE_MODELS.iter().find(|model| {
        model
            .styles
            .iter()
            .any(|&other| SENDA_STYLES[usize::from(other)] == style)
    })?;
    let authored = model.authored();
    let entry = authored.entry?;
    // A house whose door is in its eave wall is built a quarter turn round: its street
    // front is the build frame's +x wall, and its street frame turns back from it.
    let along = authored.door_wall == 2;
    let (build_w, build_d) = if along { (d, w) } else { (w, d) };
    let lift = h / entry.h;
    let fit = ModelFit {
        mirror: plan.garage != authored.garage,
        mirror_z: along,
        scale_x: build_w / entry.w,
        scale_z: build_d / entry.d,
        from: (authored.wall, authored.ridge),
        to: (
            authored.wall * lift,
            authored.ridge - authored.wall + authored.wall * lift,
        ),
    };
    Some(EntryFit {
        model,
        fit,
        centre: fit.point(DVec3::new(entry.x, 0.0, entry.z)),
        turn: if along { -FRAC_PI_2 } else { 0.0 },
    })
}

/// `p` turned `angle` (radians) about +y, as a house's build frame turns into the world.
fn turned(p: DVec3, angle: f64) -> DVec3 {
    let (sin, cos) = angle.sin_cos();
    DVec3::new(p.x * cos + p.z * sin, p.y, -p.x * sin + p.z * cos)
}

/// The office and entry before a planned house, standing, for its cover at `(x, z)`
/// ([`entry_fit`]): drawn in the cover's frame, the house's street frame about the box's
/// middle, which the cover turns into the world. `None` while the house's model is not
/// installed.
pub(super) fn standing_entry(plan: HousePlan, x: f64, z: f64, size: (f64, f64, f64)) -> Option<Node> {
    let EntryFit {
        model,
        fit,
        centre,
        turn,
    } = entry_fit(plan, size)?;
    let source = MODELS
        .lock()
        .expect("house models")
        .as_ref()?
        .get(model.name)?
        .clone();
    let meshes = ENTRIES
        .lock()
        .expect("fitted entries")
        .get_or_insert_with(HashMap::new)
        .entry(fit_key(model.name, &fit))
        .or_insert_with(|| {
            let mut meshes = source.fitted_entry(&fit, |p| turned(p - centre, turn), turn);
            grime(&mut meshes, fit.to.1);
            Arc::new(meshes)
        })
        .clone();
    // The house's centre, for its paint (its roof's shade varies with where it stands).
    let offset = turned(centre, cover_turn(CoverKind::Concrete, Some(plan)) + turn);
    let (house_x, house_z) = (x - offset.x, z - offset.z);
    let authored = model.authored();
    let design = HouseDesign::planned(house_x, house_z, authored.w, authored.d, authored.ridge, plan);
    let palette = palette_for(&design, house_x, house_z);
    let mut group = Node::group("");
    paint_cottage_whole(&mut group, (*meshes).clone(), &design, &palette);
    batch(&mut group);
    Some(group)
}

fn part_named(name: &str) -> Option<HousePart> {
    HousePart::ALL
        .into_iter()
        .find(|part| format!("{part:?}") == name)
}

fn surface_named(name: &str) -> Option<Surface> {
    use super::tank_kit::Finishes;
    Surface::ALL
        .iter()
        .copied()
        .find(|surface| format!("{surface:?}") == name)
}

/// The meshes of a binary glTF whose nodes are named `<HousePart>__<Surface>` (or
/// `Entry__<Surface>`, the office and entry before the front): each
/// mesh's first primitive (positions, normals, metre UVs scaled to the finish's texture,
/// indices), placed by its node's translation. Blender's +Y up and -Z forward export
/// lands in the build frame as is.
pub fn parse_glb(bytes: &[u8]) -> Result<ImportedHouse, String> {
    let u32_at = |at: usize| -> Result<u32, String> {
        bytes
            .get(at..at + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .ok_or_else(|| "truncated".to_string())
    };
    if bytes.get(0..4) != Some(b"glTF") {
        return Err("not a binary glTF".into());
    }
    let json_length = u32_at(12)? as usize;
    let json_chunk = bytes.get(20..20 + json_length).ok_or("truncated")?;
    let json: serde_json::Value =
        serde_json::from_slice(json_chunk).map_err(|error| error.to_string())?;
    let bin_start = 20 + json_length + 8;
    let bin = bytes.get(bin_start..).ok_or("no binary chunk")?;
    let accessor = |index: &serde_json::Value| -> Result<(&[u8], usize, usize), String> {
        let accessor = &json["accessors"][index.as_u64().ok_or("accessor")? as usize];
        let view = &json["bufferViews"][accessor["bufferView"].as_u64().ok_or("view")? as usize];
        let offset = view["byteOffset"].as_u64().unwrap_or(0) as usize
            + accessor["byteOffset"].as_u64().unwrap_or(0) as usize;
        let count = accessor["count"].as_u64().ok_or("count")? as usize;
        let component = accessor["componentType"].as_u64().ok_or("type")? as usize;
        let length = view["byteLength"].as_u64().ok_or("length")? as usize;
        let data = bin
            .get(offset..offset + length)
            .ok_or("a view past the binary chunk")?;
        Ok((data, count, component))
    };
    let floats = |index: &serde_json::Value, n: usize| -> Result<Vec<f32>, String> {
        let (data, count, _) = accessor(index)?;
        let bytes = data
            .get(..count * n * 4)
            .ok_or("an accessor past its view")?;
        Ok(bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect())
    };
    let mut meshes = Vec::new();
    let mut entry = Vec::new();
    for node in json["nodes"].as_array().ok_or("no nodes")? {
        let Some(mesh_index) = node["mesh"].as_u64() else {
            continue;
        };
        let name = node["name"].as_str().unwrap_or_default();
        let base = name.split('.').next().unwrap_or(name);
        let Some((part, surface)) = base.split_once("__") else {
            return Err(format!("node {name} is not <HousePart>__<Surface>"));
        };
        let part = if part == ENTRY_PART {
            None
        } else {
            Some(part_named(part).ok_or_else(|| format!("no part {part}"))?)
        };
        let surface = surface_named(surface).ok_or_else(|| format!("no surface {surface}"))?;
        let at = node["translation"]
            .as_array()
            .map(|t| [0, 1, 2].map(|k| t[k].as_f64().unwrap_or(0.0) as f32))
            .unwrap_or([0.0; 3]);
        let uv_scale = uv_per_metre(surface) as f32;
        for primitive in json["meshes"][mesh_index as usize]["primitives"]
            .as_array()
            .ok_or("no primitives")?
        {
            let attributes = &primitive["attributes"];
            let positions: Vec<[f32; 3]> = floats(&attributes["POSITION"], 3)?
                .as_chunks::<3>()
                .0
                .iter()
                .map(|p| [p[0] + at[0], p[1] + at[1], p[2] + at[2]])
                .collect();
            let normals: Vec<[f32; 3]> = floats(&attributes["NORMAL"], 3)?
                .as_chunks::<3>()
                .0
                .to_vec();
            let uvs: Vec<[f32; 2]> = if attributes["TEXCOORD_0"].is_null() {
                vec![[0.0, 0.0]; positions.len()]
            } else {
                floats(&attributes["TEXCOORD_0"], 2)?
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|uv| [uv[0] * uv_scale, uv[1] * uv_scale])
                    .collect()
            };
            let (data, count, component) = accessor(&primitive["indices"])?;
            let indices: Vec<u32> = match component {
                5123 => data
                    .get(..count * 2)
                    .ok_or("indices past their view")?
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|b| u32::from(u16::from_le_bytes(*b)))
                    .collect(),
                5125 => data
                    .get(..count * 4)
                    .ok_or("indices past their view")?
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|b| u32::from_le_bytes(*b))
                    .collect(),
                other => return Err(format!("index type {other}")),
            };
            let mesh = Mesh {
                positions,
                normals,
                uvs,
                indices: Some(indices),
                ..Mesh::default()
            };
            match part {
                Some(part) => meshes.push((part, surface, Arc::new(mesh))),
                None => entry.push((surface, Arc::new(mesh))),
            }
        }
    }
    Ok(ImportedHouse { meshes, entry })
}

/// Development dump: the coded home's pieces as an OBJ (one group per
/// `<HousePart>__<Surface>`, in the build frame), for an underlay in Blender, with its
/// triangle count per part.
pub fn coded_pieces_obj(design: &HouseDesign, x: f64, z: f64) -> (String, Vec<(String, usize)>) {
    use std::fmt::Write;
    let pieces = cottage_pieces(design, x, z, true);
    let mut obj = String::new();
    let mut counts = Vec::new();
    let mut base = 1;
    let mut emit = |obj: &mut String, name: String, mesh: &Mesh| {
        let _ = writeln!(obj, "o {name}");
        for p in &mesh.positions {
            let _ = writeln!(obj, "v {} {} {}", p[0], p[1], p[2]);
        }
        let triangles: Vec<u32> = match &mesh.indices {
            Some(indices) => indices.clone(),
            None => (0..mesh.positions.len() as u32).collect(),
        };
        for t in triangles.as_chunks::<3>().0 {
            let _ = writeln!(obj, "f {} {} {}", t[0] + base, t[1] + base, t[2] + base);
        }
        base += mesh.positions.len() as u32;
        triangles.len() / 3
    };
    for piece in &pieces.parts {
        let mut count = 0;
        for (surface, mesh) in &piece.meshes {
            count += emit(&mut obj, format!("{:?}__{surface:?}", piece.part), mesh);
        }
        if let Some(shingles) = &piece.shingles {
            count += emit(&mut obj, format!("{:?}__Shingles", piece.part), shingles);
        }
        counts.push((format!("{:?}", piece.part), count));
    }
    (obj, counts)
}

#[cfg(test)]
mod tests {
    use glam::{DVec2, DVec3};

    use super::*;
    use crate::sim::house_layout::{
        GARAGE_HOUSE_PITCH, GarageSide, HouseModel, HousePlan, OpeningKind, REFERENCE_HOUSE,
        SENDA_STYLES, wall_frames,
    };
    use crate::sim::house_structure::{PartKind, house_structure};
    use crate::sim::senda_layout::senda_houses;

    /// The shipped models, by name.
    fn shipped(name: &str) -> ImportedHouse {
        let path = format!(
            "{}/../../public/{}",
            env!("CARGO_MANIFEST_DIR"),
            model_path(name)
        );
        parse_glb(&std::fs::read(&path).expect("the model is shipped")).expect("the model parses")
    }
    /// What a house may cost phones: model triangles and finishes (each a draw).
    const TRIANGLE_BUDGET: usize = 6000;
    const FINISH_BUDGET: usize = 16;
    /// How far (m) past its walls a model may reach: its flanks (eaves, a porch beside the
    /// house), its back (eaves), its front (a wing or porch before the house, as the coded
    /// houses' reach) and over its ridge.
    const REACH: (f64, f64, f64, f64) = (1.1, 0.6, 2.1, 0.45);
    /// The model's wall faces lie this close to the wall's plane.
    const ON_PLANE: f64 = 0.01;

    /// Every lot a model draws, and the same lots mirrored and narrowed, as the street's
    /// other lots would ask of it.
    fn lots(model: &HouseModel) -> Vec<HouseDesign> {
        let mut designs: Vec<HouseDesign> = senda_houses()
            .into_iter()
            .map(|house| house.design())
            .filter(|design| house_model(design).is_some_and(|(found, _)| found.name == model.name))
            .collect();
        for &style in model.styles {
            for garage in [GarageSide::Left, GarageSide::Right] {
                for (w, d, plate) in [(5.8, 7.4, 3.15), (6.2, 7.7, 3.25), (6.2, 8.0, 3.2)] {
                    let plan = HousePlan {
                        style,
                        street: 0,
                        angle: 0,
                        garage,
                    };
                    let h = plate + GARAGE_HOUSE_PITCH * w / 2.0;
                    designs.push(HouseDesign::planned(0.0, 0.0, w, d, h, plan));
                }
            }
        }
        designs
    }

    #[test]
    fn the_shipped_models_parse_within_their_budgets_and_lots() {
        for model in HOUSE_MODELS {
            let house = shipped(model.name);
            let triangles: usize = house.summary().iter().map(|(.., n)| n).sum();
            assert!(
                triangles <= TRIANGLE_BUDGET,
                "{}: {triangles} triangles",
                model.name
            );
            let finishes: std::collections::HashSet<_> =
                house.meshes.iter().map(|(_, s, _)| *s).collect();
            assert!(
                finishes.len() <= FINISH_BUDGET,
                "{}: {finishes:?}",
                model.name
            );
            let authored = model.authored();
            let (hx, hz) = (authored.w / 2.0, authored.d / 2.0);
            for (part, surface, mesh) in &house.meshes {
                assert_eq!(
                    mesh.positions.len(),
                    mesh.normals.len(),
                    "{part:?} {surface:?}"
                );
                for p in mesh.positions.iter().map(|p| widen(*p)) {
                    // Along the front's axis out from it, back from the back, and across.
                    let (out, half_out, across, half_across) = if authored.door_wall == 2 {
                        (p.x, hx, p.z, hz)
                    } else {
                        (p.z, hz, p.x, hx)
                    };
                    assert!(
                        across.abs() <= half_across + REACH.0
                            && out >= -half_out - REACH.1
                            && out <= half_out + REACH.2
                            && (-0.01..=authored.ridge + REACH.3).contains(&p.y),
                        "{}: {part:?} {surface:?} reaches {p}",
                        model.name
                    );
                }
            }
        }
    }

    #[test]
    fn the_home_keeps_its_models_plan() {
        let home = senda_houses()
            .into_iter()
            .find(|house| {
                SENDA_STYLES[usize::from(house.plan.style)]
                    == SENDA_STYLES[usize::from(REFERENCE_HOUSE)]
            })
            .expect("the reference house")
            .design();
        // Drawn as modelled but for its small plan's depth: unmirrored, its front as drawn
        // and its heights within a lot's plate of it (a layout change to the reference
        // house's lot fails here rather than stretching the reference house across its front).
        let (model, fit) = house_model(&home).expect("the reference house has a model");
        let across = if home.door_wall == 0 {
            fit.scale_x
        } else {
            fit.scale_z
        };
        assert!(
            !fit.mirror
                && (across - 1.0).abs() < 1e-9
                && (fit.from.0 - fit.to.0).abs() < 0.1
                && (fit.from.1 - fit.to.1).abs() < 0.1,
            "{fit:?}"
        );
        // Its front as drawn, the openings of its other walls where the fit puts them.
        let near = |a: f64, b: f64| (a - b).abs() < 1e-6;
        for index in 0..4 {
            let plan = home.wall_plan(index);
            let authored = model.authored().walls_for_test(fit.source_wall(index));
            let scale = if index == home.door_wall {
                1.0
            } else if index < 2 {
                fit.scale_x
            } else {
                fit.scale_z
            };
            assert!(
                plan.ground.len() == authored.ground.len()
                    && plan
                        .ground
                        .iter()
                        .zip(&authored.ground)
                        .all(|(a, b)| near(a.0, b.0 * scale) && a.1 == b.1)
                    && plan.upper.len() == authored.upper.len()
                    && plan
                        .upper
                        .iter()
                        .zip(&authored.upper)
                        .all(|(a, b)| near(*a, *b * scale)),
                "wall {index}: {plan:?} against the model's {authored:?}"
            );
        }
    }

    #[test]
    fn every_lot_frames_its_models_windows() {
        for model in HOUSE_MODELS {
            for design in lots(model) {
                let structure = house_structure(&design);
                for index in 0..4 {
                    let plan = design.wall_plan(index);
                    let (center, _, along, half) = wall_frames(design.w, design.d)[index];
                    let windows = |upper: bool| {
                        structure
                            .parts
                            .iter()
                            .filter(|part| {
                                let role = part.house();
                                role.kind == PartKind::Window
                                    && role.wall == Some(index)
                                    && role.upper_storey == upper
                            })
                            .map(|part| ((part.min + part.max) / 2.0 - center).dot(along))
                            .collect::<Vec<_>>()
                    };
                    let (lower, upper) = (windows(false), windows(true));
                    for &(u, kind) in &plan.ground {
                        assert!(
                            u.abs() < half,
                            "{} wall {index}: opening at {u} past the wall",
                            model.name
                        );
                        if kind == OpeningKind::Window {
                            assert!(
                                lower.iter().any(|w| (w - u).abs() < 0.3),
                                "{} {design:?} wall {index}: no window at {u} in {lower:?}",
                                model.name
                            );
                        }
                    }
                    for &u in &plan.upper {
                        assert!(
                            upper.iter().any(|w| (w - u).abs() < 0.6),
                            "{} wall {index}: no upper window at {u} in {upper:?}",
                            model.name
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn every_planned_opening_is_open_in_the_fitted_model() {
        for model in HOUSE_MODELS {
            let source = shipped(model.name);
            for design in lots(model) {
                let (_, fit) = house_model(&design).expect("a model");
                let house = source.fitted(&fit);
                let roof = design.roof();
                let (sill, height) = roof.ground_window();
                let (upper_sill, upper_height) = roof.upper_window();
                for index in 0..4 {
                    let plan = design.wall_plan(index);
                    let (center, out, along, _) = wall_frames(design.w, design.d)[index];
                    let part = HousePart::wall(index);
                    let mut faces: Vec<[DVec2; 3]> = Vec::new();
                    for (_, _, mesh) in house
                        .meshes
                        .iter()
                        .filter(|(p, s, _)| *p == part && *s == Surface::Siding)
                    {
                        for t in mesh.indices.as_ref().expect("indexed").as_chunks::<3>().0 {
                            let points = t.map(|i| widen(mesh.positions[i as usize]));
                            if points
                                .iter()
                                .all(|p| (*p - center).dot(out).abs() < ON_PLANE)
                            {
                                faces.push(
                                    points.map(|p: DVec3| DVec2::new((p - center).dot(along), p.y)),
                                );
                            }
                        }
                    }
                    assert!(
                        !faces.is_empty(),
                        "{} wall {index} has no stucco face",
                        model.name
                    );
                    let covered = |q: DVec2| {
                        faces.iter().any(|[a, b, c]| {
                            let side = |p: DVec2, r: DVec2, s: DVec2| (r - p).perp_dot(s - p);
                            let (d0, d1, d2) = (side(*a, *b, q), side(*b, *c, q), side(*c, *a, q));
                            (d0 >= 0.0 && d1 >= 0.0 && d2 >= 0.0)
                                || (d0 <= 0.0 && d1 <= 0.0 && d2 <= 0.0)
                        })
                    };
                    for &(u, kind) in &plan.ground {
                        let y = match kind {
                            OpeningKind::Window => sill + height / 2.0,
                            OpeningKind::Door | OpeningKind::Garage => roof.plinth + 0.8,
                        };
                        assert!(
                            !covered(DVec2::new(u, y)),
                            "{} {:?} {} wall {index}: stucco over the {kind:?} at {u}",
                            model.name,
                            design.garage,
                            design.w
                        );
                    }
                    for &u in &plan.upper {
                        let y = upper_sill + upper_height / 2.0;
                        assert!(
                            !covered(DVec2::new(u, y)),
                            "{} {:?} {} wall {index}: stucco over the upper window at {u}",
                            model.name,
                            design.garage,
                            design.w
                        );
                    }
                }
            }
        }
    }

    /// How far (m) past its box an entry's model may reach: its steps, piers and planter
    /// before it, its eaves and the reference house's passage behind the house's front.
    const ENTRY_REACH: f64 = 1.0;

    #[test]
    fn every_model_draws_its_entry_apart_inside_its_box() {
        for model in HOUSE_MODELS {
            let house = shipped(model.name);
            let entry = model.authored().entry.expect("an entry box in the plan");
            assert!(!house.entry.is_empty(), "{}: no Entry__ nodes", model.name);
            let inside = |p: DVec3, grow: f64, top: f64| {
                (p.x - entry.x).abs() < entry.w / 2.0 + grow
                    && (p.z - entry.z).abs() < entry.d / 2.0 + grow
                    && p.y < top
            };
            for (surface, mesh) in &house.entry {
                for p in mesh.positions.iter().map(|p| widen(*p)) {
                    assert!(
                        p.y >= -0.01 && inside(p, ENTRY_REACH, entry.h + 0.01),
                        "{}: Entry {surface:?} reaches {p}",
                        model.name
                    );
                }
            }
            // The house keeps nothing in the entry's box: it stands whole once it is gone.
            for (part, surface, mesh) in &house.meshes {
                for p in mesh.positions.iter().map(|p| widen(*p)) {
                    assert!(
                        !inside(p, -0.05, entry.h - 0.05),
                        "{}: {part:?} {surface:?} at {p} inside the entry's box",
                        model.name
                    );
                }
            }
        }
    }

    #[test]
    fn an_entry_cover_fits_its_entry_as_its_house_fits_the_house() {
        use crate::sim::senda_entries::design_entry;
        let mut plans: Vec<(HousePlan, f64, f64, f64)> = senda_houses()
            .into_iter()
            .map(|house| (house.plan, house.w, house.d, house.h))
            .collect();
        for model in HOUSE_MODELS {
            for garage in [GarageSide::Left, GarageSide::Right] {
                for (w, d, plate) in [(5.8, 7.4, 3.15), (6.2, 7.7, 3.25), (6.2, 8.0, 3.2)] {
                    let plan = HousePlan {
                        style: model.styles[0],
                        street: 1,
                        angle: 0,
                        garage,
                    };
                    plans.push((plan, w, d, plate + GARAGE_HOUSE_PITCH * w / 2.0));
                }
            }
        }
        for (plan, w, d, h) in plans {
            let design = HouseDesign::planned(0.0, 0.0, w, d, h, plan);
            let (model, house) = house_model(&design).expect("a model");
            let place = design_entry(&design).expect("an entry");
            let entry = entry_fit(plan, (place.w, place.d, place.h)).expect("an entry fit");
            let near = |a: f64, b: f64| (a - b).abs() < 2e-3 * b.abs().max(1.0);
            assert!(
                entry.model.name == model.name
                    && entry.fit.mirror == house.mirror
                    && entry.fit.mirror_z == house.mirror_z
                    && near(entry.fit.scale_x, house.scale_x)
                    && near(entry.fit.scale_z, house.scale_z)
                    && near(entry.fit.to.0, house.to.0),
                "{}: {:?} against the house's {house:?}",
                model.name,
                entry.fit
            );
            // The cover's middle is where the house's fit puts the entry's, and its frame
            // turns into the world as the house's build frame does.
            let at = design.to_world(house.point(DVec3::new(
                model.authored().entry.expect("a box").x,
                0.0,
                model.authored().entry.expect("a box").z,
            )));
            let cover = turned(
                turned(entry.centre, entry.turn),
                crate::sim::arena::cover_turn(CoverKind::Concrete, Some(plan)),
            );
            assert!(
                at.distance(DVec3::new(place.at.x, 0.0, place.at.y)) < 1e-3
                    && at.distance(cover) < 2e-3,
                "{}: entry at {at}, cover at {}, turned {cover}",
                model.name,
                place.at
            );
        }
    }
}
