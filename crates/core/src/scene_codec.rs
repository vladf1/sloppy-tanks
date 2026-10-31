//! A binary form of model trees ([`Node`]) for the page's cache of generated geometry
//! (`crates/render/src/presentation/geometry_cache.rs`): a returning visitor reads
//! the vehicles and a theme's scenery back instead of building them again.
//!
//! Reading gives back the tree exactly: every value bit for bit (`model_pins` hashes
//! agree), and every `Arc<Mesh>` and `Arc<Material>` the tree shares written once and
//! shared again, so batching by `Arc` identity comes out the same. Values the tree
//! shares with the builders' stores are recorded by store and key, and reading hands
//! them to the stores (`models::shared_values`), so models built afterwards share
//! them too. Static strings (texture paths, effect and attribute names) come back
//! interned, one leaked copy per distinct text for the process.
//!
//! A file is a small structure (header, the key it was written under, strings, mesh
//! shapes, materials, shared-value records, the node tree) followed by the mesh
//! arrays, little-endian. Reading copies each array straight into its mesh through a
//! `fill` callback, so a page copies from the browser's buffer without first holding
//! the whole file in Wasm memory.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use glam::{DMat4, DQuat, DVec3};

use crate::geometry::{Attribute, Mesh};
use crate::models::shared_values::adopt;
use crate::scene::{
    Blending, Color, Drawable, Effect, Instance, Material, Node, Shading, Side, TextureRef,
    TextureSource, Wrap,
};

/// Bumped whenever the layout below changes.
pub const FORMAT_VERSION: u32 = 1;
const MAGIC: u32 = u32::from_le_bytes(*b"STGC");
/// Magic, format, structure length, data length.
const HEADER_BYTES: usize = 16;

/// A tree written for the cache: `structure` then the `data` slices in order make
/// the file; the slices borrow the tree's meshes.
pub struct Encoded<'a> {
    pub structure: Vec<u8>,
    pub data: Vec<&'a [u8]>,
}

impl Encoded<'_> {
    pub fn len(&self) -> usize {
        self.structure.len() + self.data_len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn data_len(&self) -> usize {
        self.data.iter().map(|slice| slice.len()).sum()
    }

    /// The whole file in one buffer (natively and in tests).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.len());
        bytes.extend_from_slice(&self.structure);
        for slice in &self.data {
            bytes.extend_from_slice(slice);
        }
        bytes
    }
}

/// Why a cache file was not read.
#[derive(Debug, PartialEq, Eq)]
pub struct DecodeError(pub &'static str);

/// Write `root` for the cache under `key` (what built it: a content version and
/// theme), which [`decode`] checks.
pub fn encode<'a>(root: &'a Node, key: &str) -> Encoded<'a> {
    // The tree first: it registers every string, mesh and material, then the stores
    // that share them, so the tables are complete before they are written ahead of it.
    let mut encoder = Encoder::default();
    let mut tree = Writer::default();
    encoder.node(root, &mut tree);
    let mut shared = Writer::default();
    let mut shared_count = 0u32;
    let places = crate::models::shared_values::places();
    let meshes: Vec<usize> = encoder.meshes.iter().map(|&mesh| address(mesh)).collect();
    let materials: Vec<usize> = encoder.materials.iter().map(|&m| address(m)).collect();
    for (kind, addresses) in [(SHARED_MESH, meshes), (SHARED_MATERIAL, materials)] {
        for (index, address) in addresses.into_iter().enumerate() {
            if let Some(&(store, key)) = places.get(&address) {
                shared.u8(kind);
                shared.u32(index as u32);
                shared.u32(encoder.string(store));
                shared.u64(key);
                shared_count += 1;
            }
        }
    }
    let string_count = encoder.strings.len();
    let mut body = Writer::default();
    body.str(key);
    body.u32(string_count as u32);
    for text in &encoder.strings {
        body.str(text);
    }
    body.u32(encoder.meshes.len() as u32);
    for mesh in &encoder.meshes {
        mesh_shape(mesh, &mut body);
    }
    body.u32(encoder.materials.len() as u32);
    for material in encoder.materials.clone() {
        encoder.material_body(material, &mut body);
    }
    debug_assert_eq!(encoder.strings.len(), string_count);
    body.u32(shared_count);
    body.bytes(&shared.0);
    body.bytes(&tree.0);
    while body.0.len() % 4 != 0 {
        body.0.push(0);
    }
    let mut data: Vec<&'a [u8]> = Vec::new();
    for mesh in &encoder.meshes {
        mesh_data(mesh, &mut data);
    }
    let data_len: usize = data.iter().map(|slice| slice.len()).sum();
    let mut structure = Vec::with_capacity(HEADER_BYTES + body.0.len());
    structure.extend_from_slice(&MAGIC.to_le_bytes());
    structure.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    structure.extend_from_slice(&(body.0.len() as u32).to_le_bytes());
    structure.extend_from_slice(&(data_len as u32).to_le_bytes());
    structure.extend_from_slice(&body.0);
    Encoded { structure, data }
}

/// The lengths of a file's structure (header included) and its data, read from its
/// first [`header_len`] bytes.
pub fn sections(header: &[u8]) -> Result<(usize, usize), DecodeError> {
    let mut reader = Reader::new(header);
    if reader.u32()? != MAGIC {
        return Err(DecodeError("not a geometry cache file"));
    }
    if reader.u32()? != FORMAT_VERSION {
        return Err(DecodeError("another format version"));
    }
    let body = reader.u32()? as usize;
    let data = reader.u32()? as usize;
    Ok((HEADER_BYTES + body, data))
}

pub const fn header_len() -> usize {
    HEADER_BYTES
}

/// Read a whole file from one buffer.
pub fn decode_bytes(bytes: &[u8], key: &str) -> Result<Node, DecodeError> {
    let (structure, data) = sections(bytes.get(..HEADER_BYTES).ok_or(DecodeError("short"))?)?;
    if bytes.len() != structure + data {
        return Err(DecodeError("truncated"));
    }
    let (head, rest) = bytes.split_at(structure);
    decode(head, key, &mut |offset, out| {
        let Some(source) = rest.get(offset..offset + out.len()) else {
            return false;
        };
        out.copy_from_slice(source);
        true
    })
}

/// Read a file whose `structure` (header included, see [`sections`]) is in memory;
/// `fill(offset, out)` copies `out.len()` bytes of its data section from `offset`.
pub fn decode(
    structure: &[u8],
    key: &str,
    fill: &mut dyn FnMut(usize, &mut [u8]) -> bool,
) -> Result<Node, DecodeError> {
    let (structure_len, data_len) =
        sections(structure.get(..HEADER_BYTES).ok_or(DecodeError("short"))?)?;
    if structure.len() != structure_len {
        return Err(DecodeError("structure length"));
    }
    let mut reader = Reader::new(&structure[HEADER_BYTES..]);
    if reader.str()? != key {
        return Err(DecodeError("built by another version"));
    }
    let string_count = reader.u32()? as usize;
    let mut strings = Vec::with_capacity(string_count);
    for _ in 0..string_count {
        strings.push(reader.str()?);
    }
    let mut decoder = Decoder {
        strings,
        statics: vec![None; string_count],
        meshes: Vec::new(),
        materials: Vec::new(),
    };
    let mesh_count = reader.u32()? as usize;
    let mut offset = 0;
    for _ in 0..mesh_count {
        let mesh = decoder.mesh(&mut reader, &mut offset, fill)?;
        decoder.meshes.push(Arc::new(mesh));
    }
    if offset != data_len {
        return Err(DecodeError("data length"));
    }
    let material_count = reader.u32()? as usize;
    for _ in 0..material_count {
        let material = decoder.material(&mut reader)?;
        decoder.materials.push(Arc::new(material));
    }
    // Take the process's shared values where it has them, and offer it the rest.
    let shared_count = reader.u32()? as usize;
    for _ in 0..shared_count {
        let kind = reader.u8()?;
        let index = reader.u32()? as usize;
        let store = decoder.text(&mut reader)?;
        let key = u64::from_le_bytes(reader.array()?);
        match kind {
            SHARED_MESH => {
                let mesh = decoder.meshes.get(index).ok_or(DecodeError("mesh index"))?;
                let adopted = adopt(store, key, mesh.clone());
                decoder.meshes[index] = adopted;
            }
            SHARED_MATERIAL => {
                let material = decoder
                    .materials
                    .get(index)
                    .ok_or(DecodeError("material index"))?;
                let adopted = adopt(store, key, material.clone());
                decoder.materials[index] = adopted;
            }
            _ => return Err(DecodeError("shared kind")),
        }
    }
    decoder.node(&mut reader)
}

/// What a shared-value record names: a mesh or a material of the file.
const SHARED_MESH: u8 = 0;
const SHARED_MATERIAL: u8 = 1;

fn address<T>(value: &T) -> usize {
    std::ptr::from_ref(value) as usize
}

// ---------------------------------------------------------------------------
// Writing.

#[derive(Default)]
struct Writer(Vec<u8>);

impl Writer {
    fn u8(&mut self, value: u8) {
        self.0.push(value);
    }
    fn bool(&mut self, value: bool) {
        self.0.push(u8::from(value));
    }
    fn u32(&mut self, value: u32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    fn i32(&mut self, value: i32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    fn f32(&mut self, value: f32) {
        self.0.extend_from_slice(&value.to_bits().to_le_bytes());
    }
    fn f64(&mut self, value: f64) {
        self.0.extend_from_slice(&value.to_bits().to_le_bytes());
    }
    fn u64(&mut self, value: u64) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }
    fn str(&mut self, text: &str) {
        self.u32(text.len() as u32);
        self.0.extend_from_slice(text.as_bytes());
    }
    fn bytes(&mut self, bytes: &[u8]) {
        self.0.extend_from_slice(bytes);
    }
}

#[derive(Default)]
struct Encoder<'a> {
    strings: Vec<&'a str>,
    string_index: HashMap<&'a str, u32>,
    meshes: Vec<&'a Mesh>,
    mesh_index: HashMap<*const Mesh, u32>,
    materials: Vec<&'a Material>,
    material_index: HashMap<*const Material, u32>,
}

impl<'a> Encoder<'a> {
    fn string(&mut self, text: &'a str) -> u32 {
        if let Some(&index) = self.string_index.get(text) {
            return index;
        }
        let index = self.strings.len() as u32;
        self.strings.push(text);
        self.string_index.insert(text, index);
        index
    }

    fn mesh(&mut self, mesh: &'a Arc<Mesh>) -> u32 {
        let key = Arc::as_ptr(mesh);
        if let Some(&index) = self.mesh_index.get(&key) {
            return index;
        }
        let index = self.meshes.len() as u32;
        for attribute in &mesh.attributes {
            self.string(attribute.name);
        }
        self.meshes.push(mesh);
        self.mesh_index.insert(key, index);
        index
    }

    fn material(&mut self, material: &'a Arc<Material>) -> u32 {
        let key = Arc::as_ptr(material);
        if let Some(&index) = self.material_index.get(&key) {
            return index;
        }
        let index = self.materials.len() as u32;
        // Register its strings now, so the table is complete before it is written.
        let mut scratch = Writer::default();
        self.material_body(material, &mut scratch);
        self.materials.push(material);
        self.material_index.insert(key, index);
        index
    }

    fn texture(&mut self, texture: &'a TextureRef, out: &mut Writer) {
        let TextureRef {
            source,
            wrap,
            repeat,
            offset,
            srgb,
            anisotropy,
            mipmaps,
            flip_y,
        } = texture;
        match source {
            TextureSource::File(path) => {
                out.u8(0);
                out.u32(self.string(path));
            }
            TextureSource::Generated(key) => {
                out.u8(1);
                out.u32(self.string(key));
            }
        }
        out.u8(match wrap {
            Wrap::Clamp => 0,
            Wrap::Repeat => 1,
            Wrap::Mirror => 2,
        });
        out.f32(repeat[0]);
        out.f32(repeat[1]);
        out.f32(offset[0]);
        out.f32(offset[1]);
        out.bool(*srgb);
        out.u8(*anisotropy);
        out.bool(*mipmaps);
        out.bool(*flip_y);
    }

    fn optional_texture(&mut self, texture: &'a Option<TextureRef>, out: &mut Writer) {
        match texture {
            Some(texture) => {
                out.u8(1);
                self.texture(texture, out);
            }
            None => out.u8(0),
        }
    }

    fn material_body(&mut self, material: &'a Material, out: &mut Writer) {
        let Material {
            shading,
            color,
            roughness,
            metalness,
            emissive,
            emissive_intensity,
            map,
            emissive_map,
            bump_map,
            bump_scale,
            vertex_colors,
            flat_shading,
            transparent,
            opacity,
            alpha_test,
            alpha_to_coverage,
            side,
            shadow_side,
            blending,
            depth_test,
            depth_write,
            polygon_offset,
            tone_mapped,
            fog,
            effect,
            extra_textures,
        } = material;
        out.u8(match shading {
            Shading::Standard => 0,
            Shading::Basic => 1,
        });
        out.u32(color.0);
        out.f32(*roughness);
        out.f32(*metalness);
        out.u32(emissive.0);
        out.f32(*emissive_intensity);
        self.optional_texture(map, out);
        self.optional_texture(emissive_map, out);
        self.optional_texture(bump_map, out);
        out.f32(*bump_scale);
        out.bool(*vertex_colors);
        out.bool(*flat_shading);
        out.bool(*transparent);
        out.f32(*opacity);
        out.f32(*alpha_test);
        out.bool(*alpha_to_coverage);
        out.u8(side_code(*side));
        out.u8(shadow_side.map_or(255, side_code));
        out.u8(match blending {
            Blending::Normal => 0,
            Blending::Additive => 1,
        });
        out.bool(*depth_test);
        out.bool(*depth_write);
        match polygon_offset {
            Some((factor, units)) => {
                out.u8(1);
                out.f32(*factor);
                out.f32(*units);
            }
            None => out.u8(0),
        }
        out.bool(*tone_mapped);
        out.bool(*fog);
        match effect {
            Effect::None => out.u8(0),
            Effect::Custom { name, params } => {
                out.u8(1);
                out.u32(self.string(name));
                out.u32(params.len() as u32);
                for &param in params {
                    out.f32(param);
                }
            }
        }
        out.u32(extra_textures.len() as u32);
        for (name, texture) in extra_textures {
            out.u32(self.string(name));
            self.texture(texture, out);
        }
    }

    fn node(&mut self, node: &'a Node, out: &mut Writer) {
        let Node {
            name,
            position,
            rotation,
            scale,
            visible,
            drawable,
            children,
        } = node;
        out.u32(self.string(name));
        for value in position.to_array() {
            out.f64(value);
        }
        for value in rotation.to_array() {
            out.f64(value);
        }
        for value in scale.to_array() {
            out.f64(value);
        }
        out.bool(*visible);
        match drawable {
            Some(drawable) => {
                out.u8(1);
                let Drawable {
                    mesh,
                    material,
                    cast_shadow,
                    receive_shadow,
                    render_order,
                    frustum_culled,
                    instances,
                } = drawable;
                out.u32(self.mesh(mesh));
                out.u32(self.material(material));
                out.bool(*cast_shadow);
                out.bool(*receive_shadow);
                out.i32(*render_order);
                out.bool(*frustum_culled);
                match instances {
                    Some(instances) => {
                        out.u8(1);
                        out.u32(instances.len() as u32);
                        for Instance { matrix, color } in instances {
                            for value in matrix.to_cols_array() {
                                out.f64(value);
                            }
                            match color {
                                Some(color) => {
                                    out.u8(1);
                                    color.iter().for_each(|&c| out.f32(c));
                                }
                                None => out.u8(0),
                            }
                        }
                    }
                    None => out.u8(0),
                }
            }
            None => out.u8(0),
        }
        out.u32(children.len() as u32);
        for child in children {
            self.node(child, out);
        }
    }
}

fn side_code(side: Side) -> u8 {
    match side {
        Side::Front => 0,
        Side::Back => 1,
        Side::Double => 2,
    }
}

/// A mesh's array lengths and attribute descriptions; its values go to the data.
fn mesh_shape(mesh: &Mesh, out: &mut Writer) {
    let Mesh {
        positions,
        normals,
        uvs,
        colors,
        indices,
        attributes,
    } = mesh;
    out.u32(positions.len() as u32);
    out.u32(normals.len() as u32);
    out.u32(uvs.len() as u32);
    out.u32(colors.len() as u32);
    match indices {
        Some(indices) => {
            out.u8(1);
            out.u32(indices.len() as u32);
        }
        None => out.u8(0),
    }
    out.u32(attributes.len() as u32);
    for attribute in attributes {
        let Attribute {
            name,
            item_size,
            data,
            per_instance,
        } = attribute;
        out.str(name);
        out.u8(*item_size);
        out.bool(*per_instance);
        out.u32(data.len() as u32);
    }
}

fn mesh_data<'a>(mesh: &'a Mesh, data: &mut Vec<&'a [u8]>) {
    data.push(bytemuck::cast_slice(&mesh.positions));
    data.push(bytemuck::cast_slice(&mesh.normals));
    data.push(bytemuck::cast_slice(&mesh.uvs));
    data.push(bytemuck::cast_slice(&mesh.colors));
    if let Some(indices) = &mesh.indices {
        data.push(bytemuck::cast_slice(indices));
    }
    for attribute in &mesh.attributes {
        data.push(bytemuck::cast_slice(&attribute.data));
    }
}

// ---------------------------------------------------------------------------
// Reading.

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }
    fn take(&mut self, count: usize) -> Result<&'a [u8], DecodeError> {
        let slice = self
            .bytes
            .get(self.at..self.at + count)
            .ok_or(DecodeError("structure ends early"))?;
        self.at += count;
        Ok(slice)
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        Ok(self.take(N)?.try_into().expect("N bytes"))
    }
    fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.take(1)?[0])
    }
    fn bool(&mut self) -> Result<bool, DecodeError> {
        Ok(self.u8()? != 0)
    }
    fn u32(&mut self) -> Result<u32, DecodeError> {
        Ok(u32::from_le_bytes(self.array()?))
    }
    fn i32(&mut self) -> Result<i32, DecodeError> {
        Ok(i32::from_le_bytes(self.array()?))
    }
    fn f32(&mut self) -> Result<f32, DecodeError> {
        Ok(f32::from_bits(self.u32()?))
    }
    fn f64(&mut self) -> Result<f64, DecodeError> {
        Ok(f64::from_bits(u64::from_le_bytes(self.array()?)))
    }
    fn str(&mut self) -> Result<&'a str, DecodeError> {
        let len = self.u32()? as usize;
        std::str::from_utf8(self.take(len)?).map_err(|_| DecodeError("text"))
    }
}

struct Decoder<'a> {
    strings: Vec<&'a str>,
    statics: Vec<Option<&'static str>>,
    meshes: Vec<Arc<Mesh>>,
    materials: Vec<Arc<Material>>,
}

/// One leaked copy of each distinct text read back as `&'static str`, for the
/// process: the builders' strings are a few dozen paths and names.
fn intern(text: &str) -> &'static str {
    static INTERNED: Mutex<Option<HashMap<String, &'static str>>> = Mutex::new(None);
    let mut interned = INTERNED.lock().unwrap_or_else(|poison| poison.into_inner());
    let map = interned.get_or_insert_with(HashMap::new);
    if let Some(&text) = map.get(text) {
        return text;
    }
    let leaked: &'static str = Box::leak(text.to_owned().into_boxed_str());
    map.insert(text.to_owned(), leaked);
    leaked
}

impl<'a> Decoder<'a> {
    fn text(&self, reader: &mut Reader) -> Result<&'a str, DecodeError> {
        let index = reader.u32()? as usize;
        self.strings
            .get(index)
            .copied()
            .ok_or(DecodeError("string index"))
    }

    fn static_text(&mut self, reader: &mut Reader) -> Result<&'static str, DecodeError> {
        let index = reader.u32()? as usize;
        let slot = self
            .statics
            .get_mut(index)
            .ok_or(DecodeError("string index"))?;
        Ok(*slot.get_or_insert_with(|| intern(self.strings[index])))
    }

    fn mesh(
        &mut self,
        reader: &mut Reader,
        offset: &mut usize,
        fill: &mut dyn FnMut(usize, &mut [u8]) -> bool,
    ) -> Result<Mesh, DecodeError> {
        fn read<T: bytemuck::Pod + bytemuck::Zeroable>(
            count: usize,
            offset: &mut usize,
            fill: &mut dyn FnMut(usize, &mut [u8]) -> bool,
        ) -> Result<Vec<T>, DecodeError> {
            let mut values = vec![T::zeroed(); count];
            let bytes: &mut [u8] = bytemuck::cast_slice_mut(&mut values);
            if !bytes.is_empty() && !fill(*offset, bytes) {
                return Err(DecodeError("data ends early"));
            }
            *offset += bytes.len();
            Ok(values)
        }
        let positions = reader.u32()? as usize;
        let normals = reader.u32()? as usize;
        let uvs = reader.u32()? as usize;
        let colors = reader.u32()? as usize;
        let indices = if reader.bool()? {
            Some(reader.u32()? as usize)
        } else {
            None
        };
        let attribute_count = reader.u32()? as usize;
        let mut attributes = Vec::with_capacity(attribute_count);
        for _ in 0..attribute_count {
            let name = intern(reader.str()?);
            let item_size = reader.u8()?;
            let per_instance = reader.bool()?;
            let len = reader.u32()? as usize;
            attributes.push((name, item_size, per_instance, len));
        }
        let mut mesh = Mesh {
            positions: read(positions, offset, fill)?,
            normals: read(normals, offset, fill)?,
            uvs: read(uvs, offset, fill)?,
            colors: read(colors, offset, fill)?,
            indices: None,
            attributes: Vec::with_capacity(attribute_count),
        };
        if let Some(count) = indices {
            mesh.indices = Some(read(count, offset, fill)?);
        }
        for (name, item_size, per_instance, len) in attributes {
            mesh.attributes.push(Attribute {
                name,
                item_size,
                data: read(len, offset, fill)?,
                per_instance,
            });
        }
        Ok(mesh)
    }

    fn texture(&mut self, reader: &mut Reader) -> Result<TextureRef, DecodeError> {
        let source = match reader.u8()? {
            0 => TextureSource::File(self.static_text(reader)?),
            1 => TextureSource::Generated(self.static_text(reader)?),
            _ => return Err(DecodeError("texture source")),
        };
        let wrap = match reader.u8()? {
            0 => Wrap::Clamp,
            1 => Wrap::Repeat,
            2 => Wrap::Mirror,
            _ => return Err(DecodeError("wrap")),
        };
        Ok(TextureRef {
            source,
            wrap,
            repeat: [reader.f32()?, reader.f32()?],
            offset: [reader.f32()?, reader.f32()?],
            srgb: reader.bool()?,
            anisotropy: reader.u8()?,
            mipmaps: reader.bool()?,
            flip_y: reader.bool()?,
        })
    }

    fn optional_texture(&mut self, reader: &mut Reader) -> Result<Option<TextureRef>, DecodeError> {
        Ok(if reader.bool()? {
            Some(self.texture(reader)?)
        } else {
            None
        })
    }

    fn material(&mut self, reader: &mut Reader) -> Result<Material, DecodeError> {
        let shading = match reader.u8()? {
            0 => Shading::Standard,
            1 => Shading::Basic,
            _ => return Err(DecodeError("shading")),
        };
        let color = Color(reader.u32()?);
        let roughness = reader.f32()?;
        let metalness = reader.f32()?;
        let emissive = Color(reader.u32()?);
        let emissive_intensity = reader.f32()?;
        let map = self.optional_texture(reader)?;
        let emissive_map = self.optional_texture(reader)?;
        let bump_map = self.optional_texture(reader)?;
        let bump_scale = reader.f32()?;
        let vertex_colors = reader.bool()?;
        let flat_shading = reader.bool()?;
        let transparent = reader.bool()?;
        let opacity = reader.f32()?;
        let alpha_test = reader.f32()?;
        let alpha_to_coverage = reader.bool()?;
        let side = side_of(reader.u8()?)?;
        let shadow_side = match reader.u8()? {
            255 => None,
            code => Some(side_of(code)?),
        };
        let blending = match reader.u8()? {
            0 => Blending::Normal,
            1 => Blending::Additive,
            _ => return Err(DecodeError("blending")),
        };
        let depth_test = reader.bool()?;
        let depth_write = reader.bool()?;
        let polygon_offset = if reader.bool()? {
            Some((reader.f32()?, reader.f32()?))
        } else {
            None
        };
        let tone_mapped = reader.bool()?;
        let fog = reader.bool()?;
        let effect = match reader.u8()? {
            0 => Effect::None,
            1 => {
                let name = self.static_text(reader)?;
                let count = reader.u32()? as usize;
                let mut params = Vec::with_capacity(count);
                for _ in 0..count {
                    params.push(reader.f32()?);
                }
                Effect::Custom { name, params }
            }
            _ => return Err(DecodeError("effect")),
        };
        let extra_count = reader.u32()? as usize;
        let mut extra_textures = Vec::with_capacity(extra_count);
        for _ in 0..extra_count {
            let name = self.static_text(reader)?;
            extra_textures.push((name, self.texture(reader)?));
        }
        Ok(Material {
            shading,
            color,
            roughness,
            metalness,
            emissive,
            emissive_intensity,
            map,
            emissive_map,
            bump_map,
            bump_scale,
            vertex_colors,
            flat_shading,
            transparent,
            opacity,
            alpha_test,
            alpha_to_coverage,
            side,
            shadow_side,
            blending,
            depth_test,
            depth_write,
            polygon_offset,
            tone_mapped,
            fog,
            effect,
            extra_textures,
        })
    }

    fn node(&mut self, reader: &mut Reader) -> Result<Node, DecodeError> {
        let name = self.text(reader)?.to_owned();
        let position = DVec3::new(reader.f64()?, reader.f64()?, reader.f64()?);
        let rotation = DQuat::from_xyzw(reader.f64()?, reader.f64()?, reader.f64()?, reader.f64()?);
        let scale = DVec3::new(reader.f64()?, reader.f64()?, reader.f64()?);
        let visible = reader.bool()?;
        let drawable = if reader.bool()? {
            let mesh = self
                .meshes
                .get(reader.u32()? as usize)
                .ok_or(DecodeError("mesh index"))?
                .clone();
            let material = self
                .materials
                .get(reader.u32()? as usize)
                .ok_or(DecodeError("material index"))?
                .clone();
            let cast_shadow = reader.bool()?;
            let receive_shadow = reader.bool()?;
            let render_order = reader.i32()?;
            let frustum_culled = reader.bool()?;
            let instances = if reader.bool()? {
                let count = reader.u32()? as usize;
                let mut instances = Vec::with_capacity(count);
                for _ in 0..count {
                    let mut columns = [0.0; 16];
                    for value in &mut columns {
                        *value = reader.f64()?;
                    }
                    let color = if reader.bool()? {
                        Some([reader.f32()?, reader.f32()?, reader.f32()?])
                    } else {
                        None
                    };
                    instances.push(Instance {
                        matrix: DMat4::from_cols_array(&columns),
                        color,
                    });
                }
                Some(instances)
            } else {
                None
            };
            Some(Drawable {
                mesh,
                material,
                cast_shadow,
                receive_shadow,
                render_order,
                frustum_culled,
                instances,
            })
        } else {
            None
        };
        let child_count = reader.u32()? as usize;
        let mut children = Vec::with_capacity(child_count);
        for _ in 0..child_count {
            children.push(self.node(reader)?);
        }
        Ok(Node {
            name,
            position,
            rotation,
            scale,
            visible,
            drawable,
            children,
        })
    }
}

fn side_of(code: u8) -> Result<Side, DecodeError> {
    match code {
        0 => Ok(Side::Front),
        1 => Ok(Side::Back),
        2 => Ok(Side::Double),
        _ => Err(DecodeError("side")),
    }
}
