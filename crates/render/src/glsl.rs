//! WGSL remains authoritative. Naga's reflection supplies the linked GLSL names;
//! resource slots below are the game's fixed frame/material layout, not a general
//! bind-group implementation.
use naga::{ShaderStage, back::glsl};

pub struct Binding {
    pub name: String,
    pub slot: u32,
}
pub struct Stage {
    pub source: String,
    pub blocks: Vec<Binding>,
    pub textures: Vec<Binding>,
}

pub fn translate(code: &str, entry: &str, stage: ShaderStage) -> Result<Stage, String> {
    let module = naga::front::wgsl::parse_str(code).map_err(|e| e.emit_to_string(code))?;
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .map_err(|e| e.to_string())?;
    let options = glsl::Options {
        version: glsl::Version::Embedded {
            version: 300,
            is_webgl: true,
        },
        writer_flags: glsl::WriterFlags::ADJUST_COORDINATE_SPACE,
        ..Default::default()
    };
    let mut source = String::new();
    let reflection = glsl::Writer::new(
        &mut source,
        &module,
        &info,
        &options,
        &glsl::PipelineOptions {
            shader_stage: stage,
            entry_point: entry.into(),
            multiview: None,
        },
        Default::default(),
    )
    .map_err(|e| e.to_string())?
    .write()
    .map_err(|e| e.to_string())?;
    let mut blocks = Vec::new();
    for (handle, name) in reflection.uniforms {
        if let Some(binding) = &module.global_variables[handle].binding {
            blocks.push(Binding {
                name,
                slot: binding.group,
            });
        }
    }
    let mut textures = Vec::new();
    for (name, mapping) in reflection.texture_mapping {
        let binding = module.global_variables[mapping.texture]
            .binding
            .as_ref()
            .ok_or("unbound texture")?;
        let slot = texture_slot(binding.group, binding.binding)?;
        textures.push(Binding { name, slot });
    }
    Ok(Stage {
        source,
        blocks,
        textures,
    })
}
fn texture_slot(group: u32, binding: u32) -> Result<u32, String> {
    match (group, binding) {
        (0, 0 | 1) => Ok(0),
        (0, 3) => Ok(1),
        (0, 5) => Ok(2),
        (1, 1 | 3 | 5 | 7 | 9) => Ok(3 + (binding - 1) / 2),
        _ => Err(format!("unsupported texture binding {group}:{binding}")),
    }
}
pub const EMPTY_FRAGMENT: &str = "@fragment fn empty() {}";

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn depth_only_program_has_a_valid_empty_fragment_stage() {
        let stage = translate(EMPTY_FRAGMENT, "empty", ShaderStage::Fragment).unwrap();
        assert!(stage.blocks.is_empty() && stage.textures.is_empty());
        assert!(stage.source.contains("void main()"));
    }
    #[test]
    fn resource_slots_match_frame_and_material_layouts() {
        assert_eq!(
            [texture_slot(0, 1), texture_slot(0, 3), texture_slot(0, 5)],
            [Ok(0), Ok(1), Ok(2)]
        );
        assert_eq!(
            (1..=9)
                .step_by(2)
                .map(|b| texture_slot(1, b).unwrap())
                .collect::<Vec<_>>(),
            vec![3, 4, 5, 6, 7]
        );
        assert!(texture_slot(2, 0).is_err());
    }
    #[test]
    fn instance_index_keeps_nagas_base_instance_uniform() {
        let source = "@vertex fn test(@builtin(instance_index) i:u32)->@builtin(position) vec4f{return vec4f(f32(i),0.,0.,1.);}";
        let stage = translate(source, "test", ShaderStage::Vertex).unwrap();
        assert!(stage.source.contains("naga_vs_first_instance"));
        assert!(stage.source.contains("gl_InstanceID"));
    }
}
