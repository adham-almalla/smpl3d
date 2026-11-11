Smpl3D is open-source parametric 3D CAD in Rust: sketch, constrain, extrude, and change a
parameter to watch the timeline rebuild. [Documentation](https://docs.smpl3d.io/)
· [try it in the browser](https://smpl3d.io/app) ·
[contributing](https://github.com/bherbruck/smpl3d/blob/main/CONTRIBUTING.md) ·
[issues](https://github.com/bherbruck/smpl3d/issues). AI agents can model with it over MCP:
`smpl3d-cli mcp` ([how](https://docs.smpl3d.io/guide/mcp.html)).

**What's new** in Smpl3D {version} is listed below, and the
[ROADMAP](https://github.com/bherbruck/smpl3d/blob/main/ROADMAP.md#known-limitations) lists
the known limitations. Designs from every earlier version open in this one.

**New app ID.** The app ID is now `io.tangencylabs.smpl3d` (it was `ai.storyteller.smpl3d`,
and `ai.storyteller.solvecraft` up to 0.7): `flatpak update` offers to replace the old app, the
`.deb`, `.rpm` and AppImage remove the old menu entry, and settings stay. On macOS the bundle ID
changed too, so macOS asks once more for the permissions it had granted.

**Upgrading from 0.5 or earlier.** 0.6 renamed SolveCraft to Smpl3D and made the zip
container the default save format. `.solvecraft` designs, old settings folders and
`SOLVECRAFT_*` variables are still picked up, and `solvecraft-cli` still forwards to
`smpl3d-cli`. Want plain JSON? Use **File ▸ Save As JSON…** or `--save design.json`. On
macOS, replace `SolveCraft.app` with `Smpl3D.app`. Details:
[compatibility notes](https://github.com/bherbruck/smpl3d/blob/main/docs/compatibility.md).

Documentation, the browser app and the Flatpak feed use the Smpl3D project Pages URLs.

**Which file?**
- **Windows:** `Smpl3D-{version}-windows-x64.exe` runs as is. Or install the `.msi`, or unzip the
  `-portable.zip` (x64, x86 and arm64).
- **macOS 11+** (Apple silicon and Intel): open the `.dmg` and drag `Smpl3D.app` onto Applications.
  The CLI is in `smpl3d-cli-{version}-macos-universal.zip`.
- **Linux** (x86_64 and aarch64, glibc 2.35+): the `.AppImage` (`chmod +x`, then run it), the
  `.deb` or `.rpm`, the `.flatpak` (`flatpak install --user <file>`), the `.snap`
  (`sudo snap install --dangerous <file>`, or `sudo snap install smpl3d` from the Snap Store), or the `.tar.gz`.
- **FreeBSD:** `tar -xzf smpl3d-{version}-freebsd-x86_64.tar.gz --strip-components 1 -C /usr/local`.
- **Web:** `smpl3d-web-{version}.zip` is a static site to host yourself.
- `SHA256SUMS.txt`: `sha256sum -c SHA256SUMS.txt --ignore-missing`.
- Files without a version in the name (`smpl3d-windows-x64.msi`, `smpl3d-linux-x86_64.AppImage`, …)
  are the same packages under fixed names, for download links that always fetch the latest release.

For Flatpak feed installation and updates, add Flathub for the required runtime before
installing Smpl3D; keep both remotes in the same user installation:

```sh
flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak remote-add --user --if-not-exists smpl3d https://docs.smpl3d.io/flatpak/smpl3d.flatpakrepo
flatpak install --user smpl3d io.tangencylabs.smpl3d
flatpak run io.tangencylabs.smpl3d
```

A remote added from the old `bherbruck.github.io` address no longer finds updates: run
`flatpak remote-modify --user --url=https://docs.smpl3d.io/flatpak/repo/ smpl3d`, then
`flatpak update --user`.

Flatpak installs under an old ID (`ai.storyteller.smpl3d`, or `ai.storyteller.solvecraft` from 0.7 or
earlier): `flatpak update --user` offers to replace it with `io.tangencylabs.smpl3d`, and settings move along.

Single-file `.flatpak` bundles include the runtime repository address; missing runtimes
still need an internet connection. [Linux installation details](https://docs.smpl3d.io/guide/getting-started.html#linux-flatpak).

The builds aren't signed yet. On macOS, right-click `Smpl3D.app` in Applications and choose
**Open** the first time (or run `xattr -dr com.apple.quarantine /Applications/Smpl3D.app`).
On Windows, SmartScreen may say "Windows protected your PC": **More info**, then **Run anyway**.
Licences are built in (Help ▸ About ▸ Licences, `smpl3d-cli licences`); the packages also
install `THIRD-PARTY-LICENSES.txt`.

