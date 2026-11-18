// Temporary, read-only HEAD: remove when the reads land in a signed SDK.
fn frame_reads(game: &Game<'_>) {
    mod_minimap::frame_reads(&sunrise_items::blocks::MAP_COLORS, game.debug);
}
