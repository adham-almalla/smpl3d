# Troubleshooting: Smpl3D won't start

## 0.10.0 closes a few seconds after it starts (Windows)

Smpl3D 0.10.0 crashes at startup on Windows PCs that draw with Direct3D 12 and the system
shader compiler. That covers older Intel graphics, PCs without a graphics driver (the
"Microsoft Basic Display Adapter"), virtual machines and Remote Desktop sessions. **Install
0.10.1 or later**, which fixes it. Versions before 0.10.0 don't have this problem.

## Start in safe mode

Safe mode starts with default settings (your own settings stay as they are, for the next normal
start), skips the "Recover unsaved design?" offer and draws with the software renderer.

- Windows: run `smpl3d.exe --safe-mode` from a terminal in the install folder, or add
  ` --safe-mode` to the end of a shortcut's *Target*.
- macOS: `open -a Smpl3D --args --safe-mode`
- Linux: `smpl3d --safe-mode` (Flatpak: `flatpak run io.tangencylabs.smpl3d --safe-mode`)

Setting `SMPL3D_SAFE_MODE=1` does the same. From 0.10.2, if two launches in a row stop
before the window opens, the next one starts in safe mode by itself. `SMPL3D_RENDERER=cpu`
uses only the software renderer and keeps your settings.

## A design won't open or hangs while opening

- **Open the previous save.** Every save keeps the version it replaced next to it as
  `<file>.bak` (see [Files](files.md)). Copy it to a new name ending in `.sc`, for
  example `bracket-previous.sc`, and open that. Work saved after the previous save is
  not in it.
- **Check the crash reports.** If Smpl3D crashed while the design was open, the newest report
  in the `crashes` folder (see the table below) usually holds a `design.sc` with the
  design as it was. `smpl3d-cli crashes` lists the reports.
- **Unsaved designs** are in the `recovery` folder, not next to the file.
- **Report it.** GitHub doesn't accept `.sc` (or `.smpl3d`) attachments, so zip the file
  (`design.sc.zip`) and attach it to an issue, with the crash report if there is one.

## Reset the settings

Smpl3D keeps its settings and its data in the folders below. All installed versions share
them. No version changes anything outside them: no system settings, drivers or registry
entries. Quit Smpl3D, then **rename** (don't delete) the folders. Smpl3D starts with
defaults and makes new ones. Rename them back to restore your settings, or send them to us
with a bug report.

| System | Settings (`app.ron`, `graphics.json`) | Data (`recovery`, `crashes`, `control`, thumbnails) |
|---|---|---|
| Windows | `%APPDATA%\Smpl3D` | `%LOCALAPPDATA%\Smpl3D` |
| macOS | `~/Library/Application Support/Smpl3D` | (the same folder) |
| Linux | `~/.local/share/smpl3d` | (the same folder); thumbnails in `$XDG_CACHE_HOME/Smpl3D` or `/tmp/Smpl3D` |
| Linux (Flatpak) | `~/.var/app/io.tangencylabs.smpl3d/data/smpl3d` | (the same folder) |

On Windows, paste `%APPDATA%` or `%LOCALAPPDATA%` into the Explorer address bar to open the
folder. Releases before the rename used `SolveCraft` folders next to these; Smpl3D only
reads them, so you can leave them alone.

- The **`recovery`** folder holds autosaves of designs that were never saved. Rename the data
  folder rather than deleting it, so you can still open those autosaves (`.sc` files, or `.smpl3d` from earlier releases)
  with File > Open.
- The **`crashes`** folder holds crash reports. Please attach the newest one to a bug report.

From 0.10.2, Smpl3D also checks the settings files at start. A file it can't read is renamed
to `app.ron.bad-<time>` or `graphics.json.bad-<time>`, and Smpl3D starts with defaults
instead.

## "No graphics adapter" or nothing happens at all

Smpl3D needs Direct3D 12 or Vulkan on Windows, Metal on macOS, and Vulkan or OpenGL on
Linux. Without a graphics driver it falls back to the system's software renderer (WARP on
Windows, llvmpipe or lavapipe on Linux), which is slow but works. From 0.10.2, if no window can
be opened at all, Smpl3D tries the software renderer once and otherwise shows a message
saying what failed (earlier versions closed without a word).

On Windows, if the graphics API cannot create the window's renderer, Smpl3D tries the
remaining APIs in order: Direct3D 12 → Vulkan → OpenGL, then software rendering. The status
message and **Preferences › Graphics** say which API failed and why. The saved graphics
preference stays unchanged. With an explicit `WGPU_BACKEND`, a failed start shows the error
and exits without switching; change or remove that variable to try another API.

Things to try:

- Install the graphics driver from the GPU maker (Intel, AMD, NVIDIA) rather than Windows Update.
- In a virtual machine, turn on 3D acceleration. Over Remote Desktop, try it on the machine itself.
- Run `smpl3d --crash-log` to list recent crash reports, and attach the newest one to an
  issue.
- On an Intel Mac (Intel Iris graphics, macOS 12), "Parent device is lost" at start comes from
  the Metal compiler refusing wgpu's indirect-call validation shader. Smpl3D leaves that
  validation off on Intel Macs. To change it, set `WGPU_VALIDATION_INDIRECT_CALL=1` (on) or `0`
  (off) before starting; a value you set is always kept.

## The desktop stutters or freezes while Smpl3D builds bodies

Some graphics drivers hang on the shader Smpl3D uses to build bodies on the GPU, and the
system then resets the GPU. Smpl3D already builds bodies on the CPU on the AMD FirePro
D300/D500/D700 (Mac Pro 2013). On another GPU that does this, start Smpl3D with
`SMPL3D_CPU_TESS=1` set to build bodies on the CPU, and open an issue with the GPU's name.

## The interface looks blurry on Linux with display scaling

On Linux, Smpl3D runs through XWayland whenever an X display is available, because file
drag and drop from a file manager only works there (niri is the exception and stays on native
Wayland). A compositor that doesn't let X11 apps scale themselves draws an XWayland window at
100 % and stretches it to the monitor's scale, so text and icons look soft. That happens with
fractional scaling (125 %, 140 %), and on GNOME 47 (RHEL and Rocky Linux 10) at any scale,
200 % included. The diagnostics in a bug report show it as about 1 physical pixel per point at
a scale of 200 %.

Run Smpl3D on native Wayland instead: Preferences › Graphics › **Window system** ›
Wayland, which applies at the next start. Versions without that setting (0.21.0 and earlier)
take `SMPL3D_WAYLAND=1` in the environment:

```
SMPL3D_WAYLAND=1 smpl3d
```

On native Wayland, dropping files onto the window doesn't work; use File > Open or pass the file
on the command line.
[Wayland, XWayland and niri](https://github.com/tangency-labs/smpl3d/blob/main/docs/nix.md#wayland-xwayland-and-niri)
has the details.

## Trackpad pinch does nothing on Linux

A Wayland compositor reports touchpad gestures only to native Wayland windows. Through
XWayland there is no gesture protocol, so a pinch never reaches Smpl3D at all (two-finger
scrolling still works there, because X11 reports it as a wheel). Run Smpl3D on native
Wayland (see above) and pinch zooms the view at the cursor, as it already did on macOS and
the browser.

## Smpl3D closes right away on Windows

A graphics driver can crash the app while it starts, before Smpl3D can show anything (an
old Intel Vulkan driver, `igvk64.dll`, did). Windows' Event Viewer names the driver under
*Windows Logs › Application* ("Faulting module name").

From 0.11.1, Smpl3D on Windows starts Direct3D 12 first and loads no other graphics API
when Direct3D 12 has a graphics card; Vulkan and then OpenGL are tried only on a computer
without one. Earlier versions started Vulkan, Direct3D 12 and OpenGL together and preferred
Vulkan. To choose the graphics API yourself, in any version, open a Command Prompt and start
Smpl3D from it:

```
set WGPU_BACKEND=dx12
"C:\Program Files\Smpl3D\smpl3d.exe"
```

`vulkan` and `gl` are the other choices. `setx WGPU_BACKEND dx12` keeps it for apps started
from the Start menu after you sign in again. From 0.11.1 the setting can also live in
`graphics.json` in the settings folder, as `{"msaa": 4, "backend": "dx12"}` (`auto` is the
order above). Safe mode ignores it and always uses the software renderer.

With OpenGL on Windows, the window has the Windows title bar above Smpl3D's application bar
instead of using the application bar as its title bar. Some OpenGL drivers (older Intel HD and
UHD graphics) otherwise show the picture a title bar's height lower than the window, so clicks
land above what they point at.

## Smpl3D restarted, saying the graphics device was reset

When the graphics driver stops responding, the system resets it (on Windows, "Display driver
stopped responding and has recovered"), and the graphics device Smpl3D draws with is gone
for good: the window can't draw anything more. Smpl3D then closes, keeps your unsaved
changes for recovery as it does after a crash, and starts again; the new window says what
happened, with what the graphics driver reported, and offers the changes back. The old window
closes on its own; Smpl3D doesn't wait for the driver to release it. If the device is reset
again within ten minutes of that restart, the next start uses software rendering, which doesn't
depend on the graphics driver. Updating the graphics driver from the GPU maker usually stops the
resets. Laptops that lose the device on waking from sleep (often with an NVIDIA GPU that doesn't
drive the screen) can keep Smpl3D on the integrated GPU instead: Preferences › Graphics.

## A design saved by 0.10 won't open in an older version

Designs saved by a newer Smpl3D use a newer file format. Older versions refuse them with
"this design was saved by a newer Smpl3D" and don't change the file. Open them with the
current version.

## Save As says the file can't be found (Windows)

If saving into *Documents* (or *Pictures*, *Desktop* and other protected folders) fails with a
"File not found" message from the Windows save dialog and nothing is saved, Windows Security's
*Controlled folder access* is probably blocking Smpl3D. Windows usually also shows an
"Unauthorized changes blocked" notification. Either save to a folder that isn't protected, or
allow Smpl3D: *Windows Security › Virus & threat protection › Ransomware protection ›
Controlled folder access › Allow an app through Controlled folder access*, then add
`smpl3d.exe`.
