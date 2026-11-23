# Sunrise Minimap

A minimap for [Sunrise](https://github.com/sunrise-org/sunrise), as an official native mod.

Enable **Show minimap** in **Options → Mods → Sunrise Minimap → Config**. The map starts off. Choose its corner, size, shape and zoom; turn on **Rotate with player** for a forward-up view with a north marker. F1 hides it with the HUD. Only terrain the client has loaded is shown; underground the map shows the floor and cuts walls at eye height.

## Build

On Linux x86-64, run `bash tools/build-sdk.sh`. It verifies the pinned Sunrise SDK release, checks the Rust source, compiles the mod against the prebuilt game, then produces an **unsigned** `target/sdk-<commit>/minimap.smod` and checksum. Building or checking the package does not need a game account or game resources. A CI dispatch can test a different signed SDK via `sdk_tag`; it does not change the checkout's pin.

The mod owns the tile cache and dynamic HUD texture. It reads map colours and whether the debug overlay is visible through the mod API. The mod does not include a copy of the game's palette or textures.

## Release

After the mod is [registered](https://github.com/sunrise-org/sunrise-mods#the-new-registry-being-built), run `cargo sunrise-mod release <version>` from a clean checkout of `main` using the current mod tool. For the first registry release, choose a new version; `v0.1.0` was published before the shared workflow.

The shared workflow publishes a source `.smod` and a separate `.page.tar` built from [`page.toml`](page.toml) and its images. Both have GitHub artifact attestations. The registry checks those attestations against this repository and the shared workflow before review. The launcher builds the native mod against the player's game build from source and the lockfile.

Licence: [MIT](LICENSE). See [CONTRIBUTING.md](CONTRIBUTING.md) for the DCO sign-off.
