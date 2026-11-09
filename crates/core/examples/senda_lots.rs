//! Senda's houses by style: how many, their widths and heights, garage sides, turns.
use sloppy_core::sim::house_layout::SENDA_STYLES;
use sloppy_core::sim::senda_layout::{HOUSE_DEPTH, HOUSE_WIDTH, senda_houses};

fn main() {
    let mut rows: std::collections::BTreeMap<u8, Vec<String>> = Default::default();
    let mut shown = std::collections::BTreeSet::new();
    for house in senda_houses() {
        let design = house.design();
        if shown.insert(house.plan.style) {
            let roof = design.roof();
            println!(
                "style {} plan: w {} d {} wall {:.3} ridge {:.3} garage {:?} door_wall {} storeys {}",
                house.plan.style,
                design.w,
                design.d,
                roof.wall,
                roof.ridge,
                design.garage,
                design.door_wall,
                design.storeys()
            );
            for index in 0..4 {
                println!("   wall {index} {:?}", design.wall_plan(index));
            }
        }
        rows.entry(house.plan.style).or_default().push(format!(
            "{:?} w{:.2} d{:.2} h{:.2} {:?} {:?}",
            house.row,
            design.w,
            design.d,
            design.h,
            house.plan.garage,
            design.roof().low_storeys
        ));
    }
    // Each style on the lot a model is drawn for: a full lot's front and depth, ridge 4.5 m,
    // garage right.
    for style in 0..SENDA_STYLES.len() as u8 {
        let plan = sloppy_core::sim::house_layout::HousePlan {
            style,
            street: 0,
            angle: 0,
            garage: sloppy_core::sim::house_layout::GarageSide::Right,
        };
        let design = sloppy_core::sim::house_layout::HouseDesign::planned(
            0.0,
            0.0,
            HOUSE_WIDTH,
            HOUSE_DEPTH,
            4.5,
            plan,
        );
        let roof = design.roof();
        println!(
            "authoring lot style {style}: w {} d {} door_wall {} wall {:.4} ridge {:.4} eave_y {:.4} pitch {:.4} ridge_half {:.3} end_run {:.3} eave_x {:.3} rake_z {:.3}",
            design.w,
            design.d,
            design.door_wall,
            roof.wall,
            roof.ridge,
            roof.eave_y,
            roof.pitch,
            roof.ridge_half,
            roof.end_run,
            roof.eave_x,
            roof.rake_z
        );
        println!("   front {:?}", design.coded_wall_plan(design.door_wall));
    }
    for (style, houses) in rows {
        let _ = SENDA_STYLES[usize::from(style)];
        println!("style {style}: {} houses", houses.len());
        let mut sizes: std::collections::BTreeMap<String, usize> = Default::default();
        for h in &houses {
            let key = h.split(' ').skip(1).collect::<Vec<_>>().join(" ");
            *sizes.entry(key).or_default() += 1;
        }
        for (k, n) in sizes {
            println!("   {n} x {k}");
        }
    }
}
