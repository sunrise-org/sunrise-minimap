# Sunrise Minimap

A minimap for [Sunrise](https://github.com/sunrise-org/sunrise), as an official native mod.

Enable **Show minimap** in **Options → Mods → Sunrise Minimap → Config**. The map starts off. Choose its corner, size, shape and zoom; turn on **Rotate with player** for a forward-up view with a north marker. F1 hides it with the HUD. Only terrain the client has loaded is shown; underground the map shows the floor and cuts walls at eye height.

## Build

On Linux x86-64, run `bash tools/build-sdk.sh`. It verifies the pinned Sunrise SDK release, checks the Rust source, compiles the mod and its read-only frame patch against the prebuilt game, then produces an **unsigned** `target/sdk-<commit>/minimap.smod` and checksum. Building or checking the package does not need a game account or game resources. A CI dispatch can test a different signed SDK via `sdk_tag`; it does not change the checkout's pin.

## Release

Push a tag `v<version>` matching `version` in `sunrise-mod.toml`. The `Release` workflow builds the package against the pinned SDK as above, then publishes `minimap.smod` and `SHA256SUMS` as that tag's GitHub release. If the repository secret `MINISIGN_SECRET_KEY` holds a minisign secret key made without a password (`minisign -G -W`), it also signs the package and publishes `minimap.smod.minisig`; without it the release is unsigned. The workflow refuses a tag that isn't on `main` or that moves while it runs. A release that already exists is never replaced: if publishing fails, re-run the failed jobs; to change a published release, tag a new version.

The mod owns the tile cache and dynamic HUD texture. On the current SDK one non-cancelling frame patch reads the game's colour palette and whether the debug overlay is visible. Remove that patch once those general reads are available in a signed SDK. The mod does not include a copy of the game's palette or textures.

Licence: [PolyForm Shield 1.0.0](LICENSE). See [CONTRIBUTING.md](CONTRIBUTING.md) for the DCO sign-off.
