# 現在利用できる機能

この文書は、現行コードから利用できる API と動作条件をまとめたものです。配布先ごとの公開状況ではなく、ローカルのソースまたは Linux 向けバンドルから使える機能を示します。

## Rust API

### `ad-render`: 汎用の 2D Figure

- `Figure` に複数の `Axes` を配置し、座標範囲、縦横比、背景、タイトル、軸ラベル、目盛りを設定できます。
- 線、マーカー、RGBA 画像、文字を追加できます。線の色・太さ・破線、描画順、表示状態を設定し、追加後にデータやスタイルを更新できます。
- `FigureCompiler` は Figure を描画命令へ変換します。この段階までは GPU を使いません。
- `wgpu` feature で Vulkan バックエンドの描画・読み戻しが使えます。`wgpu,png` では `HeadlessCanvas` で Figure を PNG に保存できます。
- `png` feature の `PngWriter` は、既存の `CpuImage` を GPU なしで PNG ファイルに保存できます。
- `window` feature で GLFW/X11 のウィンドウ表示が使えます。`window` は `wgpu` を含みます。

複数のグラフ、目盛り、ラベルを使った実例は [`practical_graph.rs`](../crates/ad-render/examples/practical_graph.rs) にあります。

### `ad-visualization`: 地図向け Visualizer

`MapData` は幅・高さ・解像度と `i8` の占有格子を受け取ります。格子の 0 行目は地図の下端で、正の値を占有セル、0 以下を空きセルとして表示します。`prepare_map` は格子を描画用画像に変換するため、同じ地図を複数フレームで再利用できます。座標と解像度はメートル単位です。

フレームごとに `begin_frame` を呼ぶと、前フレームの経路などを消して地図を配置します。その後、経路、ロボットの外形と向き、マーカー、点群、LiDAR の有効な測距点を追加できます。Rust API では `PathStyle` による経路の色・太さ・破線指定と、`MapPalette` による地図の色変更もできます。`PathStyle` の太さ・破線に使う `Points` 型は `ad-render` に定義されています。

次は、地図と経路を PNG に保存する最小例です。利用するクレートの依存設定で `ad-visualization` の `wgpu,png` feature を有効にしてください。

```rust
use std::path::Path;
use ad_visualization::{MapData, Point, Visualizer};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let map = MapData::new(2, 2, 0.5, vec![0, 100, 0, 0])?;
    let prepared = Visualizer::prepare_map(&map)?;
    let mut viz = Visualizer::new()?;
    viz.begin_frame(&prepared)?;
    let path = [Point::new(0.1, 0.1)?, Point::new(0.8, 0.8)?];
    viz.render_path(&path, &prepared.geometry)?;
    viz.save_figure(Path::new("figure.png"))?;
    Ok(())
}
```

`save_figure` の初期設定は 800×600 ピクセル、100 DPI です。Rust では `save_figure_with_spec` に `RenderSpec` を渡して変更できます。PNG 保存には Vulkan アダプターが必要ですが、ウィンドウや X11 は不要です。画面表示は `window` feature の `open_window` と `present_frame` を使います。詳しい例は [`fixed_scene.rs`](../crates/ad-visualization/examples/fixed_scene.rs) にあります。

## C ABI と C++20 アダプター

`ad-visualization-ffi` は、不透明ハンドルと C 互換の構造体で Visualizer を公開します。[C ヘッダー](../crates/ad-visualization-ffi/include/ad_visualization_ffi.h)には地図の準備、フレーム開始、経路・ロボット・マーカー・点群・LiDAR の描画、PNG 保存、ウィンドウ表示、ハンドル破棄の関数があります。呼び出し結果はステータスコードと診断メッセージで取得します。

[C++20 アダプター](../cpp/include/ad_visualization/visualizer.hpp)は C ABI のハンドルを管理し、アプリ側の `x`/`y` などのフィールドを持つ型を受け取ります。`Status` / `Result` で失敗を確認できます。[独立した利用例](../cpp/example/consumer.cpp)と [CMake の import 設定](../cpp/cmake/AdVisualizationConfig.cmake)があります。

FFI クレートの既定 feature ではシーン構築まで使えます。`ad_viz_save_figure` と `ad_viz_present_frame` を使う共有ライブラリは `native` feature でビルドしてください。`native` は現在 PNG 保存と GLFW/X11 ウィンドウ表示をまとめて有効にします。C ABI / C++ アダプターからは出力サイズを指定できず、PNG は既定の 800×600 ピクセルです。ウィンドウも未作成の場合は 800×600 で開きます。

C ABI / C++ アダプターの経路スタイル文字列は `b-`、`r-`、`m-`、`k--`、`c--` に対応します。マーカーと点群の色名は `red`、`green`、`blue`、`black`、`cyan`、`magenta`、`lime` に対応します。Rust の `PathStyle` より指定できる範囲は狭くなります。

## 動作条件と現在の範囲

- 動作確認の対象は Linux x86_64、Vulkan、ウィンドウ利用時の X11 です。GPU 描画には利用可能な Vulkan ローダー・アダプター・ドライバーが必要です。
- CPU 上の Figure 構築、地図の準備、描画命令への変換、`CpuImage` の PNG 書き出しには GPU が不要です。
- `ad-visualization` は占有格子地図を起点とする API です。地図を使わないグラフや複数 Axes の編集には `ad-render` を使います。
- C ABI / C++ アダプターは Visualizer の地図向け機能を公開します。Rust の汎用 Figure API と同じ操作は提供していません。
- Python ラッパー、`.deb`、APT リポジトリ、PyPI wheel は現時点ではありません。配布用に作れるのは Linux x86_64 のディレクトリと tarball です。

`cargo test --workspace --all-features` は主に CPU のモデル、表示リスト、PNG エンコード、FFI 境界を検証します。GPU 描画の確認には README の PNG サンプルを、ウィンドウ表示の確認には X11 上でのウィンドウ付きサンプルを実行してください。
