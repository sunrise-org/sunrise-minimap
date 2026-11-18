use super::*;

#[test]
fn scan_skips_clear_blocks_and_distinguishes_a_floor_from_a_cut_wall() {
    let scan = |head| {
        scan_column(
            -10,
            8,
            head,
            |y| Some(if matches!(y, 1 | 6) { 2 } else { 0 }),
            |_| None,
            |_| true,
        )
    };
    assert_eq!(scan(None).height, 6);
    assert_eq!(scan(Some(5)).height, 1);
    assert!(!scan(Some(5)).wall);
    assert!(scan(Some(6)).wall);
    assert_eq!(scan(Some(-10)).color, 0);
}

#[test]
fn a_fluid_takes_the_surface_except_on_a_sturdy_top_and_saturates_at_depth() {
    let scan = |top, bottom, sturdy, fluid| {
        scan_column(
            bottom,
            top,
            None,
            |_| Some(7),
            |y| (y > 0).then_some(fluid),
            |_| sturdy,
        )
    };
    let c = scan(3, -64, false, WATER);
    assert_eq!((c.color, c.depth, c.height), (WATER, 3, 3));
    assert_eq!(scan(300, 0, true, WATER).depth, 255);
    assert_eq!(scan(3, -64, true, WATER).color, 7);
    assert_eq!(scan(2, -64, false, LAVA).color, LAVA);
}

#[test]
fn shades_use_height_water_depth_parity_and_the_cut_wall() {
    let mut c = Column {
        color: WATER,
        height: 7,
        depth: 4,
        wall: false,
    };
    assert_eq!((c.shade(7, 0), c.shade(7, 1)), (255, 220));
    c.depth = 10;
    assert_eq!(c.shade(7, 0), 180);
    c.color = 2;
    assert_eq!(
        [c.shade(5, 1), c.shade(7, 0), c.shade(9, 0)],
        [255, 220, 180]
    );
    c.wall = true;
    assert_eq!(c.shade(0, 0), 135);
}

#[test]
fn moving_turning_and_removing_tiles_invalidate_the_right_pixels() {
    let mut t = Terrain::default();
    let mut v = View {
        x: -1.,
        z: -1.,
        yaw: 0.,
        rotate: false,
        scale: 1.,
        round: false,
    };
    t.tiles.insert(
        [-1, -1],
        Tile {
            head: None,
            columns: Box::new(
                [Column {
                    color: 1,
                    height: 10,
                    ..Default::default()
                }; 256],
            ),
            scanned: Instant::now(),
        },
    );
    let palette = [0, 0xff_00_ff]; // Synthetic palette: no game RGB data in the fixture.
    assert!(t.compose(v, &palette));
    let center = (64 * SIDE + 64) * 4;
    assert_eq!(&t.image[center..center + 4], &[220, 0, 220, 255]);
    assert_eq!(&t.image[..4], &[10, 10, 10, 230]);
    v.yaw = 90.;
    v.x += 0.1;
    assert!(!t.compose(v, &palette));
    v.x += 0.2;
    assert!(t.compose(v, &palette));
    v.rotate = true;
    v.round = true;
    assert!(t.compose(v, &palette));
    assert_eq!(&t.image[..4], &[0; 4]);
    assert!((v.north()[0] - 1.).abs() < 1e-5);
    t.forget([-1, -1]);
    assert!(t.compose(v, &palette));
    assert_eq!(&t.image[center..center + 4], &[20, 20, 20, 200]);
}

#[test]
fn chunk_changes_coalesce_and_an_unload_discards_partial_work() {
    let mut t = Terrain {
        area: Some(Area {
            center: [0, 0],
            radius: 2,
            head: None,
        }),
        ..Default::default()
    };
    for k in [[2, 2], [0, 0], [-1, 1], [3, 0]] {
        t.loaded(k);
    }
    assert_eq!(
        t.queue.iter().map(|(_, k)| *k).collect::<Vec<_>>(),
        [[0, 0], [-1, 1], [2, 2]]
    );
    let scanned = Instant::now();
    t.tiles.insert(
        [0, 0],
        Tile {
            head: None,
            columns: Box::new([Column::default(); 256]),
            scanned,
        },
    );
    t.invalidate([0, 0]);
    t.invalidate([0, 0]);
    assert_eq!(t.delayed.first(), Some(&(scanned + RESCAN, [0, 0])));
    assert_eq!(t.delayed.len(), 1);
    // Neither an underground jump nor a resend may erase a published tile.
    t.queue.clear();
    let area = t.area.unwrap();
    t.set_area(Area {
        head: Some(11),
        ..area
    });
    assert!(t.tiles.contains_key(&[0, 0]));
    assert_eq!(t.queue.first(), Some(&(0, [0, 0])));
    assert!(!t.changed);
    t.queue.clear();
    t.partial = Some(([0, 0], 1, Box::new([Column::default(); 256])));
    t.loaded([0, 0]);
    assert!(t.partial.is_none());
    assert!(t.tiles.contains_key(&[0, 0]));
    assert_eq!(t.queue.first(), Some(&(0, [0, 0])));
    assert!(!t.changed);
    t.forget([0, 0]);
    assert!(t.partial.is_none());
    assert!(t.changed);
    assert!(!t.tiles.contains_key(&[0, 0]));
}
