# (仮称) NTFS Fast Triage

Windows NTFSの `$MFT` と `$UsnJrnl:$J` だけを使い、**Windows標準ベースラインとの差分**と**期間**で、
調べるべきファイルとファイル活動を洗い出すファストフォレンジックツール（開発初期段階）。

- イベントログが削除されていても解析できる
- 収集データは数十〜数百MB。解析結果は、ブラウザで開くだけのHTML 1ファイル
- 判定はSigmaルールの固定レベルだけで行う（計算スコアは使わない）

詳細は [docs/product-overview.md](docs/product-overview.md) を参照。

## 開発
開発の規約は [AGENTS.md](AGENTS.md) を参照。

```sh
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace --no-tests=pass
cargo deny check
```

## ライセンス
- 本体: [AGPL-3.0-only](LICENSE)
- 検知ルール: [Detection Rule License (DRL) 1.1](https://github.com/SigmaHQ/Detection-Rule-License)
