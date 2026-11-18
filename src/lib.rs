mod terrain;

use mod_api::{
    client::hud_layout::{GuiDraw, PoseOp},
    *,
};
use std::{
    cell::{Cell, RefCell},
    sync::OnceLock,
};
use terrain::{Reader, SIDE, Terrain, View};

const TEXTURE: &str = "minimap:dynamic/terrain";
fn texture() -> &'static Identifier {
    static ID: OnceLock<Identifier> = OnceLock::new();
    ID.get_or_init(|| Identifier::parse(TEXTURE).unwrap())
}

thread_local! {
    // The patch and callbacks run on the rendering thread of the same window.
    static FRAME: Cell<(&'static [u32], bool)> = const {Cell::new((&[], false))};
    static MAP: RefCell<Map> = RefCell::default();
    // The sprite and the flip never change; only translation and heading vary per frame.
    static ARROW: RefCell<[GuiDraw; 3]> = RefCell::new([
        GuiDraw::Push(vec![PoseOp::Translate(0., 0.), PoseOp::Rotate(0.), PoseOp::Scale(1., -1.)]),
        GuiDraw::AtlasSprite { sprite: "map/decorations/player".into(), x:-4, y:-4, w:8, h:8, color:u32::MAX },
        GuiDraw::Pop,
    ]);
}
/// Called only by our read-only frame HEAD. Remove when the reads land in a signed SDK.
#[doc(hidden)]
pub fn frame_reads(palette: &'static [u32], debug: bool) {
    FRAME.set((palette, debug));
}

#[derive(Default)]
struct Map {
    terrain: Terrain,
    level: Option<LevelRef>,
    // Height, ceiling, and fluid identities are registry data for this level.
    info: Option<(LevelDimensionType, [Option<FluidKey>; 4])>,
    texture: bool,
    pending: bool,
    look: Option<(View, Corner, f32)>,
}
impl Map {
    fn clear(&mut self, host: &mut dyn ClientHost) {
        if self.texture {
            let _ = host.free_dynamic_texture(texture());
        }
        *self = Self::default();
    }
    fn update(&mut self, host: &mut dyn ClientHost) {
        let player = host
            .setting_bool("enabled")
            .unwrap_or(false)
            .then(|| host.local_player())
            .flatten();
        let Some(player) = player.map(|p| p.entity()) else {
            if self.level.is_some() {
                self.clear(host);
            }
            return;
        };
        self.look = None;
        if host.hud_mode() != HudMode::AllVisible {
            return;
        }
        let (palette, debug) = FRAME.get();
        if palette.is_empty() {
            return;
        }
        let Some(level) = host.level(host.dimension()) else {
            return;
        };
        if self.level != Some(level) {
            self.clear(host);
            self.level = Some(level);
        }
        let Some([x, y, z]) = player.eye_position(host) else {
            return;
        };
        let Some([yaw, _]) = player.rotation(host) else {
            return;
        };
        if self.info.is_none() {
            self.info = level.dimension_type(host).map(|dim| {
                (
                    dim,
                    [
                        "minecraft:water",
                        "minecraft:flowing_water",
                        "minecraft:lava",
                        "minecraft:flowing_lava",
                    ]
                    .map(|name| host.fluid_key(name)),
                )
            });
        }
        let Some((dim, fluids)) = &self.info else {
            return;
        };
        let corner = Corner::ALL[host.setting_choice("corner").unwrap_or(1) as usize];
        if debug && corner.top() {
            return;
        }
        let size = [64., 96., 128.][host.setting_choice("size").unwrap_or(1) as usize];
        let scale = [0.5, 1., 2., 4.][host.setting_choice("zoom").unwrap_or(1) as usize];
        let view = View {
            x,
            z,
            yaw,
            rotate: host.setting_bool("rotate").unwrap_or(false),
            scale,
            round: host.setting_bool("round").unwrap_or(false),
        };
        let eye = [x.floor() as i32, y.floor() as i32, z.floor() as i32];
        let head = (dim.has_ceiling || level.brightness(host, LightLayer::Sky, eye) == Some(0))
            .then_some(eye[1]);
        let radius = ((64. * scale * std::f32::consts::SQRT_2 / 16.).ceil() as i32 + 1)
            .min(host.graphics_options().render_distance.max(2));
        self.terrain.update(
            &Reader {
                host,
                level,
                bottom: dim.min_y,
                fluids,
            },
            view,
            radius,
            head,
        );
        self.pending |= self.terrain.compose(view, palette);
        if self.pending {
            let side = SIDE as u32;
            let r = if self.texture {
                host.update_dynamic_texture(texture(), side, side, &self.terrain.image)
            } else {
                host.create_dynamic_texture(texture(), side, side, &self.terrain.image)
            };
            match r {
                Ok(()) => {
                    self.pending = false;
                    self.texture = true;
                }
                Err(DynamicTextureError::Unknown) => {
                    self.texture = false;
                    return;
                }
                Err(_) => return,
            }
        }
        self.look = Some((view, corner, size));
    }
}

#[derive(Default)]
struct Minimap;
#[mod_api::sunrise_mod]
impl Hooks for Minimap {
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        for setting in [
            SettingSpec::toggle("enabled", "minimap.enabled", false),
            SettingSpec::choice(
                "corner",
                "minimap.corner",
                [
                    "minimap.top_left",
                    "minimap.top_right",
                    "minimap.bottom_left",
                    "minimap.bottom_right",
                ],
                1,
            ),
            SettingSpec::choice(
                "size",
                "minimap.size",
                ["minimap.small", "minimap.medium", "minimap.large"],
                1,
            ),
            SettingSpec::toggle("round", "minimap.round", false),
            SettingSpec::choice(
                "zoom",
                "minimap.zoom",
                [
                    "minimap.zoom.half",
                    "minimap.zoom.one",
                    "minimap.zoom.two",
                    "minimap.zoom.four",
                ],
                1,
            ),
            SettingSpec::toggle("rotate", "minimap.rotate", false),
        ] {
            r.register_setting(setting)
                .expect("unique, valid minimap setting");
        }
    }
    fn client_block_changed(&self, _: &dyn ClientHost, event: &mut ClientBlockChangedEvent<'_>) {
        MAP.with_borrow_mut(|m| m.terrain.invalidate([event.pos.x >> 4, event.pos.z >> 4]));
    }
    fn client_chunk_load(&self, _: &mut dyn ClientHost, event: &mut ClientChunkLoadEvent<'_>) {
        MAP.with_borrow_mut(|m| m.terrain.loaded(event.chunk));
    }
    fn client_chunk_unload(&self, _: &mut dyn ClientHost, event: &mut ClientChunkUnloadEvent<'_>) {
        MAP.with_borrow_mut(|m| m.terrain.forget(event.chunk));
    }
    fn client_level_unload(&self, host: &mut dyn ClientHost, _: &mut ClientLevelUnloadEvent<'_>) {
        MAP.with_borrow_mut(|m| m.clear(host));
    }
    fn render_frame_pre(&self, host: &mut dyn ClientHost, _: &mut RenderFramePreEvent) {
        MAP.with_borrow_mut(|m| m.update(host));
    }
    fn render_gui_post(&self, _: &dyn ClientHost, event: &mut RenderGuiPostEvent<'_>) {
        let Some((view, corner, size)) = MAP.with_borrow(|m| m.look) else {
            return;
        };
        let (x, y) = event.corners.place(corner, size, size);
        let draw = &mut event.draw;
        draw.sprite(
            TEXTURE,
            [0., 0., SIDE as f32, SIDE as f32],
            [x, y, size, size],
            None,
        );
        let (cx, cy) = (x + size / 2., y + size / 2.);
        let angle = if view.rotate { 180. } else { 360. + view.yaw };
        ARROW.with_borrow_mut(|ops| {
            if let GuiDraw::Push(pose) = &mut ops[0] {
                pose[0] = PoseOp::Translate(cx, cy);
                pose[1] = PoseOp::Rotate(angle.to_radians());
            }
            let _ = draw.gui_paint(ops);
        });
        if view.rotate {
            let [nx, ny] = view.north();
            let reach = size / 2. - 6.;
            draw.text(
                "N",
                [
                    (cx + nx * reach - draw.text_width("N") / 2.).round(),
                    (cy + ny * reach - 4.).round(),
                ],
                u32::MAX,
                true,
            );
        }
    }
}
