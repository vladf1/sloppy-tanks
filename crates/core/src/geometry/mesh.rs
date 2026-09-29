/// One vertex attribute beyond the standard set, read only by the effect shader that
/// names it (for example a foliage sway weight). `item_size` floats per vertex.
#[derive(Clone, Debug, PartialEq)]
pub struct Attribute {
    pub name: &'static str,
    pub item_size: u8,
    pub data: Vec<f32>,
}

/// A triangle list in model units (metres). `normals`, `uvs` and `colors` are either
/// empty or have one entry per position. Without `indices`, every three positions
/// form a triangle, like a non-indexed Three.js BufferGeometry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    /// Linear RGB vertex colors, used when the material enables vertex colors.
    pub colors: Vec<[f32; 3]>,
    pub indices: Option<Vec<u32>>,
    pub attributes: Vec<Attribute>,
}

impl Mesh {
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.as_ref().map_or(self.positions.len(), Vec::len) / 3
    }

    pub fn attribute(&self, name: &str) -> Option<&Attribute> {
        self.attributes
            .iter()
            .find(|attribute| attribute.name == name)
    }
}
