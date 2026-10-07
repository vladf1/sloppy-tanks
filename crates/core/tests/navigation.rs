//! Navigation A*: the heap search must return exactly the routes of the linear-scan search
//! it replaced, because seeded bot routes depend on its tie order (the former
//! `tests/navigation.test.ts`).

use sloppy_core::sim::math::{Random, Vec2};
use sloppy_core::sim::navigation::Navigation;

struct Route {
    path: Vec<Vec2>,
    reopened: usize,
}

/// The original linear-scan A*. Seeded bot routes depend on its tie order: among equal
/// estimates it expands the cell that entered the append-only open list first.
fn linear_scan_route(nav: &Navigation, from: Vec2, to: Vec2) -> Route {
    let cells = nav.blocked.len();
    let size = (cells as f64).sqrt() as i64;
    let start = nav.nearest(nav.index(from));
    let goal = nav.nearest(nav.index(to));
    let mut costs = vec![f64::INFINITY; cells];
    let mut parent = vec![-1i64; cells];
    let mut closed = vec![false; cells];
    let mut open = vec![start];
    costs[start] = 0.0;
    let goal_x = goal as i64 % size;
    let goal_z = goal as i64 / size;
    let heuristic = |cell: usize| {
        ((cell as i64 % size - goal_x).abs() + (cell as i64 / size - goal_z).abs()) as f64
    };
    let mut reached = start;
    let mut reopened = 0;
    while !open.is_empty() {
        let mut best = 0;
        for i in 1..open.len() {
            if costs[open[i]] + heuristic(open[i]) < costs[open[best]] + heuristic(open[best]) {
                best = i;
            }
        }
        let current = open.remove(best);
        if closed[current] {
            continue;
        }
        closed[current] = true;
        if current == goal {
            reached = current;
            break;
        }
        let x = current as i64 % size;
        let z = current as i64 / size;
        for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let nx = x + dx;
            let nz = z + dz;
            if nx < 0 || nz < 0 || nx >= size || nz >= size {
                continue;
            }
            let next = (nz * size + nx) as usize;
            if nav.blocked[next] != 0 || closed[next] {
                continue;
            }
            let cost = costs[current] + 1.0;
            if cost < costs[next] {
                reopened += if costs[next] == f64::INFINITY { 0 } else { 1 };
                costs[next] = cost;
                parent[next] = current as i64;
                open.push(next);
            }
        }
    }
    let mut path = Vec::new();
    if reached != goal {
        return Route { path, reopened };
    }
    while reached != start {
        path.push(nav.point(reached));
        reached = parent[reached] as usize;
    }
    path.reverse();
    Route { path, reopened }
}

#[test]
fn heap_a_star_returns_exactly_the_routes_of_the_linear_scan_search_it_replaced() {
    let mut random = Random::new(9001.0);
    let mut nav = Navigation::new();
    let cells = nav.blocked.len();
    let size = (cells as f64).sqrt() as usize;
    let point =
        |random: &mut Random| Vec2::new(random.range(-62.0, 62.0), random.range(-62.0, 62.0));
    let mut reopened = 0;
    let mut unreachable = 0;
    for layout in 0..12 {
        // Open ground has many equal-cost routes; scattered blocks and long walls force the
        // search to reopen cells through cheaper detours.
        nav.blocked.fill(0);
        let density = 0.05 + layout as f64 * 0.03;
        for i in 0..cells {
            nav.blocked[i] = if random.next() < density { 1 } else { 0 };
        }
        for wall in 0..layout {
            let row = random.range(2.0, size as f64 - 2.0).floor() as usize;
            let gap = random.range(0.0, size as f64).floor() as i64;
            for x in 0..size {
                if (x as i64 - gap).abs() > 1 {
                    let cell = if wall % 2 != 0 {
                        x * size + row
                    } else {
                        row * size + x
                    };
                    nav.blocked[cell] = 1;
                }
            }
        }
        for search in 0..25 {
            let from = point(&mut random);
            let to = point(&mut random);
            let expected = linear_scan_route(&nav, from, to);
            assert_eq!(
                nav.find(from, to),
                expected.path,
                "layout {layout}, search {search}"
            );
            reopened += expected.reopened;
            unreachable += if expected.path.is_empty() { 1 } else { 0 };
        }
    }
    // Both cases exercise the heap's outdated entries and a fully drained open list.
    assert!(
        reopened > 0,
        "some searches find a cheaper route to an open cell"
    );
    assert!(unreachable > 0, "some goals are walled off");
}

#[test]
fn refilling_a_route_reuses_storage_and_clears_an_unreachable_or_empty_route() {
    let mut nav = Navigation::new();
    let from = Vec2::new(-30.0, 0.0);
    let to = Vec2::new(30.0, 0.0);
    let mut path = nav.find(from, to);
    assert!(!path.is_empty());
    let pointer = path.as_ptr();
    let capacity = path.capacity();
    let expected = nav.find(to, from);
    nav.find_into(to, from, &mut path);
    assert_eq!(path, expected);
    assert_eq!(path.as_ptr(), pointer);
    assert_eq!(path.capacity(), capacity);

    // A wall spanning the whole grid makes the other half unreachable.
    let size = (nav.blocked.len() as f64).sqrt() as usize;
    for row in 0..size {
        nav.blocked[row * size + size / 2] = 1;
    }
    nav.find_into(from, to, &mut path);
    assert!(path.is_empty());
    assert_eq!(path.capacity(), capacity);
    path.push(Vec2::ZERO);
    nav.find_into(from, from, &mut path);
    assert!(path.is_empty());
    assert_eq!(path.as_ptr(), pointer);
}
