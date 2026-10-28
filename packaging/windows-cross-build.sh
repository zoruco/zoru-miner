#!/usr/bin/env bash
# Build Windows x64 CLI, GUI and installer on a Linux host. No CI is required.
set -euo pipefail
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
qt="${QT_WIN_ROOT:?Set QT_WIN_ROOT to the Qt 6.2.4 Windows MinGW installation}"
moc="${QT_HOST_MOC:-/usr/lib/qt6/libexec/moc}"
compiler="${MINGW_CXX:-x86_64-w64-mingw32-g++-posix}"
linker="${MINGW_CC:-x86_64-w64-mingw32-gcc-posix}"
target_dir="${ZORU_WIN_TARGET_DIR:-$repo/miner/target}"
output="${ZORU_WIN_OUTPUT_DIR:-$repo/dist/zoru-miner-v0.1.0-windows-x64}"
staging="$output"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

test -d "$qt/include/QtCore"
test -f "$qt/bin/Qt6Core.dll"
test -f "$qt/plugins/platforms/qwindows.dll"
test -x "$moc"
command -v "$compiler" >/dev/null
command -v "$linker" >/dev/null
command -v makensis >/dev/null
command -v zip >/dev/null
test ! -e "$output" || { echo "Release output already exists: $output" >&2; exit 1; }
test ! -e "$output.zip" || { echo "Release ZIP already exists" >&2; exit 1; }
test ! -e "$output-setup.exe" || { echo "Release installer already exists" >&2; exit 1; }

export PATH="${HOME}/.cargo/bin:${PATH}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER="$linker"
export CC_x86_64_pc_windows_gnu="$linker"
export CXX_x86_64_pc_windows_gnu="$compiler"
export AR_x86_64_pc_windows_gnu="${MINGW_AR:-x86_64-w64-mingw32-ar}"
export CARGO_TARGET_DIR="$target_dir"

cargo build --manifest-path "$repo/miner/Cargo.toml" --release --locked --target x86_64-pc-windows-gnu
"$moc" "$repo/app/src/Window.h" -o "$work/moc_Window.cpp"
"$compiler" -std=gnu++17 -O2 -mwindows \
  -DQT_WIDGETS_LIB -DQT_GUI_LIB -DQT_CORE_LIB -DQT_NO_DEBUG -DWIN32 -D_WINDOWS \
  -I"$qt/include" -I"$qt/include/QtCore" -I"$qt/include/QtGui" -I"$qt/include/QtWidgets" -I"$repo/app/src" \
  "$repo/app/src/Window.cpp" "$repo/app/src/main.cpp" "$work/moc_Window.cpp" \
  -L"$qt/lib" -lQt6Widgets -lQt6Gui -lQt6Core -lQt6EntryPoint \
  -lole32 -luser32 -lgdi32 -lshell32 -lws2_32 -luuid -loleaut32 -lcomdlg32 -lcomctl32 -ladvapi32 \
  -o "$work/zoru-miner-gui.exe"

mkdir -p "$staging/bin/platforms"
cp "$target_dir/x86_64-pc-windows-gnu/release/zoru-miner-cli.exe" "$staging/bin/"
cp "$work/zoru-miner-gui.exe" "$staging/bin/"
for name in Qt6Core.dll Qt6Gui.dll Qt6Widgets.dll libgcc_s_seh-1.dll libstdc++-6.dll libwinpthread-1.dll; do
  cp "$qt/bin/$name" "$staging/bin/$name"
done
cp "$qt/plugins/platforms/qwindows.dll" "$staging/bin/platforms/"
printf '[Paths]\nPlugins=.\n' > "$staging/bin/qt.conf"
for name in LICENSE README.md SECURITY.md CONTRIBUTING.md; do
  cp "$repo/$name" "$staging/"
done
cp -a "$repo/licenses" "$repo/docs" "$staging/"
cp "${ZORU_MAINNET_MANIFEST:?Set ZORU_MAINNET_MANIFEST to a confirmed on-chain manifest}" "$staging/deployment.json"

python3 - "$staging" <<'PY'
import hashlib,json,sys
from pathlib import Path
root=Path(sys.argv[1]); m=json.loads((root/'deployment.json').read_text())
value=m.pop('manifestSha256',None)
expected=hashlib.sha256(json.dumps(m,sort_keys=True,separators=(',',':')).encode()).hexdigest()
assert value==expected, 'Manifest checksum mismatch'
assert m['chainId']==6736 and m['schema']=='zoru-mainnet-suite-1' and m['mode']=='mainnet'
assert m['token'].lower()=='0xb972753d17c5131d127b6c22dde79b91a3132d03'
assert m['ledger'].lower()=='0xff53a9079c2853c39c5adbfca788cb0f12acff03'
files={str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest()
       for p in sorted(root.rglob('*')) if p.is_file()}
(root/'checksums.json').write_text(json.dumps(files,sort_keys=True,indent=2)+'\n')
print('Package checksum map:',len(files),'files')
PY

(cd "$(dirname "$output")" && zip -q -X -r "$(basename "$output").zip" "$(basename "$staging")")
makensis -V2 "-DSTAGING=$staging" "-DOUTPUT=$output-setup.exe" "$repo/packaging/windows-installer.nsi"
test -s "$output.zip" && test -s "$output-setup.exe"
for f in "$output.zip" "$output-setup.exe"; do
  sha256sum "$f"
done
echo "WIN64_RELEASE_ASSETS_READY"
