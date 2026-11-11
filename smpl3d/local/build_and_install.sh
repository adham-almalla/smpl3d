#!/bin/zsh
# Builds Smpl3D and installs it to /Applications. Everything else stays in this folder.
set -e
cd "$(dirname "$0")"
source ./env.sh
ROOT="$PWD"
cd app
cargo build --release -p smpl3d -p smpl3d-cli 2>&1 | grep -E "^(error|warning: unused)|Finished" || true
test -x target/release/smpl3d
VERSION=$(cargo metadata --no-deps --format-version 1 | python3 -c "import sys,json; print([p['version'] for p in json.load(sys.stdin)['packages'] if p['name']=='smpl3d'][0])")
SHORT=${VERSION%%-*}
APP="$ROOT/app/target/Smpl3D.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources/Licenses"
cp target/release/smpl3d "$APP/Contents/MacOS/Smpl3D"
cp target/release/smpl3d-cli "$APP/Contents/MacOS/smpl3d-cli"
cp smpl3d/Smpl3D.icns "$APP/Contents/Resources/Smpl3D.icns"
cp LICENSE-APACHE LICENSE-MIT NOTICE THIRD-PARTY-LICENSES.txt "$APP/Contents/Resources/Licenses/"
sed -e "s/@VERSION@/$VERSION/g" -e "s/@SHORT_VERSION@/$SHORT/g" smpl3d/Info.plist.in > "$APP/Contents/Info.plist"
printf 'APPL????' > "$APP/Contents/PkgInfo"
codesign --force --deep --sign - "$APP" >/dev/null 2>&1 || true
osascript -e 'tell application "Smpl3D" to quit' >/dev/null 2>&1 || true
sleep 1
rm -rf /Applications/Smpl3D.app
cp -R "$APP" /Applications/
echo "Installed /Applications/Smpl3D.app ($VERSION)"
