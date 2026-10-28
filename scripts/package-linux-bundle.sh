#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
output="${1:-${repo_root}/dist/ad-visualization-linux-x86_64}"
output="$(realpath -m "${output}")"
archive="${output}.tar.gz"
archive_checksum="${archive}.sha256"

if [[ "$(uname -s)" != Linux || "$(uname -m)" != x86_64 ]]; then
  echo "This bundle script currently supports Linux x86_64 only." >&2
  exit 1
fi
if [[ -e "${output}" || -e "${archive}" || -e "${archive_checksum}" ]]; then
  echo "Output already exists; choose a different destination: ${output}" >&2
  exit 1
fi

export CARGO_TARGET_DIR="${repo_root}/target"
cd "${repo_root}"
mkdir -p "$(dirname "${output}")"
stage="$(mktemp -d "$(dirname "${output}")/.ad-visualization-bundle.XXXXXXXX")"
trap 'rm -rf "${stage}"' EXIT
mkdir -p "${stage}/bin" "${stage}/lib/cmake/AdVisualization" \
  "${stage}/include/ad_visualization" "${stage}/examples/cpp" "${stage}/licenses"

cargo build --locked --release -p ad-visualization --features wgpu,png --example fixed_scene
install -m 755 "${CARGO_TARGET_DIR}/release/examples/fixed_scene" "${stage}/bin/fixed_scene_headless"
cargo build --locked --release -p ad-visualization --features window,png --example fixed_scene
install -m 755 "${CARGO_TARGET_DIR}/release/examples/fixed_scene" "${stage}/bin/fixed_scene_window"
cargo build --locked --release -p ad-render --features wgpu,png --example practical_graph
install -m 755 "${CARGO_TARGET_DIR}/release/examples/practical_graph" "${stage}/bin/practical_graph"
cargo build --locked --release -p ad-visualization-ffi --features native
install -m 755 "${CARGO_TARGET_DIR}/release/libad_visualization_ffi.so" "${stage}/lib/libad_visualization_ffi.so"
install -m 644 crates/ad-visualization-ffi/include/ad_visualization_ffi.h "${stage}/include/"
install -m 644 cpp/include/ad_visualization/visualizer.hpp "${stage}/include/ad_visualization/"
install -m 644 cpp/cmake/AdVisualizationConfig.cmake "${stage}/lib/cmake/AdVisualization/"
install -m 644 cpp/example/CMakeLists.txt cpp/example/consumer.cpp "${stage}/examples/cpp/"
install -m 644 crates/ad-render/assets/LIBERATION_LICENSE.txt "${stage}/licenses/LiberationSans-LICENSE.txt"
install -m 644 LICENSE "${stage}/LICENSE"
install -m 644 Cargo.lock "${stage}/Cargo.lock"

# Include license files of available Cargo dependencies used for the build.
cargo_registry_root="${CARGO_HOME:-${HOME:-}/.cargo}/registry/src"
if [[ -d "${cargo_registry_root}" ]]; then
  shopt -s nullglob
  while IFS= read -r package; do
    for source_dir in "${cargo_registry_root}"/*/"${package}"; do
      [[ -d "${source_dir}" ]] || continue
      while IFS= read -r -d '' license_file; do
        relative="${license_file#${source_dir}/}"
        destination="${stage}/licenses/rust/${package}/${relative}"
        mkdir -p "$(dirname "${destination}")"
        install -m 644 "${license_file}" "${destination}"
      done < <(find "${source_dir}" -maxdepth 2 -type f \
        \( -iname 'LICENSE' -o -iname 'LICENSE.*' -o -iname 'COPYING' -o -iname 'NOTICE' \) -print0)
    done
  done < <(awk '/^name = / {name=$3; gsub(/"/,"",name)} /^version = / && name != "" {version=$3; gsub(/"/,"",version); print name "-" version; name=""}' Cargo.lock)
  shopt -u nullglob
fi

cat > "${stage}/run-fixed-scene.sh" <<'LAUNCHER'
#!/bin/sh
set -eu
bundle_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cd "${bundle_dir}"
exec "${bundle_dir}/bin/fixed_scene_headless" "$@"
LAUNCHER
cat > "${stage}/run-fixed-scene-window.sh" <<'LAUNCHER'
#!/bin/sh
set -eu
bundle_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cd "${bundle_dir}"
exec "${bundle_dir}/bin/fixed_scene_window" "$@" --window-smoke
LAUNCHER
cat > "${stage}/run-practical-graph.sh" <<'LAUNCHER'
#!/bin/sh
set -eu
bundle_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cd "${bundle_dir}"
exec "${bundle_dir}/bin/practical_graph" "$@"
LAUNCHER
chmod 755 "${stage}/run-fixed-scene.sh" "${stage}/run-fixed-scene-window.sh" "${stage}/run-practical-graph.sh"

cat > "${stage}/README.md" <<'BUNDLE_README'
# ad-visualization Linux x86_64 bundle

This directory contains Rust rendering examples, the C ABI library and header,
and a C++ adapter. GPU rendering uses wgpu's Vulkan backend. A Rust toolchain
is not needed to run the bundle.

Run from this directory:

```sh
./run-fixed-scene.sh
./run-practical-graph.sh
./run-fixed-scene-window.sh output/window.png
```

The scripts write PNG files under `examples/output/` by default. An explicit
output path replaces the default. Window mode needs an X11 display. Both PNG
and window rendering need a compatible Vulkan loader and GPU driver. The host
must provide compatible Linux system libraries.

For a C++20 application, include `<ad_visualization/visualizer.hpp>` and link
`AdVisualization::Visualizer` from the bundled CMake package:

```sh
cmake -S examples/cpp -B /tmp/ad-visualization-consumer -DCMAKE_PREFIX_PATH="$PWD"
cmake --build /tmp/ad-visualization-consumer
/tmp/ad-visualization-consumer/ad_visualization_cpp_consumer /tmp/cpp_visualizer.png
```

The header accepts application types with `x`/`y`, `x`/`y`/`theta`,
`width`/`height`/`resolution`/`grid`, and the corresponding footprint and scan
fields. `examples/cpp/consumer.cpp` demonstrates this with independent structs.
`prepareMap` returns `ad::visualization::Result<PreparedMap>`; drawing methods
return `ad::visualization::Status`. Check errors with `if (!status)` and
`status.error().message`, then translate to your application's error type.
The raw C ABI remains available in `include/ad_visualization_ffi.h`.

`LICENSE` contains the Apache-2.0 license for this project. `licenses/`
contains available Cargo dependency notices and the embedded Liberation Sans
font license. `SHA256SUMS` lists bundle file hashes.
BUNDLE_README

(cd "${stage}" && find . -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum > SHA256SUMS)
for executable in "${stage}/bin/fixed_scene_headless" "${stage}/bin/fixed_scene_window" "${stage}/bin/practical_graph" "${stage}/lib/libad_visualization_ffi.so"; do
  if ldd "${executable}" | grep -q 'not found'; then
    echo "Unresolved shared library in ${executable}" >&2
    exit 1
  fi
done

mv "${stage}" "${output}"
trap - EXIT
tar -C "$(dirname "${output}")" -czf "${archive}" "$(basename "${output}")"
(cd "$(dirname "${archive}")" && sha256sum "$(basename "${archive}")" > "$(basename "${archive_checksum}")")
echo "Created ${output}"
echo "Created ${archive}"
echo "Created ${archive_checksum}"
