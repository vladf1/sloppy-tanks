//! Polygon triangulation: a port of mapbox/earcut 3.0.2 as bundled in Three.js r185
//! (`src/extras/lib/earcut.js`), and Three's `ShapeUtils` wrappers. Triangle order
//! and vertex order match the JavaScript, which matters because generated meshes
//! keep that order.
//!
//! The JS doubly linked list becomes an arena of nodes addressed by index. Removed
//! nodes keep their own links, as earcut relies on (`p = p.prev` after removal).

use glam::DVec2;

/// `ShapeUtils.area`: signed area, positive for counter-clockwise contours.
pub fn shape_area(contour: &[DVec2]) -> f64 {
    let n = contour.len();
    let mut a = 0.0;
    let mut p = n.wrapping_sub(1);
    for q in 0..n {
        a += contour[p].x * contour[q].y - contour[q].x * contour[p].y;
        p = q;
    }
    a * 0.5
}

/// `ShapeUtils.isClockWise`.
pub fn is_clockwise(points: &[DVec2]) -> bool {
    shape_area(points) < 0.0
}

/// `ShapeUtils.triangulateShape(contour, holes)`: triangles as vertex indices into
/// the contour followed by each hole. Like Three, this first drops a duplicated
/// closing point from the contour and from each hole, in place.
pub fn triangulate_shape(contour: &mut Vec<DVec2>, holes: &mut [Vec<DVec2>]) -> Vec<[usize; 3]> {
    remove_duplicate_end_point(contour);
    let mut vertices: Vec<f64> = contour.iter().flat_map(|p| [p.x, p.y]).collect();
    let mut hole_indices = Vec::with_capacity(holes.len());
    let mut hole_index = contour.len();
    for hole in holes.iter_mut() {
        remove_duplicate_end_point(hole);
    }
    for hole in holes.iter() {
        hole_indices.push(hole_index);
        hole_index += hole.len();
        vertices.extend(hole.iter().flat_map(|p| [p.x, p.y]));
    }
    earcut(&vertices, &hole_indices)
        .as_chunks::<3>()
        .0
        .iter()
        .map(|t| [t[0], t[1], t[2]])
        .collect()
}

fn remove_duplicate_end_point(points: &mut Vec<DVec2>) {
    let l = points.len();
    if l > 2 && points[l - 1] == points[0] {
        points.pop();
    }
}

type Link = Option<usize>;

#[derive(Clone, Debug)]
struct Node {
    /// Vertex index in the flat coordinate array.
    i: usize,
    x: f64,
    y: f64,
    prev: usize,
    next: usize,
    z: i32,
    prev_z: Link,
    next_z: Link,
    steiner: bool,
}

struct Earcut<'a> {
    data: &'a [f64],
    nodes: Vec<Node>,
    triangles: Vec<usize>,
    min_x: f64,
    min_y: f64,
    inv_size: f64,
}

/// `earcut(data, holeIndices)` for 2D coordinates: flat triangle vertex indices.
pub fn earcut(data: &[f64], hole_indices: &[usize]) -> Vec<usize> {
    let dim = 2;
    let has_holes = !hole_indices.is_empty();
    let outer_len = if has_holes {
        hole_indices[0] * dim
    } else {
        data.len()
    };
    let mut state = Earcut {
        data,
        nodes: Vec::with_capacity(data.len() / dim * 3 / 2 + 8),
        triangles: Vec::new(),
        min_x: 0.0,
        min_y: 0.0,
        inv_size: 0.0,
    };
    let Some(mut outer_node) = state.linked_list(0, outer_len, true) else {
        return state.triangles;
    };
    if state.next(outer_node) == state.prev(outer_node) {
        return state.triangles;
    }
    if has_holes {
        outer_node = state.eliminate_holes(hole_indices, outer_node);
    }
    // For larger shapes, a z-order curve hash speeds up the ear tests.
    if data.len() > 80 * dim {
        let mut min_x = data[0];
        let mut min_y = data[1];
        let mut max_x = min_x;
        let mut max_y = min_y;
        let mut i = dim;
        while i < outer_len {
            let (x, y) = (data[i], data[i + 1]);
            if x < min_x {
                min_x = x;
            }
            if y < min_y {
                min_y = y;
            }
            if x > max_x {
                max_x = x;
            }
            if y > max_y {
                max_y = y;
            }
            i += dim;
        }
        let size = (max_x - min_x).max(max_y - min_y);
        state.min_x = min_x;
        state.min_y = min_y;
        state.inv_size = if size != 0.0 { 32767.0 / size } else { 0.0 };
    }
    state.earcut_linked(Some(outer_node), 0);
    state.triangles
}

impl Earcut<'_> {
    fn prev(&self, n: usize) -> usize {
        self.nodes[n].prev
    }

    fn next(&self, n: usize) -> usize {
        self.nodes[n].next
    }

    fn xy(&self, n: usize) -> (f64, f64) {
        (self.nodes[n].x, self.nodes[n].y)
    }

    fn create_node(&mut self, i: usize, x: f64, y: f64) -> usize {
        let id = self.nodes.len();
        self.nodes.push(Node {
            i,
            x,
            y,
            prev: id,
            next: id,
            z: 0,
            prev_z: None,
            next_z: None,
            steiner: false,
        });
        id
    }

    fn insert_node(&mut self, i: usize, x: f64, y: f64, last: Link) -> usize {
        let p = self.create_node(i, x, y);
        if let Some(last) = last {
            let last_next = self.next(last);
            self.nodes[p].next = last_next;
            self.nodes[p].prev = last;
            self.nodes[last_next].prev = p;
            self.nodes[last].next = p;
        }
        p
    }

    fn remove_node(&mut self, p: usize) {
        let (prev, next) = (self.prev(p), self.next(p));
        self.nodes[next].prev = prev;
        self.nodes[prev].next = next;
        let (prev_z, next_z) = (self.nodes[p].prev_z, self.nodes[p].next_z);
        if let Some(prev_z) = prev_z {
            self.nodes[prev_z].next_z = next_z;
        }
        if let Some(next_z) = next_z {
            self.nodes[next_z].prev_z = prev_z;
        }
    }

    fn equals(&self, a: usize, b: usize) -> bool {
        self.nodes[a].x == self.nodes[b].x && self.nodes[a].y == self.nodes[b].y
    }

    /// Signed area of a triangle.
    fn area(&self, p: usize, q: usize, r: usize) -> f64 {
        let (px, py) = self.xy(p);
        let (qx, qy) = self.xy(q);
        let (rx, ry) = self.xy(r);
        (qy - py) * (rx - qx) - (qx - px) * (ry - qy)
    }

    fn signed_area(&self, start: usize, end: usize) -> f64 {
        let data = self.data;
        let mut sum = 0.0;
        let mut j = end - 2;
        let mut i = start;
        while i < end {
            sum += (data[j] - data[i]) * (data[i + 1] + data[j + 1]);
            j = i;
            i += 2;
        }
        sum
    }

    /// A circular list of the points in `start..end`, in the requested winding.
    fn linked_list(&mut self, start: usize, end: usize, clockwise: bool) -> Link {
        let mut last: Link = None;
        if start >= end {
            return None;
        }
        if clockwise == (self.signed_area(start, end) > 0.0) {
            let mut i = start;
            while i < end {
                last = Some(self.insert_node(i / 2, self.data[i], self.data[i + 1], last));
                i += 2;
            }
        } else {
            let mut i = end - 2;
            loop {
                last = Some(self.insert_node(i / 2, self.data[i], self.data[i + 1], last));
                if i < start + 2 {
                    break;
                }
                i -= 2;
            }
        }
        if let Some(l) = last
            && self.equals(l, self.next(l))
        {
            self.remove_node(l);
            last = Some(self.next(l));
        }
        last
    }

    /// Remove collinear or duplicate points.
    fn filter_points(&mut self, start: Link, end: Link) -> Link {
        let start = start?;
        let mut end = end.unwrap_or(start);
        let mut p = start;
        loop {
            let mut again = false;
            let (prev, next) = (self.prev(p), self.next(p));
            if !self.nodes[p].steiner && (self.equals(p, next) || self.area(prev, p, next) == 0.0) {
                self.remove_node(p);
                p = self.prev(p);
                end = p;
                if p == self.next(p) {
                    break;
                }
                again = true;
            } else {
                p = self.next(p);
            }
            if !(again || p != end) {
                break;
            }
        }
        Some(end)
    }

    /// The main ear slicing loop.
    fn earcut_linked(&mut self, ear: Link, pass: u8) {
        let Some(mut ear) = ear else {
            return;
        };
        if pass == 0 && self.inv_size != 0.0 {
            self.index_curve(ear);
        }
        let mut stop = ear;
        while self.prev(ear) != self.next(ear) {
            let (prev, next) = (self.prev(ear), self.next(ear));
            let is_ear = if self.inv_size != 0.0 {
                self.is_ear_hashed(ear)
            } else {
                self.is_ear(ear)
            };
            if is_ear {
                self.triangles
                    .extend([self.nodes[prev].i, self.nodes[ear].i, self.nodes[next].i]);
                self.remove_node(ear);
                // Skipping the next vertex leads to fewer sliver triangles.
                ear = self.next(next);
                stop = ear;
                continue;
            }
            ear = next;
            if ear == stop {
                match pass {
                    0 => {
                        let filtered = self.filter_points(Some(ear), None);
                        self.earcut_linked(filtered, 1);
                    }
                    1 => {
                        let filtered = self.filter_points(Some(ear), None);
                        let cured = self.cure_local_intersections(filtered);
                        self.earcut_linked(cured, 2);
                    }
                    2 => self.split_earcut(ear),
                    _ => {}
                }
                break;
            }
        }
    }

    fn is_ear(&self, ear: usize) -> bool {
        let (a, b, c) = (self.prev(ear), ear, self.next(ear));
        if self.area(a, b, c) >= 0.0 {
            return false;
        }
        let (ax, ay) = self.xy(a);
        let (bx, by) = self.xy(b);
        let (cx, cy) = self.xy(c);
        let (x0, y0) = (ax.min(bx).min(cx), ay.min(by).min(cy));
        let (x1, y1) = (ax.max(bx).max(cx), ay.max(by).max(cy));
        let mut p = self.next(c);
        while p != a {
            let (px, py) = self.xy(p);
            if px >= x0
                && px <= x1
                && py >= y0
                && py <= y1
                && point_in_triangle_except_first(ax, ay, bx, by, cx, cy, px, py)
                && self.area(self.prev(p), p, self.next(p)) >= 0.0
            {
                return false;
            }
            p = self.next(p);
        }
        true
    }

    fn blocks_ear(&self, n: usize, a: usize, c: usize, corners: [f64; 6], bbox: [f64; 4]) -> bool {
        let [ax, ay, bx, by, cx, cy] = corners;
        let [x0, y0, x1, y1] = bbox;
        let (px, py) = self.xy(n);
        px >= x0
            && px <= x1
            && py >= y0
            && py <= y1
            && n != a
            && n != c
            && point_in_triangle_except_first(ax, ay, bx, by, cx, cy, px, py)
            && self.area(self.prev(n), n, self.next(n)) >= 0.0
    }

    fn is_ear_hashed(&self, ear: usize) -> bool {
        let (a, b, c) = (self.prev(ear), ear, self.next(ear));
        if self.area(a, b, c) >= 0.0 {
            return false;
        }
        let (ax, ay) = self.xy(a);
        let (bx, by) = self.xy(b);
        let (cx, cy) = self.xy(c);
        let (x0, y0) = (ax.min(bx).min(cx), ay.min(by).min(cy));
        let (x1, y1) = (ax.max(bx).max(cx), ay.max(by).max(cy));
        let corners = [ax, ay, bx, by, cx, cy];
        let bbox = [x0, y0, x1, y1];
        let min_z = z_order(x0, y0, self.min_x, self.min_y, self.inv_size);
        let max_z = z_order(x1, y1, self.min_x, self.min_y, self.inv_size);
        let mut p = self.nodes[ear].prev_z;
        let mut n = self.nodes[ear].next_z;
        while let (Some(pp), Some(nn)) = (p, n) {
            if !(self.nodes[pp].z >= min_z && self.nodes[nn].z <= max_z) {
                break;
            }
            if self.blocks_ear(pp, a, c, corners, bbox) {
                return false;
            }
            p = self.nodes[pp].prev_z;
            if self.blocks_ear(nn, a, c, corners, bbox) {
                return false;
            }
            n = self.nodes[nn].next_z;
        }
        while let Some(pp) = p {
            if self.nodes[pp].z < min_z {
                break;
            }
            if self.blocks_ear(pp, a, c, corners, bbox) {
                return false;
            }
            p = self.nodes[pp].prev_z;
        }
        while let Some(nn) = n {
            if self.nodes[nn].z > max_z {
                break;
            }
            if self.blocks_ear(nn, a, c, corners, bbox) {
                return false;
            }
            n = self.nodes[nn].next_z;
        }
        true
    }

    /// Cure small local self-intersections.
    fn cure_local_intersections(&mut self, start: Link) -> Link {
        let mut start = start?;
        let mut p = start;
        loop {
            let a = self.prev(p);
            let b = self.next(self.next(p));
            if !self.equals(a, b)
                && self.intersects(a, p, self.next(p), b)
                && self.locally_inside(a, b)
                && self.locally_inside(b, a)
            {
                self.triangles
                    .extend([self.nodes[a].i, self.nodes[p].i, self.nodes[b].i]);
                self.remove_node(p);
                let p_next = self.next(p);
                self.remove_node(p_next);
                p = b;
                start = b;
            }
            p = self.next(p);
            if p == start {
                break;
            }
        }
        self.filter_points(Some(p), None)
    }

    /// Split the polygon along a valid diagonal and triangulate both halves.
    fn split_earcut(&mut self, start: usize) {
        let mut a = start;
        loop {
            let mut b = self.next(self.next(a));
            while b != self.prev(a) {
                if self.nodes[a].i != self.nodes[b].i && self.is_valid_diagonal(a, b) {
                    let c = self.split_polygon(a, b);
                    let a_next = self.next(a);
                    let a = self.filter_points(Some(a), Some(a_next));
                    let c_next = self.next(c);
                    let c = self.filter_points(Some(c), Some(c_next));
                    self.earcut_linked(a, 0);
                    self.earcut_linked(c, 0);
                    return;
                }
                b = self.next(b);
            }
            a = self.next(a);
            if a == start {
                break;
            }
        }
    }

    /// Link every hole into the outer ring, leftmost hole first.
    fn eliminate_holes(&mut self, hole_indices: &[usize], mut outer_node: usize) -> usize {
        let mut queue = Vec::with_capacity(hole_indices.len());
        for (k, &hole) in hole_indices.iter().enumerate() {
            let start = hole * 2;
            let end = if k + 1 < hole_indices.len() {
                hole_indices[k + 1] * 2
            } else {
                self.data.len()
            };
            if let Some(list) = self.linked_list(start, end, false) {
                if list == self.next(list) {
                    self.nodes[list].steiner = true;
                }
                queue.push(self.get_leftmost(list));
            }
        }
        // Array.prototype.sort is stable; so is sort_by.
        queue.sort_by(|&a, &b| {
            self.compare_x_y_slope(a, b)
                .partial_cmp(&0.0)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for hole in queue {
            outer_node = self.eliminate_hole(hole, outer_node);
        }
        outer_node
    }

    fn compare_x_y_slope(&self, a: usize, b: usize) -> f64 {
        let (na, nb) = (&self.nodes[a], &self.nodes[b]);
        let mut result = na.x - nb.x;
        if result == 0.0 {
            result = na.y - nb.y;
            if result == 0.0 {
                let (a_next, b_next) = (&self.nodes[na.next], &self.nodes[nb.next]);
                let a_slope = (a_next.y - na.y) / (a_next.x - na.x);
                let b_slope = (b_next.y - nb.y) / (b_next.x - nb.x);
                result = a_slope - b_slope;
            }
        }
        result
    }

    fn eliminate_hole(&mut self, hole: usize, outer_node: usize) -> usize {
        let Some(bridge) = self.find_hole_bridge(hole, outer_node) else {
            return outer_node;
        };
        let bridge_reverse = self.split_polygon(bridge, hole);
        let reverse_next = self.next(bridge_reverse);
        self.filter_points(Some(bridge_reverse), Some(reverse_next));
        let bridge_next = self.next(bridge);
        self.filter_points(Some(bridge), Some(bridge_next))
            .unwrap_or(outer_node)
    }

    /// David Eberly's algorithm for a bridge between a hole and the outer ring.
    fn find_hole_bridge(&self, hole: usize, outer_node: usize) -> Link {
        let mut p = outer_node;
        let (hx, hy) = self.xy(hole);
        let mut qx = f64::NEG_INFINITY;
        let mut m: Link = None;
        if self.equals(hole, p) {
            return Some(p);
        }
        loop {
            let next = self.next(p);
            if self.equals(hole, next) {
                return Some(next);
            }
            let (px, py) = self.xy(p);
            let (nx, ny) = self.xy(next);
            if hy <= py && hy >= ny && ny != py {
                let x = px + (hy - py) * (nx - px) / (ny - py);
                if x <= hx && x > qx {
                    qx = x;
                    let candidate = if px < nx { p } else { next };
                    m = Some(candidate);
                    if x == hx {
                        return m;
                    }
                }
            }
            p = next;
            if p == outer_node {
                break;
            }
        }
        let mut m = m?;
        let stop = m;
        let (mx, my) = self.xy(m);
        let mut tan_min = f64::INFINITY;
        p = m;
        loop {
            let (px, py) = self.xy(p);
            if hx >= px
                && px >= mx
                && hx != px
                && point_in_triangle(
                    if hy < my { hx } else { qx },
                    hy,
                    mx,
                    my,
                    if hy < my { qx } else { hx },
                    hy,
                    px,
                    py,
                )
            {
                let tan = (hy - py).abs() / (hx - px);
                let (m_x, _) = self.xy(m);
                if self.locally_inside(p, hole)
                    && (tan < tan_min
                        || (tan == tan_min
                            && (px > m_x || (px == m_x && self.sector_contains_sector(m, p)))))
                {
                    m = p;
                    tan_min = tan;
                }
            }
            p = self.next(p);
            if p == stop {
                break;
            }
        }
        Some(m)
    }

    fn sector_contains_sector(&self, m: usize, p: usize) -> bool {
        self.area(self.prev(m), m, self.prev(p)) < 0.0
            && self.area(self.next(p), m, self.next(m)) < 0.0
    }

    /// Interlink polygon nodes in z-order.
    fn index_curve(&mut self, start: usize) {
        let mut p = start;
        loop {
            if self.nodes[p].z == 0 {
                let (x, y) = self.xy(p);
                self.nodes[p].z = z_order(x, y, self.min_x, self.min_y, self.inv_size);
            }
            self.nodes[p].prev_z = Some(self.prev(p));
            self.nodes[p].next_z = Some(self.next(p));
            p = self.next(p);
            if p == start {
                break;
            }
        }
        let tail = self.nodes[p].prev_z.expect("linked in the loop above");
        self.nodes[tail].next_z = None;
        self.nodes[p].prev_z = None;
        self.sort_linked(p);
    }

    /// Simon Tatham's linked list merge sort on the z links.
    fn sort_linked(&mut self, list: usize) {
        let mut list: Link = Some(list);
        let mut in_size = 1;
        loop {
            let mut p = list;
            list = None;
            let mut tail: Link = None;
            let mut num_merges = 0;
            while p.is_some() {
                num_merges += 1;
                let mut q = p;
                let mut p_size = 0;
                for _ in 0..in_size {
                    p_size += 1;
                    q = q.and_then(|n| self.nodes[n].next_z);
                    if q.is_none() {
                        break;
                    }
                }
                let mut q_size = in_size;
                while p_size > 0 || (q_size > 0 && q.is_some()) {
                    let take_p = p_size != 0
                        && (q_size == 0
                            || match (p, q) {
                                (Some(pn), Some(qn)) => self.nodes[pn].z <= self.nodes[qn].z,
                                _ => true,
                            });
                    let e = if take_p {
                        let e = p.expect("p still holds p_size nodes");
                        p = self.nodes[e].next_z;
                        p_size -= 1;
                        e
                    } else {
                        let e = q.expect("q is non-empty here");
                        q = self.nodes[e].next_z;
                        q_size -= 1;
                        e
                    };
                    match tail {
                        Some(t) => self.nodes[t].next_z = Some(e),
                        None => list = Some(e),
                    }
                    self.nodes[e].prev_z = tail;
                    tail = Some(e);
                }
                p = q;
            }
            if let Some(t) = tail {
                self.nodes[t].next_z = None;
            }
            in_size *= 2;
            if num_merges <= 1 {
                break;
            }
        }
    }

    fn get_leftmost(&self, start: usize) -> usize {
        let mut p = start;
        let mut leftmost = start;
        loop {
            let (px, py) = self.xy(p);
            let (lx, ly) = self.xy(leftmost);
            if px < lx || (px == lx && py < ly) {
                leftmost = p;
            }
            p = self.next(p);
            if p == start {
                break;
            }
        }
        leftmost
    }

    fn is_valid_diagonal(&self, a: usize, b: usize) -> bool {
        let (na, nb) = (&self.nodes[a], &self.nodes[b]);
        self.nodes[na.next].i != nb.i
            && self.nodes[na.prev].i != nb.i
            && !self.intersects_polygon(a, b)
            && ((self.locally_inside(a, b)
                && self.locally_inside(b, a)
                && self.middle_inside(a, b)
                && (truthy(self.area(self.prev(a), a, self.prev(b)))
                    || truthy(self.area(a, self.prev(b), b))))
                || (self.equals(a, b)
                    && self.area(self.prev(a), a, self.next(a)) > 0.0
                    && self.area(self.prev(b), b, self.next(b)) > 0.0))
    }

    fn intersects(&self, p1: usize, q1: usize, p2: usize, q2: usize) -> bool {
        let o1 = sign(self.area(p1, q1, p2));
        let o2 = sign(self.area(p1, q1, q2));
        let o3 = sign(self.area(p2, q2, p1));
        let o4 = sign(self.area(p2, q2, q1));
        if o1 != o2 && o3 != o4 {
            return true;
        }
        (o1 == 0 && self.on_segment(p1, p2, q1))
            || (o2 == 0 && self.on_segment(p1, q2, q1))
            || (o3 == 0 && self.on_segment(p2, p1, q2))
            || (o4 == 0 && self.on_segment(p2, q1, q2))
    }

    /// For collinear points p, q, r: whether q lies on segment pr.
    fn on_segment(&self, p: usize, q: usize, r: usize) -> bool {
        let (px, py) = self.xy(p);
        let (qx, qy) = self.xy(q);
        let (rx, ry) = self.xy(r);
        qx <= px.max(rx) && qx >= px.min(rx) && qy <= py.max(ry) && qy >= py.min(ry)
    }

    fn intersects_polygon(&self, a: usize, b: usize) -> bool {
        let (ai, bi) = (self.nodes[a].i, self.nodes[b].i);
        let mut p = a;
        loop {
            let next = self.next(p);
            let (pi, ni) = (self.nodes[p].i, self.nodes[next].i);
            if pi != ai && ni != ai && pi != bi && ni != bi && self.intersects(p, next, a, b) {
                return true;
            }
            p = next;
            if p == a {
                break;
            }
        }
        false
    }

    fn locally_inside(&self, a: usize, b: usize) -> bool {
        let (prev, next) = (self.prev(a), self.next(a));
        if self.area(prev, a, next) < 0.0 {
            self.area(a, b, next) >= 0.0 && self.area(a, prev, b) >= 0.0
        } else {
            self.area(a, b, prev) < 0.0 || self.area(a, next, b) < 0.0
        }
    }

    fn middle_inside(&self, a: usize, b: usize) -> bool {
        let mut p = a;
        let mut inside = false;
        let (ax, ay) = self.xy(a);
        let (bx, by) = self.xy(b);
        let (px, py) = ((ax + bx) / 2.0, (ay + by) / 2.0);
        loop {
            let next = self.next(p);
            let (x, y) = self.xy(p);
            let (nx, ny) = self.xy(next);
            if ((y > py) != (ny > py)) && ny != y && (px < (nx - x) * (py - y) / (ny - y) + x) {
                inside = !inside;
            }
            p = next;
            if p == a {
                break;
            }
        }
        inside
    }

    /// Link two vertices with a bridge: splits one ring in two, or merges a hole
    /// into the outer ring. Returns the duplicate of `b`.
    fn split_polygon(&mut self, a: usize, b: usize) -> usize {
        let (ai, ax, ay) = (self.nodes[a].i, self.nodes[a].x, self.nodes[a].y);
        let (bi, bx, by) = (self.nodes[b].i, self.nodes[b].x, self.nodes[b].y);
        let a2 = self.create_node(ai, ax, ay);
        let b2 = self.create_node(bi, bx, by);
        let an = self.next(a);
        let bp = self.prev(b);
        self.nodes[a].next = b;
        self.nodes[b].prev = a;
        self.nodes[a2].next = an;
        self.nodes[an].prev = a2;
        self.nodes[b2].next = a2;
        self.nodes[a2].prev = b2;
        self.nodes[bp].next = b2;
        self.nodes[b2].prev = bp;
        b2
    }
}

/// JavaScript truthiness of a number: neither zero nor NaN.
fn truthy(value: f64) -> bool {
    value != 0.0 && !value.is_nan()
}

fn sign(value: f64) -> i8 {
    if value > 0.0 {
        1
    } else if value < 0.0 {
        -1
    } else {
        0
    }
}

/// Z-order of a point, from coordinates scaled into a 15-bit integer range.
fn z_order(x: f64, y: f64, min_x: f64, min_y: f64, inv_size: f64) -> i32 {
    let mut x = super::math::to_int32((x - min_x) * inv_size);
    let mut y = super::math::to_int32((y - min_y) * inv_size);
    x = (x | (x << 8)) & 0x00FF_00FF;
    x = (x | (x << 4)) & 0x0F0F_0F0F;
    x = (x | (x << 2)) & 0x3333_3333;
    x = (x | (x << 1)) & 0x5555_5555;
    y = (y | (y << 8)) & 0x00FF_00FF;
    y = (y | (y << 4)) & 0x0F0F_0F0F;
    y = (y | (y << 2)) & 0x3333_3333;
    y = (y | (y << 1)) & 0x5555_5555;
    x | (y << 1)
}

#[allow(clippy::too_many_arguments)]
fn point_in_triangle(
    ax: f64,
    ay: f64,
    bx: f64,
    by: f64,
    cx: f64,
    cy: f64,
    px: f64,
    py: f64,
) -> bool {
    (cx - px) * (ay - py) >= (ax - px) * (cy - py)
        && (ax - px) * (by - py) >= (bx - px) * (ay - py)
        && (bx - px) * (cy - py) >= (cx - px) * (by - py)
}

#[allow(clippy::too_many_arguments)]
fn point_in_triangle_except_first(
    ax: f64,
    ay: f64,
    bx: f64,
    by: f64,
    cx: f64,
    cy: f64,
    px: f64,
    py: f64,
) -> bool {
    !(ax == px && ay == py) && point_in_triangle(ax, ay, bx, by, cx, cy, px, py)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_with_hole_covers_the_ring_area() {
        let mut contour = vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(4.0, 0.0),
            DVec2::new(4.0, 4.0),
            DVec2::new(0.0, 4.0),
        ];
        let mut holes = vec![vec![
            DVec2::new(1.0, 1.0),
            DVec2::new(1.0, 3.0),
            DVec2::new(3.0, 3.0),
            DVec2::new(3.0, 1.0),
        ]];
        let triangles = triangulate_shape(&mut contour, &mut holes);
        let points: Vec<DVec2> = contour.iter().chain(&holes[0]).copied().collect();
        let area: f64 = triangles
            .iter()
            .map(|t| shape_area(&[points[t[0]], points[t[1]], points[t[2]]]).abs())
            .sum();
        assert_eq!(triangles.len(), 8);
        assert!((area - 12.0).abs() < 1e-12);
    }
}
