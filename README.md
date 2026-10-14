# cargo-about-cached

`cargo-about` を CI で実行すると、依存関係が多いプロジェクトではライセンス情報の収集や `cargo metadata` に時間がかかることがあります。

`cargo-about-cached` は、`cargo-about` によるライセンス生成結果と Cargo metadata をキャッシュし、2回目以降の実行を高速化するためのツールです。

キャッシュは `target/.cargo-about-cached/` 以下に保存されます。

```text
target/.cargo-about-cached/
├── metadata/
└── licenses.json
```

依存関係に変更がない場合は metadata とライセンス情報の両方を再利用します。

依存ライブラリのバージョンが変更された場合は metadata を再取得しつつ、変更されていないライブラリのライセンスキャッシュはそのまま再利用します。

## インストールと実行

<!-- x-release-please-start-version -->
```sh
# cargo-about のテンプレートを用意
$ cargo install --locked --features cli cargo-about
$ cargo about init

# cargo-about-cached をインストール
$ cargo install --locked --git https://github.com/ferrizuqi/cargo-about-cached --tag v0.2.0

# キャッシュ付きでライセンスファイルを生成
$ cargo about-cached about.hbs -o license.html
```
<!-- x-release-please-end -->

初回実行時は通常どおり依存関係とライセンス情報を収集するため、キャッシュによる高速化は行われません。

2回目以降は利用可能なキャッシュを再利用します。

## キャッシュ

Cargo metadata のキャッシュは、workspace の情報、`Cargo.lock`、Cargo のバージョンなどをもとに管理されます。

ライセンス情報はパッケージ単位でキャッシュされるため、一部の依存ライブラリだけが更新された場合でも、変更されていないパッケージのキャッシュを再利用できます。

そのため、依存関係を変更した場合でもキャッシュ全体を破棄して最初から収集し直す必要はありません。

キャッシュを完全に削除したい場合は、次のディレクトリを削除してください。

```text
target/.cargo-about-cached/
```

## cargo-about との互換性

`cargo-about-cached` では、キャッシュを使用しない場合、キャッシュを使用した場合、依存ライブラリのバージョンを変更した場合について、同じ依存バージョンでビルドした `cargo-about` と生成結果を比較する互換性テストを行っています。

テストでは生成されたファイルの SHA-256 Hash が一致することを確認しています。

ただし、すべてのプロジェクト構成や `cargo-about` の設定について完全な互換性を保証するものではありません。

生成されたライセンスファイルについては、利用するプロジェクト側でも内容を確認することを推奨します。

## License

Licensed under either of

- Apache License, Version 2.0
- MIT License

at your option.
