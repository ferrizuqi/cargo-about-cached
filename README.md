# cargo-about-cached

cargo-about を CI で実行するときに依存関係が多いと5分近く待たされることがあるので作りました。

ライセンスを `target/.cargo-about-cached/cache.json` にキャッシュすることで、実行を高速化します。

実行方法:
```sh
# 先に cargo-about 本体をインストールしてライセンス生成のためのテンプレートを用意
$ cargo install --locked --features cli cargo-about
$ cargo about init

# cargo-about-cached をインストール
$ cargo install --locked --git https://github.com/ferrizuqi/cargo-about-cached --tag v0.1.0

# キャッシュ付きでライセンス生成
$ cargo about-cached about.hbs -o license.html
```

## 注意

一応両ツールで生成されたライセンスファイルの Hash 値が一致することはコミット時点では確かめていますが、完全に一致するとは限りません。
