# ad-visualization

Rust の `wgpu` を使う 2D 描画ライブラリです。グラフの構成、地図・経路・ロボット・LiDAR の描画、ウィンドウ表示、PNG 保存を提供します。C++ アプリからは C ABI を介して同じ Visualizer を呼び出せます。

## 構成

| クレート | 用途 |
| --- | --- |
| [`ad-render`](crates/ad-render/) | Figure、Axes、線・点・画像・文字、GPU 描画、表示、PNG 出力 |
| [`ad-visualization`](crates/ad-visualization/) | 地図、経路、ロボット、LiDAR 向けの Rust API |
| [`ad-visualization-ffi`](crates/ad-visualization-ffi/) | C++ 呼び出し用の C ABI |

設計と対象範囲は [描画バックエンド仕様書](docs/rust_rendering_backend_spec.md) を参照してください。

## 動作環境と準備

動作確認の対象は Linux x86_64、X11、`wgpu` の Vulkan バックエンドです。Rust（edition 2024 対応）、Vulkan ローダーと対応ドライバーを用意してください。ウィンドウを使う場合は、GLFW の X11 ビルドに必要な C コンパイラーと開発用ライブラリも必要です。PNG 出力にも利用可能な Vulkan アダプターが必要です。CPU のモデルテストは GPU なしで実行できます。

## Rust のサンプルを実行する

地図、経路、ロボット、LiDAR を描いて PNG に保存します。

```bash
cargo run -p ad-visualization --features wgpu,png --example fixed_scene
```

出力先は [`examples/output/visualization_fixed_scene.png`](examples/output/visualization_fixed_scene.png) です。引数にファイル名を渡すと保存先を変更できます。

```bash
cargo run -p ad-visualization --features wgpu,png --example fixed_scene -- /tmp/fixed_scene.png
```

同じシーンをウィンドウに表示してから PNG に保存する場合は X11 が使える環境で実行します。

```bash
cargo run -p ad-visualization --features window,png --example fixed_scene -- --window-smoke
```

一般的なグラフ描画の例もあります。2 段の折れ線グラフ、目盛り、ラベルを [`examples/output/practical_graph.png`](examples/output/practical_graph.png) に保存します。

```bash
cargo run -p ad-render --features wgpu,png --example practical_graph
```

Rust から Visualizer を組み込むときは、地図を一度 `prepare_map` で画像に変換し、フレームごとに `begin_frame` と描画メソッドを呼びます。座標はメートル単位です。PNG 保存には `wgpu,png`、画面表示には `window` feature を有効にしてください。実際の呼び出し順は [固定シーンサンプル](crates/ad-visualization/examples/fixed_scene.rs) を参照してください。

## C++ アプリから使う

配布物には [C++20 の Visualizer ヘッダー](cpp/include/ad_visualization/visualizer.hpp)、[CMake の import 設定](cpp/cmake/AdVisualizationConfig.cmake)、[独立した C++ サンプル](cpp/example/consumer.cpp) を含めます。参照ワークスペースの型を直接依存にせず、`x`/`y`、`x`/`y`/`theta`、`width`/`height`/`resolution`/`grid` などのフィールドを持つアプリ独自の型を受け取ります。

配布物を作成した後、任意の C++20 プロジェクトで `find_package(AdVisualization CONFIG REQUIRED)` と `target_link_libraries(your_app PRIVATE AdVisualization::Visualizer)` を使います。`CMAKE_PREFIX_PATH` に配布ディレクトリを指定してください。配布物内のサンプルは次のコマンドでビルドできます。

```bash
bundle="$PWD/dist/ad-visualization-linux-x86_64"
cmake -S "$bundle/examples/cpp" -B /tmp/ad-visualization-consumer -DCMAKE_PREFIX_PATH="$bundle"
cmake --build /tmp/ad-visualization-consumer
/tmp/ad-visualization-consumer/ad_visualization_cpp_consumer /tmp/cpp_visualizer.png
```

低レベルの [C ABI ヘッダー](crates/ad-visualization-ffi/include/ad_visualization_ffi.h) も引き続き利用できます。

## 配布物を作る

Linux x86_64 用の配布ディレクトリと `.tar.gz` を作成できます。Rust のサンプル実行ファイル、C ABI と C++ ヘッダー、CMake 設定、ライセンス文書を同梱します。

```bash
bash scripts/package-linux-bundle.sh
```

出力は `dist/ad-visualization-linux-x86_64/` と同名の `.tar.gz` です。別の出力先を指定する場合はスクリプトの第 1 引数にディレクトリ名を渡します。既存の配布物は上書きしません。

```bash
./dist/ad-visualization-linux-x86_64/run-fixed-scene.sh
./dist/ad-visualization-linux-x86_64/run-practical-graph.sh
./dist/ad-visualization-linux-x86_64/run-fixed-scene-window.sh output/window.png
```

ヘッドレス版の `run-fixed-scene.sh` は GLFW/X11 にリンクせず、ウィンドウ版は X11 が使える環境で実行します。配布物の実行に Rust ツールチェーンは不要です。対応する Linux のシステムライブラリ、Vulkan ローダーと GPU ドライバーは実行先に必要です。C++ アプリ本体はこの配布物には含めません。

## テスト

GPU を使わないモデル・FFI テストは次のコマンドで実行できます。

```bash
cargo test --workspace
```

全 feature のテストは次のコマンドで実行します。GPU を使うサンプルの実行には Vulkan アダプターが必要です。

```bash
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
```
