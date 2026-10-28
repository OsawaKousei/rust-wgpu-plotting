# GitHub Release の作成

最初の配布対象は Linux x86_64、Vulkan、ウィンドウ利用時は X11 です。Release asset には Rust のサンプル実行ファイル、C ABI の共有ライブラリとヘッダー、C++20 アダプター、CMake 設定を含む tarball と、その SHA-256 チェックサムを載せます。現時点では `.deb` と Python wheel は含めません。

## ビルドと検証

配布したいコミットをチェックアウトし、変更のない状態で次を実行します。版名は各クレートの `Cargo.toml` と揃えます。

```bash
git status --short
cargo test --workspace --all-features --locked
bash scripts/package-linux-bundle.sh "dist/ad-visualization-v0.1.0-linux-x86_64"
cd dist
sha256sum -c ad-visualization-v0.1.0-linux-x86_64.tar.gz.sha256
```

tarball を展開して `SHA256SUMS` の検証と、ヘッドレスの実行例を確認します。PNG の実行には Vulkan アダプターが必要です。

```bash
tar -xzf ad-visualization-v0.1.0-linux-x86_64.tar.gz
cd ad-visualization-v0.1.0-linux-x86_64
sha256sum -c SHA256SUMS
./run-fixed-scene.sh /tmp/ad-visualization-release-smoke.png
```

必要なら [`README.md`](../README.md) の C++ サンプルも、展開したディレクトリを `CMAKE_PREFIX_PATH` に指定してビルド・実行します。配布先に Rust ツールチェーンは不要ですが、互換性のある Linux システムライブラリと Vulkan ローダー・ドライバーは必要です。

## GitHub への公開

`gh` を使用する場合は、リポジトリのルートに戻り、GitHub へログインしたうえで、成果物を作成したコミットをタグ付けします。初回リリースでは `v0.1.0` を使います。現在の `main` は `dev` より古いため、以下では `dev` のコミットを対象にします。

```bash
gh auth login
git switch dev
git status --short
git push origin dev
git tag -a v0.1.0 -m "v0.1.0"
git push origin v0.1.0
gh release create v0.1.0 \
  dist/ad-visualization-v0.1.0-linux-x86_64.tar.gz \
  dist/ad-visualization-v0.1.0-linux-x86_64.tar.gz.sha256 \
  --repo OsawaKousei/rust-wgpu-plotting \
  --title "v0.1.0 — Linux x86_64 preview" \
  --notes-file docs/releases/v0.1.0.md \
  --prerelease \
  --verify-tag
```

GitHub の画面から作成する場合は、同じ `v0.1.0` タグと対象コミットを選び、2 つのファイルを Release assets に添付します。GitHub が自動で表示する source archive はソースコードで、Linux 用の実行ファイルや `.so` は含まれません。
