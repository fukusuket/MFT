# AGENTS.md

## プロジェクト概要
Windows NTFSの `$MFT` / `$UsnJrnl:$J` を解析するファストフォレンジックツール（Rust）。
Windows標準ベースラインとの差分と期間で絞り込み、Sigmaルールに一致したものを、
根拠つきで自己完結HTMLのレポートに出力する。ツール名は未定。

設計資料:
- docs/product-overview.md … 課題、ユーザ、MVP、アーキテクチャ、CLIとUI
- docs/engineering-review.md … 設計レビュー（R1〜R16）と、本ファイルの根拠
- docs/adr/ … 設計判断の記録

### 絶対に守る製品方針
- **計算スコアを導入しない。** 重みの合算、係数、確率で順位を付けない。判定は、Sigmaルールの
  固定 `level` と、一致したかどうかだけで行う。表示順は「レベル → 時刻」に固定する。
- **判定ロジックをRustのコードに書かない。** 「怪しいかどうか」はルールファイル（`rules/`）に書く。
  コードは事実（フィールド、Facts）を渡すだけにする。
- **同じ入力からは、バイト単位で同じ出力を返す。**
- **外部と通信しない。** 解析・レポートの処理に、ネットワークアクセスを追加しない。
- 解析の対象は `$MFT`、`$J`、`$Boot`、`$Secure:$SDS` だけ。`$LogFile` などへのスコープの拡大は、
  issueで合意してから行う。

## リポジトリ構成
現時点で存在するのは `ntfs-types` だけ。残りは予定（クレートを追加するときは、この表に従う）。

crates/
  ntfs-types/   値型（FileRef, Filetime, NtfsName, NormPath）。I/Oもロジックも持たない
  mft-parse/    $MFT → Entry。判定はしない                       （予定）
  usn-parse/    $J → UsnEvent。判定はしない                      （予定）
  resolve/      MFT⇔USNの突合、Rewind                           （予定）
  baseline/     正規化、fstの検索、manifest                       （予定）
  sigma/        Sigmaエンジン。NTFSに依存しない                    （予定）
  detect/       NTFSのイベント → SigmaEventの橋渡し               （予定）
  analyze/      パイプライン、Facts、Finding                      （予定）
  report/       HTML / JSONL / CSV の出力                        （予定）
  collector/    Rawボリュームの読み取り（cfg(windows)）             （予定）
  cli/          clap                                            （予定）
viewer/         TypeScript + Svelte + Vite（1ファイルのHTMLテンプレート）（予定）
rules/          Sigmaルール、explain/<lang>/、正規化ルール（YAML）   （予定）
testdata/       小さなフィクスチャだけ（1ファイル1MB以下）
xtask/          コーパスの取得、スキーマの生成、リリース作業          （予定）
docs/           設計資料、ADR

**依存の向き**: cli → {report, analyze, collector} → {detect, baseline, resolve} → {parse系, sigma} → ntfs-types。
逆向きの依存や、循環する依存を追加しない。`sigma` はNTFS系のクレートに依存しない。

## コマンド
ツールの準備: `cargo install --locked cargo-nextest cargo-deny cargo-insta`

```sh
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace --no-tests=pass   # 単体テストと結合テスト
cargo test --doc --workspace
cargo insta test --review                       # スナップショットを更新したとき
cargo deny check                                # ライセンスと脆弱性
```

予定（実装されたら有効になる）:
```sh
cargo xtask schema                              # Viewer用のTypeScript型を生成する
cargo xtask corpus fetch && cargo nextest run --profile corpus
(cd viewer && npm ci && npm run check && npm test)
```

PRを出す前に、少なくとも fmt / clippy / nextest / deny が通ることを確認する。

## コーディング規約

### 全般
- Edition 2024。ツールチェーンは `rust-toolchain.toml` に固定する。MSRVはワークスペースの `rust-version`。
- ライブラリのクレートでは `thiserror` を使う。`anyhow` は `cli` と `xtask` だけで使う。
- 新しいクレートは `[lints] workspace = true` を必ず指定し、`[workspace.package]` の値を継承する。
- 新しい依存を追加するときは、PRの説明に「理由」「ライセンス」「メンテナンスの状況」を書く。
  まずは標準ライブラリか、既存の依存で済まないかを検討する。
- 公開APIには doc comment を書く。コメントには「何をしているか」ではなく「なぜそうしているか」を書く。

### ライセンス
- 本体は AGPL-3.0-only、ルール（`rules/`）は DRL 1.1（docs/adr/0001-license.md）。
- 依存クレートは、AGPLv3と組み合わせられるライセンスに限る（`deny.toml` で強制する）。
  GPL-2.0-only、LGPL-2.1-only、SSPL、BUSL は追加しない。
- 他のプロジェクトのコードを流用するときは、出典と著作権表示を `NOTICE` に追記する。
- ルールに一致した結果を出力するときは、ルールの `author` と参照先を残す（DRL 1.1の条件）。
  SigmaHQから流用したルールの `author` を削らない。
- HTMLレポートのフッターから、ライセンスとソースの入手先（URLとコミット）の表示を消さない。

### unsafeとpanic
- ワークスペースの既定は `unsafe_code = "forbid"`。例外は `collector` のFFIモジュールだけとし、
  すべての `unsafe` ブロックに `// SAFETY:` コメントを書く。
- 信頼できない入力（証拠ファイル）を扱うコードでは、panicしない。
  - `unwrap` / `expect` / `panic!` を、テスト以外では使わない（ワークスペースのlintで `deny`）。
    テストモジュールでは `#[allow(clippy::unwrap_used, clippy::expect_used)]` を付けてよい。
  - parse系のクレートでは `#![deny(clippy::indexing_slicing)]` を有効にし、スライスには `get()` でアクセスする。
  - オフセットや長さの計算には `checked_*` を使う。`as` による数値の変換は使わず、`try_from` を使う。
  - 入力から読んだ長さや件数で、メモリを確保する量を決めない。必ず上限を設ける。

### エラーと診断
- 処理を中止するエラーは `Error`、レコード単位の問題は `Diagnostic` として扱う。
  パーサーは破損したレコードをスキップして処理を続け、`Diagnostic` として報告する。
- `Diagnostic` にはコード（enum）とオフセットを持たせる。文章は持たせない（文章は report / cli で作る）。

### 文字列、パス、時刻
- 証拠のファイル名は `NtfsName`（UTF-16のまま）で持つ。`String` には、表示するときにだけ
  エスケープして変換する。`from_utf16_lossy` で作った値を、比較や検索のキーに使わない。
- 証拠のパスに `std::path::Path` / `PathBuf` を使わない（解析するPCのOSの規則に依存するため）。
  `Path` を使ってよいのは、解析するPC上の入出力ファイルを指すときだけ。
- パスを比較するときは、`$UpCase` で大文字にした `NormPath` のキーを使う。正規化には、
  ベースラインの生成時と解析時で同じ関数を使う。
- 時刻は、内部ではUTCの `Filetime(u64)` で持つ。タイムゾーンは report / cli でだけ適用する。

### 決定性
- 出力に関係するコレクションは `BTreeMap` / `IndexMap` を使うか、出力する前に明示的なキーで並べ替える。
- 並列処理（rayon）の結果は、集めてから並べ替える。処理が完了した順に依存しない。
- 現在時刻、乱数、環境変数は、解析のコンテキスト（`analyze::Context`）からだけ取得する（テストで固定できるようにする）。

### 出力の安全性
ファイル名は攻撃者が自由に決められる入力として扱う。
- HTMLに埋め込むデータは、圧縮してBase64にする。Viewerで `innerHTML` / `{@html}` を使わない。
- CSVでは、先頭が `= + - @ \t \r` のセルの先頭に `'` を付ける。
- ターミナルに出す文字列は、制御文字をエスケープする。

### 契約（形式）の変更
- 次のものを変えるときは、バージョンを上げてCHANGELOGに書く。
  - 埋め込みJSONの `schema_version`
  - `findings.jsonl` / `timeline.csv` の列
  - ベースラインやルールパッケージの `format_version`
- Viewer用のTypeScript型は手で書かず、Rustから生成する。
- 出力には、ツールのバージョンとコミット、ルールセットID、ベースラインID、入力のSHA-256、実行時のオプションを記録する。

## テスト規約

### 何をテストするか
| 層 | 必須のテスト |
|---|---|
| parse系 | 正常系、境界値、破損（fixupの不一致、切り詰め、不正な長さ）、4Kn（1レコード4096バイト）。テスト用のビルダーでバイト列を組み立てる。proptestで「任意のバイト列でもpanicしない」ことを確かめる |
| resolve | 親の削除、エントリの再利用（シーケンス番号の不一致）、リネームの前後、Rewindで巻き戻した結果。解決できないときに推測でパスを確定しないこと |
| baseline | 正規化（ユーザー名、SID、GUID、WinSxS）、大文字と小文字の区別、言語の違い、最も近いビルドへのフォールバック |
| sigma | 修飾子ごとの照合、correlation（`event_count` / `temporal_ordered`）、SigmaHQのルールの読み込み |
| rules | 各ルールに陽性と陰性のテストケースを最低1つずつ用意する（`rules/tests/<rule-id>.yml`） |
| report | 出力のスナップショット（insta）。悪意のあるファイル名（`</script>`、`=cmd|...`、ANSIエスケープ、不正なサロゲート）のテスト。DRLの帰属表示（author）が出力に含まれること |
| E2E | 小さなフィクスチャ → `analyze` → JSONL のスナップショット。同じ入力を2回解析して、出力がバイト単位で一致すること |

### ルール
- バグを直すときは、まず再現するテストを書いて、それが失敗することを確認してから直す。
- スナップショットを更新するときは、PRにその差分の理由を書く。`--accept` で一括承認しない。
- fuzzのターゲット（`fuzz/`）は、parse系に新しい入口を足したら一緒に追加する。CIでは短時間実行し、
  nightlyで長時間実行する。
- `testdata/` に置くのは1MB以下のファイルだけ。大きなデータはgitに入れず、`xtask corpus` で取得する。
  実在の組織や個人のデータを含むファイルは置かない。
- 差分テスト（MFTECmd、usnjrnl_rewind との比較）と、精度の評価（再現率、ノイズの件数）は、
  `corpus` プロファイルでnightlyに実行する。

### 性能
- `benches/` で、処理時間と最大メモリを計測する。目標は、`$MFT` 1GB＋`$J` 32MBを、
  1分以内かつ2GB以下で処理すること。
- 性能が退行した場合（10％以上）は、PRに理由を書く。

## ルール（rules/）を変更するとき
- 形式はSigma標準に従う。独自フィールドは追加しない。ベースライン外に限定したいときは、
  logsourceの `service: baseline_outside` を使う。
- `level` は docs/rule-levels.md の基準で付ける（文書は作成予定）。
- 説明文（解釈、制約、推奨確認事項）は `rules/explain/{ja,en}/<rule-id>.yml` に書く。
  ルールの本体には書かない。

## 作業の進め方
- 1つのPRでは、1つの目的だけを扱う。リファクタリングと機能の追加を混ぜない。
- 複数のクレートにまたがる変更や、契約（形式）の変更をするときは、先に設計メモ
  （`docs/adr/NNNN-*.md`）を書く。
- コミットメッセージは Conventional Commits（`feat(resolve): ...`）に従う。
- してはいけないこと：
  - 証拠ファイルやコーパスをコミットする
  - テストを消したり `#[ignore]` にしたりして、CIを通す
  - 計算スコアを導入する
  - ネットワークアクセスを追加する
  - 証拠ファイルを書き込みモードで開く
