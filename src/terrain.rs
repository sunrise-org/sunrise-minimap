use mod_api::{
    BlockFace, BlockState, ClientHost, FluidKey, Heightmap, HostBlockStates, LevelRef, SupportType,
};
use std::{
    collections::{BTreeSet, HashMap},
    time::{Duration, Instant},
};

#[cfg(test)]
mod tests;

pub const SIDE: usize = 128;
type Key = [i32; 2];
const BUDGET: Duration = Duration::from_micros(300);
const RESCAN: Duration = Duration::from_secs(1);

#[derive(Default, Clone, Copy, PartialEq, Eq)]
struct Column {
    color: u8,
    height: i32,
    depth: u8,
    wall: bool,
}
type Columns = [Column; 256];
struct Tile {
    head: Option<i32>,
    columns: Box<Columns>,
    scanned: Instant,
}

#[derive(Clone, Copy, PartialEq)]
pub struct View {
    pub x: f64,
    pub z: f64,
    pub yaw: f32,
    pub rotate: bool,
    pub scale: f32,
    pub round: bool,
}
impl View {
    fn axes(self) -> ([f64; 2], [f64; 2]) {
        if !self.rotate {
            return ([1., 0.], [0., -1.]);
        }
        let (s, c) = f64::from(self.yaw).to_radians().sin_cos();
        ([-c, -s], [-s, c])
    }
    pub fn north(self) -> [f32; 2] {
        let (right, forward) = self.axes();
        [-right[1] as f32, forward[1] as f32]
    }
    fn differs(self, prev: Self) -> bool {
        self.round != prev.round
            || self.scale != prev.scale
            || self.rotate != prev.rotate
            || (self.rotate && (self.yaw - prev.yaw).abs() >= 0.5)
            || (self.x - prev.x).abs().max((self.z - prev.z).abs()) >= f64::from(self.scale) / 4.
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Area {
    center: Key,
    radius: i32,
    head: Option<i32>,
}
impl Area {
    fn distance(self, k: Key) -> i32 {
        (self.center[0] - k[0])
            .abs()
            .max((self.center[1] - k[1]).abs())
    }
}

#[derive(Default)]
pub struct Terrain {
    tiles: HashMap<Key, Tile>,
    colors: HashMap<BlockState, u8>,
    area: Option<Area>,
    // Priority stays stable as a scan spans frames. Never publish half a tile.
    queue: BTreeSet<(i32, Key)>,
    delayed: BTreeSet<(Instant, Key)>,
    partial: Option<(Key, usize, Box<Columns>)>,
    changed: bool,
    composed: Option<View>,
    pub image: Vec<u8>,
}
impl Terrain {
    pub fn forget(&mut self, key: Key) {
        self.changed |= self.tiles.remove(&key).is_some();
        if self.partial.as_ref().is_some_and(|p| p.0 == key) {
            self.partial = None;
        }
        if let Some(a) = self.area {
            self.queue.remove(&(a.distance(key), key));
        }
    }
    pub fn loaded(&mut self, key: Key) {
        // A whole-column resend replaces the scan, not the visible tile.
        if self.partial.as_ref().is_some_and(|p| p.0 == key) {
            self.partial = None;
        }
        self.enqueue(key);
    }
    fn enqueue(&mut self, key: Key) {
        if let Some(a) = self.area
            && a.distance(key) <= a.radius
        {
            self.queue.insert((a.distance(key), key));
        }
    }
    pub fn invalidate(&mut self, key: Key) {
        // An in-flight tile may have read the old value already; scan it once more on completion.
        if let Some(tile) = self.tiles.get(&key) {
            self.delayed.insert((tile.scanned + RESCAN, key));
        } else {
            self.enqueue(key);
        }
    }
    fn set_area(&mut self, area: Area) {
        if self.area == Some(area) {
            return;
        }
        let Area {
            center: [cx, cz],
            radius,
            head,
        } = area;
        let before = self.tiles.len();
        self.tiles.retain(|&k, _| area.distance(k) <= radius);
        self.changed |= before != self.tiles.len();
        self.area = Some(area);
        // Re-rank invalidations as well as missing tiles as the player moves.
        for (_, key) in std::mem::take(&mut self.queue) {
            self.enqueue(key);
        }
        if let Some((key, _, _)) = self.partial.take() {
            self.enqueue(key);
        }
        for x in cx - radius..=cx + radius {
            for z in cz - radius..=cz + radius {
                let key = [x, z];
                if self.tiles.get(&key).is_none_or(|t| t.head != head) {
                    self.enqueue(key);
                }
            }
        }
    }
    pub fn update(&mut self, read: &Reader<'_>, view: View, radius: i32, head: Option<i32>) {
        let now = Instant::now();
        let center = [(view.x.floor() as i32) >> 4, (view.z.floor() as i32) >> 4];
        self.set_area(Area {
            center,
            radius,
            head,
        });
        while let Some(&(time, key)) = self.delayed.first()
            && time <= now
        {
            self.delayed.pop_first();
            self.enqueue(key);
        }
        loop {
            if self.partial.is_none() {
                let Some((_, key)) = self.queue.pop_first() else {
                    break;
                };
                if read
                    .level
                    .is_loaded(read.host, [key[0] * 16, read.bottom, key[1] * 16])
                {
                    self.partial = Some((key, 0, Box::new([Column::default(); 256])));
                }
            }
            if let Some((key, row, tile)) = &mut self.partial {
                let xs = key[0] * 16;
                let z = key[1] * 16 + *row as i32;
                let positions: [_; 16] = std::array::from_fn(|i| (xs + i as i32, z));
                let Ok(tops) = read
                    .host
                    .client_surface_heights(&positions, Heightmap::WorldSurface)
                else {
                    self.partial = None;
                    break;
                };
                for (i, top) in tops.into_iter().enumerate() {
                    if let Some(y) = top {
                        tile[*row * 16 + i] =
                            read.column(&mut self.colors, positions[i], y - 1, head);
                    }
                }
                *row += 1;
                if *row == 16 {
                    let (k, _, columns) = self.partial.take().unwrap();
                    self.changed |= self
                        .tiles
                        .get(&k)
                        .is_none_or(|prev| prev.columns != columns);
                    self.tiles.insert(
                        k,
                        Tile {
                            head,
                            columns,
                            scanned: Instant::now(),
                        },
                    );
                }
            }
            if now.elapsed() >= BUDGET {
                break;
            }
        }
    }
    fn column(&self, x: i32, z: i32) -> Option<Column> {
        self.tiles
            .get(&[x >> 4, z >> 4])
            .map(|t| t.columns[((z & 15) * 16 + (x & 15)) as usize])
            .filter(|c| c.color != 0)
    }
    pub fn compose(&mut self, view: View, palette: &[u32]) -> bool {
        if !self.changed && self.composed.is_some_and(|old| !view.differs(old)) {
            return false;
        }
        let (r, f) = view.axes();
        self.image.resize(SIDE * SIDE * 4, 0);
        for j in 0..SIDE {
            for i in 0..SIDE {
                let (u, v) = (i as f64 + 0.5 - 64., j as f64 + 0.5 - 64.);
                let d2 = u * u + v * v;
                let rim = if view.round {
                    d2 > 62.5 * 62.5
                } else {
                    i == 0 || i == SIDE - 1 || j == 0 || j == SIDE - 1
                };
                let rgba = if view.round && d2 > 64. * 64. {
                    [0; 4]
                } else if rim {
                    [10, 10, 10, 230]
                } else {
                    let x = (view.x + f64::from(view.scale) * (u * r[0] - v * f[0])).floor() as i32;
                    let z = (view.z + f64::from(view.scale) * (u * r[1] - v * f[1])).floor() as i32;
                    self.column(x, z).map_or([20, 20, 20, 200], |c| {
                        let shade = c.shade(
                            self.column(x, z - 1).map_or(c.height, |n| n.height),
                            (x ^ z) & 1,
                        );
                        palette.get(usize::from(c.color)).map_or([0; 4], |rgb| {
                            let ch = |s: u32| (((rgb >> s) & 255) * shade / 255) as u8;
                            [ch(16), ch(8), ch(0), 255]
                        })
                    })
                };
                let o = (j * SIDE + i) * 4;
                self.image[o..o + 4].copy_from_slice(&rgba);
            }
        }
        self.changed = false;
        self.composed = Some(view);
        true
    }
}

pub struct Reader<'a> {
    pub host: &'a dyn ClientHost,
    pub level: LevelRef,
    pub bottom: i32,
    pub fluids: &'a [Option<FluidKey>; 4],
}
const WATER: u8 = 12;
const LAVA: u8 = 4;
impl Reader<'_> {
    fn fluid(&self, pos: [i32; 3]) -> Option<u8> {
        let f = self.level.fluid_state(self.host, pos)?.fluid;
        self.fluids
            .iter()
            .position(|&key| key == Some(f))
            .map(|i| if i < 2 { WATER } else { LAVA })
    }
    fn column(
        &self,
        colors: &mut HashMap<BlockState, u8>,
        (x, z): (i32, i32),
        surface: i32,
        head: Option<i32>,
    ) -> Column {
        scan_column(
            self.bottom,
            surface,
            head,
            |y| {
                let state = self.level.block_state(self.host, [x, y, z])?;
                Some(
                    *colors
                        .entry(state)
                        .or_insert_with(|| self.host.block_states().map_color(state).unwrap_or(0)),
                )
            },
            |y| self.fluid([x, y, z]),
            |y| {
                self.level
                    .block_face_sturdy(self.host, [x, y, z], BlockFace::Up, SupportType::Full)
                    == Some(true)
            },
        )
    }
}

fn scan_column(
    bottom: i32,
    surface: i32,
    head: Option<i32>,
    mut color_at: impl FnMut(i32) -> Option<u8>,
    fluid_at: impl Fn(i32) -> Option<u8>,
    sturdy_top: impl Fn(i32) -> bool,
) -> Column {
    let mut y = head.map_or(surface, |h| h.min(surface));
    while y > bottom {
        let Some(mut color) = color_at(y) else {
            break;
        };
        if color == 0 {
            y -= 1;
            continue;
        }
        let fluid = fluid_at(y);
        let wall = head == Some(y) && fluid.is_none();
        let mut depth = 0u8;
        if let Some(fluid) = fluid {
            depth = 1;
            while depth < u8::MAX
                && y - i32::from(depth) > bottom
                && fluid_at(y - i32::from(depth)).is_some()
            {
                depth += 1;
            }
            if !sturdy_top(y) {
                color = fluid;
            }
        }
        return Column {
            color,
            height: y,
            wall,
            depth,
        };
    }
    Column::default()
}
impl Column {
    fn shade(self, north: i32, parity: i32) -> u32 {
        if self.wall {
            return 135;
        }
        if self.color == WATER {
            let d = f64::from(self.depth) * 0.1 + f64::from(parity) * 0.2;
            if d < 0.5 {
                255
            } else if d > 0.9 {
                180
            } else {
                220
            }
        } else {
            let d = f64::from(self.height - north) * 0.8 + (f64::from(parity) - 0.5) * 0.4;
            if d > 0.6 {
                255
            } else if d < -0.6 {
                180
            } else {
                220
            }
        }
    }
}
