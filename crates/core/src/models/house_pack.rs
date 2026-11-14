//! The Neighborhood's house models as the page downloads them: every design in one file
//! (`public/models/houses.bin`), written from Blender's glTF exports by
//! `scripts/pack-houses.ts`, whose header describes the layout. One file because the
//! designs share most of their kit parts and Brotli finds those repeats only within one
//! stream; integer columns because Brotli shrinks them about six times smaller than
//! glTF's float arrays. Positions sit on a grid of a fraction of a millimetre, UVs are
//! projected again from the positions as Blender projected them, and normals are summed
//! from the faces round each vertex except where the file keeps one.

use crate::geometry::Mesh;

/// The format [`unpack`] reads; the packer writes the same number.
const FORMAT_VERSION: u8 = 1;
const MAGIC: &[u8; 4] = b"SLHP";

/// The objects of each design, by the design's name: each object's name
/// (`<HousePart>__<Surface>`) and mesh, its UVs in metres as Blender projected them.
pub(super) type Designs = Vec<(String, Vec<(String, Mesh)>)>;

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.at.checked_add(n).ok_or("truncated")?;
        let taken = self.bytes.get(self.at..end).ok_or("truncated")?;
        self.at = end;
        Ok(taken)
    }

    fn byte(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    /// An unsigned LEB128 varint.
    fn uint(&mut self) -> Result<usize, String> {
        let mut value = 0usize;
        for shift in (0..usize::BITS).step_by(7) {
            let byte = self.byte()?;
            value |= usize::from(byte & 0x7f)
                .checked_shl(shift)
                .ok_or("varint too long")?;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err("varint too long".into())
    }

    /// A zigzag varint.
    fn int(&mut self) -> Result<i64, String> {
        let value = self.uint()? as i64;
        Ok((value >> 1) ^ -(value & 1))
    }

    fn string(&mut self) -> Result<String, String> {
        let length = self.uint()?;
        String::from_utf8(self.take(length)?.to_vec()).map_err(|error| error.to_string())
    }
}

/// What the object table says of each object's columns.
struct Counts {
    name: String,
    vertices: usize,
    indices: usize,
    runs: usize,
    exceptions: usize,
}

/// The UV a projection code gives a point: metres along two of its axes (`u axis | v axis
/// << 2 | u negated << 4 | v negated << 5`), v flipped as glTF stores Blender's UVs.
fn projected(code: u8, p: [f64; 3]) -> Result<[f32; 2], String> {
    let (u, v) = (usize::from(code & 3), usize::from((code >> 2) & 3));
    if u > 2 || v > 2 || u == v || code >> 6 != 0 {
        return Err(format!("projection {code}"));
    }
    let sign = |negated: u8| if code & negated != 0 { -1.0 } else { 1.0 };
    Ok([(sign(16) * p[u]) as f32, (1.0 + sign(32) * p[v]) as f32])
}

/// Each vertex's normal: the sum of its triangles' areas along their normals. Flat-shaded
/// faces split their vertices at every crease, so this is the face's own normal.
fn summed_normals(positions: &[[f64; 3]], indices: &[u32]) -> Vec<[f32; 3]> {
    let mut sums = vec![glam::DVec3::ZERO; positions.len()];
    for triangle in indices.as_chunks::<3>().0 {
        let [a, b, c] = triangle.map(|i| glam::DVec3::from(positions[i as usize]));
        let area = (b - a).cross(c - a);
        for &i in triangle {
            sums[i as usize] += area;
        }
    }
    sums.iter()
        .map(|sum| {
            let normal = sum.try_normalize().unwrap_or(glam::DVec3::Y);
            [normal.x as f32, normal.y as f32, normal.z as f32]
        })
        .collect()
}

/// Every design a pack holds, in its order.
pub(super) fn unpack(bytes: &[u8]) -> Result<Designs, String> {
    let mut read = Reader { bytes, at: 0 };
    if read.take(4)? != MAGIC {
        return Err("not a house pack".into());
    }
    let version = read.byte()?;
    if version != FORMAT_VERSION {
        return Err(format!("house pack version {version}"));
    }
    let step_bits = read.byte()?;
    if step_bits > 30 {
        return Err(format!("position step 2^-{step_bits}"));
    }
    let step = (-f64::from(step_bits)).exp2();
    let mut designs = Vec::new();
    for _ in 0..read.uint()? {
        designs.push((read.string()?, read.uint()?));
    }
    let objects = designs.iter().map(|(_, objects)| objects).sum();
    let mut table = Vec::new();
    for _ in 0..objects {
        table.push(Counts {
            name: read.string()?,
            vertices: read.uint()?,
            indices: read.uint()?,
            runs: read.uint()?,
            exceptions: read.uint()?,
        });
    }
    let total = |count: fn(&Counts) -> usize| -> usize { table.iter().map(count).sum() };
    let column_bytes = total(|c| c.vertices)
        .checked_mul(2)
        .ok_or("truncated")?;
    let columns = [(); 3].map(|()| read.take(column_bytes));
    let [x, y, z] = columns.map(|column| column.map(|bytes| bytes.as_chunks::<2>().0));
    let coordinate = |value: &[u8; 2]| f64::from(i16::from_le_bytes(*value)) * step;
    let points: Vec<[f64; 3]> = x?
        .iter()
        .zip(y?)
        .zip(z?)
        .map(|((x, y), z)| [coordinate(x), coordinate(y), coordinate(z)])
        .collect();
    let mut indices = Vec::new();
    for counts in &table {
        let mut highest = -1i64;
        for _ in 0..counts.indices {
            let index = highest + 1 - read.int()?;
            if !(0..counts.vertices as i64).contains(&index) {
                return Err(format!("{}: index {index}", counts.name));
            }
            highest = highest.max(index);
            indices.push(index as u32);
        }
    }
    let mut run_lengths = Vec::new();
    for _ in 0..total(|c| c.runs) {
        run_lengths.push(read.uint()?);
    }
    let run_codes = read.take(run_lengths.len())?;
    let mut exception_vertices = Vec::new();
    for counts in &table {
        let mut vertex = 0;
        for _ in 0..counts.exceptions {
            vertex += read.uint()?;
            if vertex >= counts.vertices {
                return Err(format!("{}: normal of vertex {vertex}", counts.name));
            }
            exception_vertices.push(vertex);
        }
    }
    let exception_normals = read.take(exception_vertices.len() * 3)?;
    if read.at != bytes.len() {
        return Err("bytes after the house pack".into());
    }

    let (mut points, mut indices) = (points.as_slice(), indices.as_slice());
    let mut runs = run_lengths.iter().zip(run_codes);
    let mut exceptions = exception_vertices
        .iter()
        .zip(exception_normals.as_chunks::<3>().0);
    let mut table = table.into_iter();
    let mut unpacked = Vec::new();
    for (design, count) in designs {
        let mut objects = Vec::new();
        for counts in table.by_ref().take(count) {
            let (object_points, rest) = points.split_at(counts.vertices);
            points = rest;
            let (object_indices, rest) = indices.split_at(counts.indices);
            indices = rest;
            let mut uvs = Vec::with_capacity(counts.vertices);
            for (&length, &code) in runs.by_ref().take(counts.runs) {
                for &point in object_points.get(uvs.len()..uvs.len() + length).ok_or("runs past the vertices")? {
                    uvs.push(projected(code, point)?);
                }
            }
            if uvs.len() != counts.vertices {
                return Err(format!("{}: UV runs for {} vertices", counts.name, uvs.len()));
            }
            let mut normals = summed_normals(object_points, object_indices);
            for (&vertex, normal) in exceptions.by_ref().take(counts.exceptions) {
                let n = glam::Vec3::from(normal.map(|c| f32::from(c as i8) / 127.0));
                normals[vertex] = n.try_normalize().ok_or("a zero normal")?.into();
            }
            let mesh = Mesh {
                positions: object_points.iter().map(|p| p.map(|c| c as f32)).collect(),
                normals,
                uvs,
                indices: Some(object_indices.to_vec()),
                ..Mesh::default()
            };
            objects.push((counts.name, mesh));
        }
        unpacked.push((design, objects));
    }
    Ok(unpacked)
}
