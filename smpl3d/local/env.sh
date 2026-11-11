# Source this to use Smpl3D's bundled Rust toolchain: `source env.sh`
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
export RUSTUP_HOME="$ROOT/tools/rustup"
export CARGO_HOME="$ROOT/tools/cargo"
export PATH="$CARGO_HOME/bin:$PATH"
export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
