//! Find the functions that can be deferred without guessing which menu paths players use.
use std::collections::HashSet;
use std::{env, fs};
use walrus::ir::{InstrLocId, Unreachable};
use walrus::{ModuleConfig, passes};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let [input, named, deferred]: [String; 3] = env::args()
        .skip(1)
        .collect::<Vec<_>>()
        .try_into()
        .map_err(|_| "expected input.wasm named.wasm deferred.txt")?;
    let mut module = ModuleConfig::new()
        .generate_name_section(true)
        .parse_file(input)?;

    // Binaryen accepts internal names. Give each body a build-local name, including
    // anonymous LLVM functions, before making a separate copy for the reachability pass.
    let local: Vec<_> = module.funcs.iter_local().map(|(id, _)| id).collect();
    for id in &local {
        module.funcs.get_mut(*id).name = Some(format!("body{}", id.index()));
    }
    module.emit_wasm_file(named)?;

    // Every call to these bodies happens after installation. Treat them as leaves
    // only in this analysis copy. Walrus follows exports, start, table elements and
    // function references, so resize, input, error paths and worker entry points all
    // stay callable, including paths no test or training profile has ever exercised.
    let mut gates = HashSet::new();
    for name in [
        // The shell always constructs Game with defer_arena. Its current arena
        // is built by advance_game; a room can be entered only after attachment.
        "sloppy_build_arena",
        "sloppy_arena_scenery",
        "sloppy_advance_game",
        "sloppy_game_command",
        "sloppy_begin_round",
        "sloppy_draw_battle",
        "sloppy_room_redraw",
        "sloppy_room_network",
    ] {
        let id = module.exports.get_func(name)?;
        gates.insert(id);
        let body = module.funcs.get_mut(id).kind.unwrap_local_mut();
        let block = body.block_mut(body.entry_block());
        block.instrs.clear();
        block
            .instrs
            .push((Unreachable {}.into(), InstrLocId::default()));
    }
    passes::gc::run(&mut module);
    let early: HashSet<_> = module.funcs.iter_local().map(|(id, _)| id).collect();
    let late: Vec<_> = local
        .into_iter()
        .filter(|id| gates.contains(id) || !early.contains(id))
        .map(|id| format!("body{}", id.index()))
        .collect();
    fs::write(deferred, late.join("\n"))?;
    println!("Deferring {} function bodies", late.len());
    Ok(())
}
