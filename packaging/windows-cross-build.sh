#!/usr/bin/env bash
# Build the Windows desktop, command line tools, portable package and installer.
set -euo pipefail
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
qt="${QT_WIN_ROOT:?Set QT_WIN_ROOT to the Qt 6.2.4 Windows MinGW installation}"
moc="${QT_HOST_MOC:-/usr/lib/qt6/libexec/moc}"
rcc="${QT_HOST_RCC:-/usr/lib/qt6/libexec/rcc}"
compiler="${MINGW_CXX:-x86_64-w64-mingw32-g++-posix}"
linker="${MINGW_CC:-x86_64-w64-mingw32-gcc-posix}"
target_dir="${ZORU_WIN_TARGET_DIR:-$repo/miner/target}"
version="$(python3 -c 'import pathlib,re,sys; print(re.search(r"^version = \"([^\"]+)\"",pathlib.Path(sys.argv[1]).read_text(),re.M).group(1))' "$repo/miner/Cargo.toml")"
output="${ZORU_WIN_OUTPUT_DIR:-$repo/dist/ZORU-Miner-$version-Windows-x64}"
manifest="${ZORU_MAINNET_MANIFEST:?Set ZORU_MAINNET_MANIFEST to the reviewed mainnet manifest}"
work="$repo/build-windows-release"
mkdir -p "$work/generated"
for program in "$moc" "$rcc" "$compiler" "$linker" makensis zip; do command -v "$program" >/dev/null; done
test -f "$qt/bin/Qt6Quick.dll"
test ! -e "$output" || { echo "Choose a fresh release directory" >&2; exit 1; }
export PATH="${HOME}/.cargo/bin:${PATH}"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER="$linker"
export CC_x86_64_pc_windows_gnu="$linker"
export CXX_x86_64_pc_windows_gnu="$compiler"
export AR_x86_64_pc_windows_gnu="${MINGW_AR:-x86_64-w64-mingw32-ar}"
export CARGO_TARGET_DIR="$target_dir"
cargo build --manifest-path "$repo/miner/Cargo.toml" --release --locked --target x86_64-pc-windows-gnu --bins
python3 - "$repo" "$work" "$version" <<'PY'
from pathlib import Path
from xml.etree import ElementTree as E
import sys
root,work=map(Path,sys.argv[1:3]);version=sys.argv[3]
(work/'generated/AppVersion.h').write_text('#pragma once\n#define ZORU_APP_VERSION "'+version+'"\n')
resources=E.Element('RCC')
qml=E.SubElement(resources,'qresource',prefix='/Zoru')
for path in sorted((root/'app/qml').rglob('*')):
 if path.suffix in ('.qml','.js'):E.SubElement(qml,'file',alias=str(path.relative_to(root))).text=str(path)
brand=E.SubElement(resources,'qresource',prefix='/')
for name,alias in [('zoru-panda.png','app.png'),('zoru-token.png','token.png'),('vex-native.png','vex.png')]:
 E.SubElement(brand,'file',alias='branding/'+alias).text=str(root/'assets'/name)
docs=E.SubElement(resources,'qresource',prefix='/docs')
for name in ['APP_GUIDE.md','ABOUT_ZORU.md','UPDATES.md']:E.SubElement(docs,'file',alias=name).text=str(root/'docs'/name)
E.ElementTree(resources).write(work/'resources.qrc',encoding='utf-8')
PY
for header in AppState WalletService UpdateService; do "$moc" "$repo/app/src/$header.h" -I"$work/generated" -o "$work/moc_$header.cpp"; done
"$rcc" "$work/resources.qrc" -o "$work/resources.cpp"
includes=(-I"$qt/include" -I"$repo/app/src" -I"$work/generated")
for module in Core Gui Widgets Network Qml QmlModels Quick QuickControls2; do includes+=(-I"$qt/include/Qt$module"); done
sources=("$repo/app/src/AppState.cpp" "$repo/app/src/WalletService.cpp" "$repo/app/src/UpdateService.cpp" "$work/moc_AppState.cpp" "$work/moc_WalletService.cpp" "$work/moc_UpdateService.cpp" "$work/resources.cpp")
libraries=(-L"$qt/lib" -lQt6QuickControls2 -lQt6Quick -lQt6QmlModels -lQt6Qml -lQt6Network -lQt6Widgets -lQt6Gui -lQt6Core -lQt6EntryPoint -lole32 -luser32 -lgdi32 -lshell32 -lws2_32 -luuid -loleaut32 -lcomdlg32 -lcomctl32 -ladvapi32)
"$compiler" -std=gnu++17 -O2 -mwindows -DQT_NO_DEBUG -DWIN32 -D_WINDOWS -DQT_CORE_LIB -DQT_GUI_LIB -DQT_WIDGETS_LIB -DQT_NETWORK_LIB -DQT_QML_LIB -DQT_QUICK_LIB -DQT_TESTLIB_LIB "${includes[@]}" "$repo/app/src/main.cpp" "${sources[@]}" "${libraries[@]}" -o "$work/zoru-miner-gui.exe"
if [ "${ZORU_BUILD_TESTS:-0}" = 1 ]; then
  mkdir -p "$work/tests"
  test_includes=("${includes[@]}" -I"$qt/include/QtTest")
  for name in AppStateTest FileDialogTest UpdateDialogTest; do
    "$moc" "$repo/app/tests/$name.cpp" -o "$work/generated/$name.moc"
    if [ "$name" = AppStateTest ]; then test_sources=("${sources[@]}"); else test_sources=("$work/resources.cpp"); fi
    "$compiler" -std=gnu++17 -O2 -DQT_NO_DEBUG -DWIN32 -D_WINDOWS -DQT_CORE_LIB -DQT_GUI_LIB -DQT_WIDGETS_LIB -DQT_NETWORK_LIB -DQT_QML_LIB -DQT_QUICK_LIB -DQT_TESTLIB_LIB "${test_includes[@]}" "$repo/app/tests/$name.cpp" "${test_sources[@]}" "${libraries[@]}" -lQt6Test -o "$work/tests/$name.exe"
  done
fi
mkdir -p "$output/bin/platforms" "$output/bin/imageformats" "$output/share/zoru-miner"
for binary in zoru-miner-cli zoru-update; do cp "$target_dir/x86_64-pc-windows-gnu/release/$binary.exe" "$output/bin/"; done
cp "$work/zoru-miner-gui.exe" "$output/bin/"
cp "$qt/bin/"*.dll "$output/bin/"
cp "$qt/plugins/platforms/qwindows.dll" "$qt/plugins/platforms/qoffscreen.dll" "$output/bin/platforms/"
cp "$qt/plugins/imageformats/"*.dll "$output/bin/imageformats/"
cp -a "$qt/qml" "$output/"
if [ -d "$qt/plugins/tls" ]; then cp -a "$qt/plugins/tls" "$output/bin/"; fi
printf '[Paths]\nPlugins=.\nQml2Imports=../qml\n' > "$output/bin/qt.conf"
for name in LICENSE README.md SECURITY.md CONTRIBUTING.md; do cp "$repo/$name" "$output/"; done
cp -a "$repo/licenses" "$repo/docs" "$output/"
cp "$manifest" "$output/share/zoru-miner/deployment.json"
python3 - "$output" <<'PY'
import hashlib,json,sys
from pathlib import Path
root=Path(sys.argv[1]);m=json.loads((root/'share/zoru-miner/deployment.json').read_text());declared=m.pop('manifestSha256',None)
assert hashlib.sha256(json.dumps(m,sort_keys=True,separators=(',',':')).encode()).hexdigest()==declared
assert m['chainId']==6736 and m['mode']=='mainnet'
checks={str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(root.rglob('*')) if p.is_file()}
(root/'checksums.json').write_text(json.dumps(checks,indent=2)+'\n')
PY
(cd "$(dirname "$output")" && zip -q -X -r "$(basename "$output").zip" "$(basename "$output")")
makensis -V2 "-DSTAGING=$output" "-DOUTPUT=$output.exe" "-DVERSION=$version" "$repo/packaging/windows-installer.nsi"
sha256sum "$output.zip" "$output.exe"
