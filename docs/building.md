# Building and running

```sh
cargo run --release -p smpl3d -- --sample          # desktop app with a sample part
cargo run --release -p smpl3d-cli -- commands      # the command registry
cargo run --release -p smpl3d-cli -- run examples/bracket.json --out bracket.step
cargo run --release -p smpl3d-cli -- snapshot examples/bracket.json --out bracket.png
cargo run --release -p smpl3d-cli -- snapshot part.sc --out preview.png --tool Pin --section z=12
cargo run --release -p smpl3d-cli -- render examples/render-studio.json --out product.png
cargo run --release -p smpl3d -- part.step          # open a STEP file as a new design
cargo run --release -p smpl3d-cli -- eval part.step # measure an imported STEP file
cargo xtask ci                                          # fmt, clippy, tests, icons, assets, layers, wasm
```

UI artwork changes follow the [icon specification](../assets/icons/ui/STYLE_GUIDE.md).
The [included review tool](../assets/icons/ui/README.md) discovers new icons and keeps
unaccepted artwork Open; run `cargo xtask icons` to check the collection.

Windows builds: `cargo xwin build --release --target x86_64-pc-windows-msvc -p smpl3d`.


In the browser: `cd apps/smpl3d-web && trunk build --release --public-url ./` (WebGPU, or
WebGL2 with `?webgl`; `?sample` opens the sample part).

Agents can control the document open in a browser tab through `window.smpl3d.execute(...)`
and `window.smpl3d.request(...)`; see the [browser command API](browser-api.md).

See [CONTRIBUTING](../CONTRIBUTING.md) for the test and review workflow.
