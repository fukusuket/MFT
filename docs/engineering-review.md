# 設計レビューと開発規約の提案

作成日: 2026-10-04
対象: [product-overview.md](product-overview.md)
観点: ソフトウェアエンジニアリング / ソフトウェアアーキテクチャ

本書は2部構成である。

- **第1部**：product-overview.md の設計レビュー。実装に入る前に決めておくべき点を挙げる。
- **第2部**：AGENTS.md と CLAUDE.md に書く内容の提案。第1部で決めた方針を、日々の開発ルールとして落とし込む。

---

# 第1部 設計レビュー

重要度は次の3段階で示す。

- **高**：今決めないと、後から直すコストが大きい
- **中**：v0.2までに決める
- **低**：v1.0までに決めればよい

## 1. 指摘の一覧

| # | 重要度 | 分類 | 指摘 |
|---|---|---|---|
| R1 | 高 | 正確性 | NTFSのファイル名をRustの `String` / `Path` で扱えない |
| R2 | 高 | 正確性 | 大文字と小文字の区別の扱いが決まっていない |
| R3 | 高 | 決定性 | 並列処理を使うと、出力の順序が実行ごとに変わる |
| R4 | 高 | セキュリティ | 攻撃者が付けたファイル名がそのままレポートとCSVに入る |
| R5 | 高 | 構造 | `core` クレートの責務が大きすぎる |
| R6 | 高 | 方針の矛盾 | `forbid(unsafe_code)` を宣言しているが、Collectorはunsafeなしでは書けない |
| R7 | 高 | ライセンス | **決定済み（2026-10-04）**：本体はAGPL-3.0（Hayabusaと同じ）。それに伴って守るべきことを整理した |
| R8 | 中 | 性能 | 「ストリーミング処理」と書いているが、Rewindと相関には全件をメモリに持つ必要がある |
| R9 | 中 | 契約 | Rust⇔Viewer間のデータ形式と、JSONL/CSVの形式に、バージョン管理の仕組みがない |
| R10 | 中 | 再現性 | 同じ結果を再現するのに必要な情報（ツール、ルール、ベースラインのバージョン）が、レポートに記録されない |
| R11 | 中 | 時刻 | 時刻の内部表現とタイムゾーンを、どの層で扱うかが決まっていない |
| R12 | 中 | エラー処理 | 「解析を止めるエラー」と「レコード単位の破損」の区別が決まっていない |
| R13 | 中 | Sigma | Sigmaエンジンの、イベント型への依存の仕方が決まっていない |
| R14 | 中 | テスト | テストデータの作り方と置き場所が決まっていない |
| R15 | 低 | 計画 | v0.1〜v0.2では、最初から端から端までつながった状態にならない |
| R16 | 低 | 国際化 | 文言を出す責任がどの層にあるかが決まっていない |

## 2. 各指摘の詳細と提案

### R1 NTFSのファイル名をRustの `String` / `Path` で扱えない（高）

- NTFSのファイル名はUTF-16だが、**対になっていないサロゲートを含みうる**（正当なUTF-16ではない）。攻撃者は意図的にこうした名前を付けることがある。`String::from_utf16_lossy` で変換すると、別々のファイルが同じ名前になってしまう。
- `std::path::Path` は、解析するPCのOSの規則に従う。LinuxやmacOSでは `\` を区切り文字として扱わないため、証拠のWindowsパスを `Path` に入れると、プラットフォームによって挙動が変わる。

**提案**
- 証拠から取ったファイル名は、独自の型 `NtfsName(Box<[u16]>)` で持つ。正規化パスは、元に戻せる形（WTF-8相当）で持つ。
- 表示するときだけ変換し、不正なコード単位は `\u{D800}` のようにエスケープして見えるようにする。
- 証拠のパスに `std::path::Path` / `PathBuf` を使うことを禁止する（規約に書く）。

### R2 大文字と小文字の区別の扱いが決まっていない（高）

- NTFSは大文字と小文字を区別しない（`$UpCase` テーブルで比較する）。一方、SigmaのフィールドはASCII範囲を中心に、大文字と小文字を区別せずに照合するのが前提である。
- ベースラインの検索（fst）は、バイト列が完全に一致するかで照合する。`C:\Windows\system32\` と `C:\Windows\System32\` を別物と判定すると、ノイズが大量に出る。

**提案**
- 比較用のキーは、「`$UpCase` に従って大文字にした正規化パス」に統一する。ベースラインの生成時と解析時で同じ関数を使う。
- `$UpCase` はボリュームごとに異なりうる。MVPでは、Windows既定のテーブルを組み込んで使う。

### R3 並列処理を使うと、出力の順序が実行ごとに変わる（高）

- 「同じ入力からは必ず同じ結果」がこの製品の根幹である。しかし、rayonと `HashMap` を使うと、出力の順序やcorrelationの評価順が実行ごとに変わりうる。

**提案**
- すべての出力は、明示的なキー（レベル → 時刻 → USN → エントリ番号）で並べ替えてから書き出す。
- 出力に直結するコレクションには `BTreeMap` / `IndexMap` を使う。`HashMap` は内部の検索にだけ使う。
- 同じ入力を2回解析して、出力がバイト単位で一致することをCIで検証する（`determinism` テスト）。
- レポートの生成時刻のように、実行ごとに変わる値は1か所にまとめ、テストでは固定値に差し替えられるようにする。

### R4 攻撃者が付けたファイル名がそのままレポートとCSVに入る（高）

ファイル名は**攻撃者が自由に決められる入力**である。

| 出力先 | 起こりうること | 対策 |
|---|---|---|
| `report.html` に埋め込んだJSON | ファイル名に `</script>` が含まれると、埋め込みが途中で終わり、XSSになる | JSONをBase64＋圧縮して埋め込む（product-overview.mdの方針どおり）。Viewerでは `innerHTML` などを使わない（lintで禁止する） |
| `timeline.csv` | `=HYPERLINK(...)` や `=cmd|...` で始まるファイル名を、Excelが数式として実行する（CSVインジェクション） | 先頭が `= + - @ \t \r` のセルは、先頭に `'` を付ける。オプションで無効にできるようにする |
| ターミナルへの出力 | ANSIエスケープシーケンスで、表示を改ざんされる | 制御文字をエスケープしてから表示する |

### R5 `core` クレートの責務が大きすぎる（高）

product-overview.md 6.5節の `core` には、model、parse、resolve、normalize、baseline、factsがまとめて入っている。このままでは、変更の影響範囲が広くなり、ビルドやテストも遅くなる。

**提案するクレート構成と、依存の向き**（矢印は「依存する」を表す。逆向きの依存は禁止）

```
                       cli ─────────────┐
                        │               │
             ┌──────────┼─────────┐     │
             ▼          ▼         ▼     ▼
          report     analyze   collector(cfg windows)
             │          │         │
             │   ┌──────┼──────┐  │
             │   ▼      ▼      ▼  │
             │ detect baseline resolve
             │   │      │      │  │
             │   ▼      │   ┌──┴──┴───┐
             │ sigma    │   ▼         ▼
             │ (NTFS非依存) mft-parse usn-parse
             │          │   │         │
             └──────────┴───┴────┬────┘
                                 ▼
                             ntfs-types   （FileRef、Filetime、NtfsName、正規化パス）
```

| クレート | 責務 | 依存してよいもの |
|---|---|---|
| `ntfs-types` | 値型だけを持つ。I/Oもロジックも持たない | なし（serdeのみ） |
| `mft-parse` / `usn-parse` | バイト列を型付きのレコードに変換する。**判定はしない** | `ntfs-types`、`mft` crate |
| `resolve` | MFTとUSNの突合、Rewindによるパス解決 | parse系 |
| `baseline` | 正規化、fstの検索、ベースラインのmanifest | `ntfs-types` |
| `sigma` | Sigmaの読み込みと評価、correlation。**NTFSを知らない** | なし |
| `detect` | NTFSのイベントをSigmaのイベントとして渡す橋渡し | `sigma`、`resolve`、`baseline` |
| `analyze` | パイプライン全体の組み立て、Facts、Finding | 上記すべて |
| `report` | HTML / JSONL / CSV の出力 | `analyze` の出力型 |
| `collector` | Rawボリュームの読み取り（Windowsのみ） | `ntfs-types`、`mft-parse`（data runs） |
| `cli` | 引数の処理と、進捗の表示 | すべて |

`sigma` をNTFSから独立させておけば、Hayabusaや他のツールと共有するライブラリとして切り出せる。

### R6 `forbid(unsafe_code)` とCollectorが矛盾する（高）

`\\.\C:` を開くことや `DeviceIoControl` の呼び出しには、`windows` クレートを使っても `unsafe` が必要である。

**提案**
- ワークスペース全体の既定は `unsafe_code = "forbid"` にする。
- `collector` クレートだけ `deny` に緩め、FFIを包む1つのモジュールでだけ `#[allow(unsafe_code)]` を許す。すべての `unsafe` ブロックに `// SAFETY:` コメントを必須にする（clippyの `undocumented_unsafe_blocks`）。
- **パーサー（信頼できない入力を扱う層）では、`unsafe` を例外なく禁止する。**

### R7 ライセンス（決定済み）

**決定**：本体はHayabusaと同じ **AGPL-3.0**（GitHub APIで `Yamato-Security/hayabusa` のライセンスが `AGPL-3.0` であることを確認した。2026-10-04）。

| 対象 | ライセンス | 根拠・補足 |
|---|---|---|
| 本体（全クレート、`viewer/`、`collector`） | AGPL-3.0-or-later か AGPL-3.0-only（Hayabusaの表記に合わせる） | Hayabusaのコード（Sigmaエンジン）を流用できる |
| ルール（`tool-rules` リポジトリ） | DRL 1.1 | SigmaHQとhayabusa-rulesのどちらもDRL 1.1。流用元と同じにする |
| ベースラインのデータ（`tool-baselines`） | 未定 | Windowsのファイルパスの一覧であり、Microsoftの著作物そのもの（バイナリ）は含めない。別途確認する |

**この決定によって変わること**
1. **dfir_ntfs（GPLv3）を参照だけにする制約は不要になる。** GPLv3のコードは、AGPLv3の著作物と組み合わせられる（GPLv3、AGPLv3の第13条）。ただし、流用するときは出典と著作権表示を残す。
2. **依存クレートは、AGPLv3と組み合わせられるライセンスに限る。** Apache-2.0、MIT、BSD、ISC、Zlib、Unicode、MPL-2.0、GPL-3.0、LGPL-3.0、AGPL-3.0 は使える。GPL-2.0-only、LGPL-2.1-only、SSPL、BUSL、独自の制限付きライセンスは使えない。これを `deny.toml` で強制する。mft crate（Apache-2.0）は使える。
3. **HTMLレポートは、AGPLのViewerのコードを含んだまま配布される。** レポートをメールで他人に送るとViewerのコードも渡ることになるため、レポートのフッターに、ライセンス名とソースコードの入手先（リポジトリのURLとコミット）を記載する。
4. **DRL 1.1の条件：ルールに一致した結果を出力するときは、ルールの作者（`author`）、ルールへのリンク、DRLであることの表示を残す必要がある。** Findingの詳細、`findings.jsonl` / `timeline.csv` に、ルールの `author` と参照先を含める。SigmaHQから流用したルールの `author` は削らない。
5. **AGPLのネットワーク条項（第13条）の影響は小さい。** MVPはローカルのCLIで、ネットワーク越しにサービスを提供しない。将来localhostサーバーやSaaSの形態にする場合は、改めて確認する。
6. 各ソースファイルの先頭に `// SPDX-License-Identifier: AGPL-3.0-...` を付けるかどうかは、Hayabusaの慣習に合わせる（任意）。`Cargo.toml` の `license` フィールドは必須とする。

### R8 メモリの見積もりがない（中）

- Rewindは、ジャーナルを新しい方から逆順に辿るため、USNを全件メモリに持つ（少なくとも2回読む）必要がある。MFTとUSNの相関でも、エントリ番号から引ける索引が要る。「全レコードをメモリに保持しない」というCodex案の方針は、そのままでは実現できない。
- 目安：100万エントリの `$MFT` と120万件のUSN。1件あたり100〜200バイトの構造体なら、合計で数百MBになる。ファイルサーバー（1,000万エントリ以上）では数GBになる。

**提案**
- 「メモリの上限」を非機能要件に追加する。例：`$MFT` 1GBで、最大使用メモリ2GB以下。
- 名前は `NtfsName` を1か所にまとめて持ち（interning）、構造体からはIDで参照する。タイムスタンプは `u64` で持つ。
- ベンチマークで、処理時間と最大メモリの両方を記録する。

### R9 データ形式にバージョン管理の仕組みがない（中）

次の3つは、**公開された約束（契約）**として扱う。

| 形式 | 誰が読むか | 提案 |
|---|---|---|
| レポートに埋め込むJSON | 同梱のViewer | `schema_version` を持たせる。TypeScriptの型はRustから生成する（`ts-rs` や `typeshare`）。手で書き写さない |
| `findings.jsonl` / `timeline.csv` | 利用者、Hayabusa、Takajo、Timesketch | 列の構成をスナップショットテストで固定する。変更するときはCHANGELOGに書く |
| ベースライン / ルールのパッケージ | 本体 | `format_version` を持たせる。本体は対応していないバージョンを拒否する |

### R10 再現に必要な情報がレポートに残らない（中）

レポート、JSONL、CSVのすべてに、次の情報を記録する。

- ツールのバージョンとgitのコミット
- ルールセットのID（コミットまたはハッシュ）
- ベースラインのIDとハッシュ
- 入力ファイルのSHA-256
- 実行時のオプション（期間、タイムゾーン、`--mask-users` など）

### R11 時刻の扱いが決まっていない（中）

- 内部では、すべてUTCの `Filetime(u64)`（100ナノ秒単位）で持つ。精度を落とさない。
- タイムゾーンは**表示の層（`report` と `cli`）でだけ**適用する。`--from` / `--to` は、入力を受け取った時点でUTCに変換する。
- 日時ライブラリは1つに統一する（`jiff` か `chrono`）。

### R12 エラーの種類が区別されていない（中）

| 種類 | 例 | 扱い |
|---|---|---|
| 致命的なエラー（`Error`） | 入力ファイルがない、`$MFT` のシグネチャがまったくない、出力先に書けない | 処理を中止し、終了コードを0以外にする |
| 診断情報（`Diagnostic`） | fixupの不一致、レコードの切り詰め、パスを解決できない | レコードをスキップして処理を続ける。件数と、代表的なオフセットをレポートの「解析品質」に出す |

パーサーは `Result<Record, Diagnostic>` の列を返し、どこまでを許容するかは呼び出し側が決める。

### R13 Sigmaエンジンの設計が決まっていない（中）

- イベントを `HashMap<String, Value>` に変換してから評価すると、数百万件ではメモリの確保が支配的になる。
- 次のようなtraitを定義し、フィールドを必要になったときだけ取り出すようにする。

```rust
pub trait SigmaEvent {
    fn field(&self, name: &FieldName) -> Option<FieldValue<'_>>;
    fn timestamp(&self) -> Filetime;
}
```

- ルールは読み込み時に一度だけコンパイルする（`endswith` などの修飾子の判定や、大文字小文字を区別しないための前処理）。
- logsourceの対応表（`file_event` ↔ USNの `FILE_CREATE` など）は、コードに埋め込まず、設定データとして持つ。

### R14 テストデータの方針がない（中）

| 種類 | 作り方 | 置き場所 |
|---|---|---|
| 単体テスト用のレコード | **テスト用のビルダー**で、MFTレコードやUSNレコードのバイト列をコードから組み立てる | コード内 |
| 小さな実データ（KB単位） | VMで作り、ラベルを付ける | `testdata/`（gitで管理） |
| 大きな実データ・攻撃コーパス | VMで作る＋公開データセット | 別の場所（リリースアセットかオブジェクトストレージ）。`xtask` で取得する。**gitには入れない** |

公開データセットには、再配布できないものがある。取得スクリプトを置くだけにして、データ本体は再配布しない。

### R15 最初から端から端までつながった状態にならない（低）

v0.1が「CSV出力まで」なので、HTMLレポートやSigmaとの結合で問題が見つかるのが遅くなる。

v0.1の最初の2週間で、「`$MFT` のパース → ベースライン差分 → Sigmaのルール1本 → 最小限のHTML」が最後までつながる骨組み（walking skeleton）を作り、その後で各段を作り込むことを推奨する。

### R16 文言を出す責任が決まっていない（低）

- 解析の層は**コード**（`DiagnosticCode::FixupMismatch` や、ルールID）だけを返し、文章を持たない。
- 日本語と英語の文言は、`report`、`cli`、ルールの説明ファイル（`explain/<lang>/`）にだけ置く。

---

# 第2部 AGENTS.md / CLAUDE.md の提案

## 3. 置き方

- **AGENTS.md を正本とする**。Codexを含む多くのエージェントがこのファイルを読む。
- **CLAUDE.md は AGENTS.md を読み込むだけ**にして、Claude Code固有の補足だけを書く。両方に同じ内容を書くと、片方だけが更新されてずれていく。
- 長くなる規約（テストの詳細、Sigmaのフィールド対応表など）は `docs/` に分け、AGENTS.md からはリンクする。エージェントが毎回読む量を抑えるため。
- 現在、このディレクトリはgitリポジトリになっていない。`git init` のときに、これらのファイルとCIの設定を一緒にコミットすることを推奨する。

## 4. AGENTS.md の案

````markdown
# AGENTS.md

## プロジェクト概要
Windows NTFSの `$MFT` / `$UsnJrnl:$J` を解析するファストフォレンジックツール（Rust）。
Windows標準ベースラインとの差分と期間で絞り込み、Sigmaルールに一致したものを、
根拠つきで自己完結HTMLのレポートに出力する。
設計の詳細: docs/product-overview.md、docs/engineering-review.md

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
crates/
  ntfs-types/   値型（FileRef, Filetime, NtfsName, NormPath）。I/Oもロジックも持たない
  mft-parse/    $MFT → Entry。判定はしない
  usn-parse/    $J → UsnEvent。判定はしない
  resolve/      MFT⇔USNの突合、Rewind
  baseline/     正規化、fstの検索、manifest
  sigma/        Sigmaエンジン。NTFSに依存しない
  detect/       NTFSのイベント → SigmaEventの橋渡し
  analyze/      パイプライン、Facts、Finding
  report/       HTML / JSONL / CSV の出力
  collector/    Rawボリュームの読み取り（cfg(windows)）
  cli/          clap
viewer/         TypeScript + Svelte + Vite（1ファイルのHTMLテンプレート）
rules/          Sigmaルール、explain/<lang>/、正規化ルール（YAML）
testdata/       小さなフィクスチャだけ（1ファイル1MB以下）
xtask/          コーパスの取得、スキーマの生成、リリース作業

**依存の向き**: cli → {report, analyze, collector} → {detect, baseline, resolve} → {parse系, sigma} → ntfs-types。
逆向きの依存や、循環する依存を追加しない。`sigma` はNTFS系のクレートに依存しない。

## コマンド
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace            # 単体テストと結合テスト
cargo test --doc --workspace
cargo insta test --review                # スナップショットを更新したとき
cargo deny check                         # ライセンスと脆弱性
cargo xtask schema                       # Viewer用のTypeScript型を生成する
cargo xtask corpus fetch && cargo nextest run --profile corpus   # 大きなコーパス（任意）
(cd viewer && npm ci && npm run check && npm test)

PRを出す前に、少なくとも fmt / clippy / nextest / deny が通ることを確認する。

## コーディング規約

### 全般
- Edition 2024。MSRVは `rust-toolchain.toml` に固定する。
- ライブラリのクレートでは `thiserror` を使う。`anyhow` は `cli` と `xtask` だけで使う。
- 新しい依存を追加するときは、PRの説明に「理由」「ライセンス」「メンテナンスの状況」を書く。
  まずは標準ライブラリか、既存の依存で済まないかを検討する。

### ライセンス
- 本体は AGPL-3.0、ルール（`rules/`）は DRL 1.1。
- 依存クレートは、AGPLv3と組み合わせられるライセンスに限る（`deny.toml` で強制する）。
  GPL-2.0-only、LGPL-2.1-only、SSPL、BUSL は追加しない。
- 他のプロジェクトのコードを流用するときは、出典と著作権表示を `NOTICE` に追記する。
- ルールに一致した結果を出力するときは、ルールの `author` と参照先を残す（DRL 1.1の条件）。
  SigmaHQから流用したルールの `author` を削らない。
- HTMLレポートのフッターから、ライセンスとソースの入手先（URLとコミット）の表示を消さない。
- 公開APIには doc comment を書く。コメントには「何をしているか」ではなく「なぜそうしているか」を書く。

### unsafeとpanic
- ワークスペースの既定は `unsafe_code = "forbid"`。例外は `collector` のFFIモジュールだけとし、
  すべての `unsafe` ブロックに `// SAFETY:` コメントを書く。
- 信頼できない入力（証拠ファイル）を扱うコードでは、panicしない。
  - `unwrap` / `expect` / `panic!` / `unreachable!` を、テスト以外では使わない
    （clippy: `unwrap_used`, `expect_used`, `panic`）。
  - スライスに `[]` でアクセスしない。`get()` を使う（parse系のクレートでは `indexing_slicing` を有効にする）。
  - オフセットや長さの計算には `checked_*` を使う。`as` による数値の変換は使わず、`try_from` を使う
    （`cast_possible_truncation` を有効にする）。
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
  ベースラインの生成時と解析時で同じ関数（`baseline::normalize`）を使う。
- 時刻は、内部ではUTCの `Filetime(u64)` で持つ。タイムゾーンは report / cli でだけ適用する。

### 決定性
- 出力に関係するコレクションは `BTreeMap` / `IndexMap` を使うか、出力する前に明示的なキーで並べ替える。
- 並列処理（rayon）の結果は、集めてから並べ替える。処理が完了した順に依存しない。
- 現在時刻、乱数、環境変数は `analyze::Context` からだけ取得する（テストで固定できるようにする）。

### 出力の安全性
- HTMLに埋め込むデータは、圧縮してBase64にする。Viewerで `innerHTML` / `{@html}` を使わない。
- CSVでは、先頭が `= + - @ \t \r` のセルの先頭に `'` を付ける。
- ターミナルに出す文字列は、制御文字をエスケープする。

### 契約（形式）の変更
- 次のものを変えるときは、バージョンを上げてCHANGELOGに書く。
  - 埋め込みJSONの `schema_version`
  - `findings.jsonl` / `timeline.csv` の列
  - ベースラインやルールパッケージの `format_version`
- Viewer用のTypeScript型は手で書かず、`cargo xtask schema` で生成する。

## テスト規約

### 何をテストするか
| 層 | 必須のテスト |
|---|---|
| parse系 | 正常系、境界値、破損（fixupの不一致、切り詰め、不正な長さ）、4Kn（1レコード4096バイト）。テスト用のビルダーでバイト列を組み立てる。proptestで「任意のバイト列でもpanicしない」ことを確かめる |
| resolve | 親の削除、エントリの再利用（シーケンス番号の不一致）、リネームの前後、Rewindで巻き戻した結果。解決できないときに推測でパスを確定しないこと |
| baseline | 正規化（ユーザー名、SID、GUID、WinSxS）、大文字と小文字の区別、言語の違い、最も近いビルドへのフォールバック |
| sigma | 修飾子ごとの照合、correlation（`event_count` / `temporal_ordered`）、SigmaHQのルールの読み込み |
| rules | 各ルールに陽性と陰性のテストケースを最低1つずつ用意する（`rules/tests/<rule-id>.yml`） |
| report | 出力のスナップショット（insta）。悪意のあるファイル名（`</script>`、`=cmd|...`、ANSIエスケープ、不正なサロゲート）のテスト |
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
- `level` は docs/rule-levels.md の基準で付ける。
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
````

## 5. CLAUDE.md の案

````markdown
# CLAUDE.md

@AGENTS.md

## Claude Code向けの補足
- 複数のクレートにまたがる変更、データ形式の変更、新しい依存の追加は、プランモードで方針を
  示してから着手する。
- 編集した後は、少なくとも対象のクレートで
  `cargo clippy -p <crate> --all-targets -- -D warnings` と `cargo nextest run -p <crate>` を実行する。
  完了と報告する前に、ワークスペース全体のチェックを実行する。
- insta のスナップショットが変わったときは、差分を確認して、意図した変更であることを報告に書く。
  勝手に承認しない。
- `testdata/` の外にある証拠ファイルやコーパスは、読むだけにする。移動、変更、削除をしない。
- Sigmaルールを追加するときは、陽性と陰性のテストケース、ja / en の説明ファイルを同じ変更に含める。
````

## 6. AGENTS.md と一緒に用意するもの

AGENTS.md に書いた規約は、できるだけ**ツールで強制**する。文章で書いただけの規約は守られなくなる。

| ファイル | 内容 |
|---|---|
| `rust-toolchain.toml` | チャネルを固定する（例: `1.98`）。`components = ["rustfmt", "clippy"]` |
| `Cargo.toml`（`[workspace.lints]`） | `unsafe_code = "forbid"`、`unwrap_used` / `expect_used` / `panic` / `indexing_slicing` / `cast_possible_truncation` / `undocumented_unsafe_blocks` = `deny` |
| `Cargo.toml`（`[profile.release]`） | `overflow-checks = true`（パーサーの算術オーバーフローを見逃さないため）、`lto = "thin"`、`codegen-units = 1` |
| `deny.toml` | 許可するライセンスの一覧（R7のAGPLv3と組み合わせられるもの）、RustSecの脆弱性情報、重複する依存の警告 |
| `LICENSE` / `NOTICE` | 本体はAGPL-3.0の全文。`NOTICE` に、流用したコード（Hayabusa、dfir_ntfsなど）の出典を書く。ルールのリポジトリにはDRL 1.1を置く |
| `.config/nextest.toml` | `default` と `corpus` のプロファイル |
| `clippy.toml` | `disallowed-types = ["std::path::Path", "std::path::PathBuf"]` をparse系、resolve、baseline、analyzeに適用する（クレートごとに設定する） |
| `.github/workflows/ci.yml` | Linux / macOS / Windows のマトリクスで fmt / clippy / nextest / deny / viewerを実行する。fuzzの短時間実行。determinismテスト |
| `.github/workflows/nightly.yml` | 長時間のfuzz、コーパスでの差分テスト、精度の評価、ベンチマーク |
| `docs/adr/` | 設計判断の記録。最初のADRとして、本書のR1〜R7の決定を記録する |
| `docs/rule-levels.md` | ルールのレベルを付ける基準（product-overview.md 11章 #5） |

## 7. 次にやること

1. ~~R7 ライセンスを決める~~ → AGPL-3.0に決定（2026-10-04）。`LICENSE`、`NOTICE`、`deny.toml` を用意する。
2. `git init` し、第2部の AGENTS.md、CLAUDE.md、6節の設定ファイルを最初のコミットにする。
3. `ntfs-types`（`NtfsName`、`Filetime`、`NormPath`）を最初に実装する。R1〜R3の方針は、この型で決まる。
4. walking skeleton（R15）を作る。
