# アーキテクチャ提案

作成日: 2026-10-04
前提: Rust / ブラウザベース / 対象は$MFTと$UsnJrnl（$LogFileは除外）
関連: [problem-statement.md](problem-statement.md), [research-ntfs-artifacts.md](research-ntfs-artifacts.md)

---

## 1. 設計方針

| 方針 | 理由 |
|---|---|
| **1つのRustコアに、複数のフロントエンド**（CLI / HTMLレポート / 将来はWASM） | 解析ロジックを1か所に集約し、配布形態を増やしても挙動がぶれないようにする |
| **重い処理はCLI、閲覧はブラウザ** | ファイルサーバーの$MFTは数GBになり、ブラウザ（wasm32のメモリ上限、UIスレッド）で処理するのは厳しい。ブラウザには絞り込み済みの結果だけを渡す |
| **成果物は1ファイルの自己完結HTML** | サーバーもインストールも不要で、メールで送れる。データを外部に送信しない。Hayabusa/Takajoのレポートと同じ配布感覚にする |
| **ベースラインはデータとして分離・バージョン管理する** | ツール本体とは別のサイクルで更新する（Hayabusaのルールリポジトリと同じ運用） |
| **コアは「事実」を項目として渡すだけにし、判定ロジックはすべてルール側に書く** | 何を怪しいとみなすかが、ルールファイルを読めば全部わかるようにする。コードの中に隠れた判定を作らない |
| **判定は明示的なルール（Sigma構文）のみで行い、計算スコアは使わない** | 重みの合算のようなブラックボックスを排除する。各Findingは「どのルールの、どの条件に一致したか」と根拠の値だけで構成し、同じ入力からは必ず同じ結果になるようにする。非専門家にも説明でき、専門家が検証できる |

---

## 2. 全体構成

```
 ┌───────────── 調査対象のWindows端末 ─────────────┐
 │  [Collector]  tool collect                       │
 │   ・\\.\C: をRawで読み、$MFT・$J（スパース除去）・$Boot を取得 │
 │   ・zstdで圧縮し、メタ情報(OS build, hostname, 取得時刻)を付与 │
 └──────────────┬──────────────────────────────┘
                │ collection.zip（数十〜数百MB）
                ▼
 ┌───────────── 解析用PC（Win/Mac/Linux）─────────────┐
 │  [Analyzer]  tool analyze --from .. --to ..      │
 │   core: parse → resolve(Rewind) → normalize      │
 │         → baseline subtract → detect(Sigma)      │
 │                 ▲                                │
 │   [Baseline DB] / [Sigmaルール] (別リポジトリ)    │
 └──────────────┬──────────────────────────────┘
                │ report.html（データ埋め込み・1ファイル）
                │ + findings.jsonl / timeline.csv（任意）
                ▼
 ┌───────────── 任意のブラウザ ─────────────┐
 │  [Viewer] サマリ / 要確認リスト / タイムライン / ツリー差分 │
 │  ・完全オフラインで動作し、外部通信なし           │
 └──────────────────────────────────────┘
```

**入力の多様性**：Collectorを使わなくても、KAPE / Velociraptor / FTK Imagerで取得した`$MFT`と`$J`、またはマウントしたイメージ（raw/dd）も入力にできる。既存の運用を壊さない。

---

## 3. コンポーネント詳細

### 3.1 Collector（`tool collect`）
- **方式**：ボリュームハンドル（`\\.\C:`）をRawで読み、$Bootを解釈して、$MFTのdata runsに沿って読み出す。$UsnJrnl:$Jは、スパースの先頭領域をスキップして実データ部分だけを取得する。
- **出力**：`collection.zip`（zstd圧縮）
  - `$MFT`
  - `$J`
  - `$Boot`
  - `meta.json`（ホスト名、OSビルド、UBR、タイムゾーン、ボリューム情報、取得時刻、ツールバージョン、ハッシュ）
- **付加情報**：OSビルドはベースラインの選択に使う。`NtfsDisableLastAccessUpdate`の値やインストール日時など、判定精度に効く最小限のレジストリ値を取得する（オプション。スコープの拡大は最小限にとどめる）。
- **非機能要件**：管理者権限が必要。コード署名する（EDRの誤検知対策）。複数ボリュームに対応する。読み取り専用で動作する。
- **配布**：Windows x64/ARM64向けのシングルexe。

### 3.2 Core（Rustライブラリ）

処理パイプライン

| 段 | 処理 | 実装方針 |
|---|---|---|
| 1. Parse MFT | FILEレコード → `Entry`（エントリ番号、シーケンス番号、フラグ、$SI/$FNのタイムスタンプ、サイズ、ADS、親参照、resident有無） | `mft` crate（Apache-2.0/MIT）を第一候補とする。4Kn（4096バイトのレコード、512バイト単位のfixup）のテストを必須にする |
| 2. Parse USN | $J → `UsnEvent`（V2/V3/V4、Reason、時刻、FRN、親FRN、名前） | 公開仕様に基づき自前実装する（軽量でテスト容易） |
| 3. Resolve paths | MFTの親参照＋**Rewindアルゴリズム**で、USNイベントのフルパスを復元する | 解決できないものは`unknown`、推定の場合は`inferred`を明示する |
| 4. Normalize | パスを正規化する（`C:\Users\<name>` → `%USERPROFILE%`、SID、GUID、WinSxSやDriverStoreのバージョン・ハッシュ部、一時ファイル名のランダム部） | 正規化ルールは宣言的なYAMLで、ベースラインと同じリポジトリで管理する |
| 5. Baseline subtract | 正規化パスがベースラインにあれば「標準」とする。結果は**Sigmaの判定条件には使わず**、Findingの属性（標準/標準外）と、ベースライン差分ビュー（3.5節）に使う | ルックアップは`fst` crate（有限状態トランスデューサ）で、高速かつ省メモリにする |
| 6. Time window | `--from/--to`での抽出。指定がなければ自動提案（USNの活動スパイク、新規実行ファイルの集中期間） | — |
| 7. Detect | Sigmaルール（3.3節）を、ベースラインの内外を問わず**全件**のUSNイベントとMFTエントリに適用する（標準ファイルの削除、例えば`.evtx`の削除を見逃さないため） | 一致したルールはすべて列挙し、合算・スコア化はしない。レベルはSigmaルールの`level`（固定値）をそのまま使う |

**内部データモデル**：`Entry` / `UsnEvent` / `Finding { target, matched_rules[{ rule_id, title, level, evidence }], attributes }`（`attributes`は3.3節の「Sigmaの外で扱う事実」）。将来ほかのファイルシステムを追加する余地を残すため、パーサー層と解析層を分ける。

### 3.3 検知ルール（Sigma構文）

**方針**：検知ルールの形式は**Sigmaに統一する**。理由は次のとおり。
- 判定がルールファイルにすべて書かれ、レベルも固定値なので、ブラックボックスにならない。
- SigmaHQの既存ルールを流用できる。Windowsの`file_*`系ルールのうち、**プロセス系の項目（`Image`/`CommandLine`/`User`等）を使わず、ファイル名だけで判定するルールが約120本**ある（2026-10時点、`file_event` 113本、`file_delete` 8本、ほか）。
- Hayabusaで培ったSigmaエンジン（correlationルール対応を含む）の知見・コードを活かせる。ユーザーもSigmaに慣れている。

**方針：独自フィールド・独自カテゴリは使わない**。Sigma標準のlogsourceとフィールド（Sysmonのファイル系イベントに相当するもの）だけを使う。SigmaHQのルールとそのまま互換になり、pySigma等の標準ツールでも検証できる。

**logsource**

| 入力 | logsource | 備考 |
|---|---|---|
| USN `FILE_CREATE` | `product: windows` + `category: file_event` | — |
| USN `FILE_DELETE` | `category: file_delete` | — |
| USN `RENAME_OLD_NAME` → `RENAME_NEW_NAME` | `category: file_rename` | 前後の組を1件のイベントにまとめる |
| USN `BASIC_INFO_CHANGE` | `category: file_change` | タイムスタンプ・属性の変更（Sysmon EID 2に相当） |
| MFTエントリ（現在の状態） | `category: file_event` | 「存在するファイル」を、$SIの作成日時に作成されたイベントとして扱う。USNの`FILE_CREATE`と同じファイルに一致した場合は、1つのFindingにまとめる |

USNの`DATA_OVERWRITE` / `DATA_EXTEND`には対応する標準カテゴリがないので、Sigmaの対象外とする（タイムラインには表示する）。

**フィールド**（Sigma標準のみ）

| フィールド | 内容 | 対象カテゴリ |
|---|---|---|
| `TargetFilename` | フルパス。Rewindで解決できなかった場合は解決できた範囲のパス | 全カテゴリ |
| `SourceFilename` | リネーム前のフルパス | `file_rename` |
| `CreationUtcTime` | 作成日時（MFTは$SI、USNはレコードの時刻） | `file_event` / `file_change` |

プロセス系の項目（`Image`、`CommandLine`、`User`等）は$MFT/$UsnJrnlに存在しないので、提供しない。これらを条件に使うルールは一致しない（条件を外して流用はしない）。

**ルールの例**

```yaml
title: Executable Dropped in PerfLogs
logsource:
  product: windows
  category: file_event
detection:
  selection:
    TargetFilename|startswith: 'C:\PerfLogs\'
    TargetFilename|endswith: ['.exe', '.dll', '.ps1', '.bat']
  condition: selection
level: high
```

```yaml
title: Event Log File Deleted
logsource:
  product: windows
  category: file_delete
detection:
  selection:
    TargetFilename|startswith: 'C:\Windows\System32\winevt\Logs\'
    TargetFilename|endswith: '.evtx'
  condition: selection
level: high
```

**1件の照合では書けない検知**（時間的な連鎖）は、Sigmaのcorrelationルールで書く。`group-by`にも標準フィールドだけを使う。

| 検知 | correlationの種類 |
|---|---|
| ランサムウェア（短時間の大量リネーム） | `event_count`（`file_rename`、端末単位、N件/時間） |
| 作成→短時間で削除された実行ファイル | `temporal_ordered`（`group-by: TargetFilename`、作成→削除） |
| SDeleteのパターン（`AAAA.AAA`…の連続リネーム→削除） | `event_count` / `temporal_ordered` |
| 持ち出しの準備（アーカイブの作成→削除） | `temporal_ordered`（`group-by: TargetFilename`） |

ディレクトリ単位の集計（例: 同じディレクトリでのexeとdllの同時作成）は、標準フィールドではgroup-byできないため、v1ではSigmaの対象外とする。

**Sigmaの外で扱う事実**（判定もレベルも付けない。Findingの詳細と、一覧の列・フィルタとして表示するだけ）

| 事実 | 内容 |
|---|---|
| ベースライン | 標準 / 標準外（3.4節） |
| timestompの手がかり | `$SI作成 < $FN作成`、$SIの秒未満がゼロ、$SIの作成日時がOSインストールより前（MFTECmdの`SI<FN`・`uSecZeros`列と同じ考え方の、値の比較結果） |
| Zone.Identifier | ZoneId、HostUrl、ReferrerUrl |
| ADS | 代替データストリームの有無と名前 |
| 削除済み | MFTの使用中フラグがOFF |
| パス解決 | `resolved` / `inferred` / `unknown` |
| 所有者SID | `$Secure:$SDS`から取得（収集した場合） |

これらは「事実の提示」であり、怪しいかどうかの判定はしない。判定はSigmaルールに一致したものだけとする。

**v1の初期ルールセット**
1. SigmaHQの`file_*`ルールのうち、ファイル名だけで判定するもの（約120本）。プロセス系の項目を使うルールは、**条件を外して流用しない**（意味が変わり、誤検知が増えるため）。
2. 自作ルール：[research-abused-paths.md](research-abused-paths.md)の悪用されやすいパス、なりすまし名、アンチフォレンジック（すべて`TargetFilename` / `SourceFilename`だけで書く）。
3. LOLBIN：LOLBASのリストから、「標準外のパスにあるLOLBIN」を検出するルールを生成する。

**YARA（オプション）**
- メインのルール形式には採用しない。YARAはファイルの**中身（バイト列）**を照合する道具で、本ツールの入力（メタデータ）や、解析時に組み立てるフルパスの照合には向かない。時間的な連鎖も書けない。
- 用途は**residentデータ（MFTレコード内に本体があるファイル。1024バイトのレコードで約700バイト以下、4Knで約3.7KB以下）**に限定する。例：1行のWebシェル（China Chopper等）、短い`.bat`/`.ps1`/`.vbs`/`.hta`のステージャー。
- エンジンはRust製の[YARA-X](https://github.com/VirusTotal/yara-x)を組み込む。一致した場合はFindingに`yara:<rule名>`として列挙する（レベルはYARAルールの`meta`で固定値を宣言する）。
- ファイル全体のYARAスキャンが必要な場合は、本ツールで絞り込んだファイルだけを回収し、既存のツールでスキャンする運用とする。

### 3.4 Baseline DB

| 項目 | 提案 |
|---|---|
| 単位 | Windowsのエディション×ビルド（例: Win10 22H2 19045、Win11 23H2 22631 / 24H2 26100、Server 2016/2019/2022/2025）。言語版の差は正規化で吸収するか、言語別に持つ |
| 中身 | 正規化パスの集合と、任意の属性（ファイル種別、想定サイズ範囲、ADSの有無）。**$MFTからはファイルのハッシュが得られない**ので、パスと属性ベースになる |
| 形式 | `fst`のバイナリ＋メタ情報のJSON。zstdで圧縮して配布 |
| 生成（CI/CD） | ① Packerで評価版ISOからVMを構築（Hyper-V/QEMU、セルフホストランナー）<br>② 素の状態 → Windows Update適用後 → 代表的なアプリ（Office、Edge、Chrome、Defenderの更新等）導入後、の段階ごとに`tool collect`<br>③ 複数回の採取の和集合を取り、正規化して`tool baseline build`<br>④ 差分レビュー後にリリース |
| 拡張 | `tool baseline build --from <自社ゴールデンイメージのcollection>`で**自組織ベースライン**を作れるようにする。複数のベースラインを重ねて適用できる |
| 配布 | 別リポジトリで管理し、`tool baseline update`で取得する |

**既知の限界とその対策**

| 限界 | 対策 |
|---|---|
| 標準パスに置かれた同名ファイル（正規ファイルの置き換え）は、パスベースでは見逃す | timestomp・サイズ・作成時期（OSインストールやWindows Updateの時期と乖離していないか）で補う |
| ビルド/UBRの微差によるノイズ | 近いビルドへのフォールバックと、正規化ルールの継続的な改善で対処する |

### 3.5 Viewer（ブラウザ）
- **形態**：自己完結HTML。CSS/JS/データを1ファイルに埋め込み、データはzstdまたはgzipで圧縮したうえでBase64化する。`file://`で開いて動作し、外部通信はしない。
- **技術**：TypeScript＋軽量なフレームワーク（Svelte/Preact）。仮想スクロールのテーブル、タイムラインチャート。Viteと`vite-plugin-singlefile`で1ファイルにビルドする。
- **データ量の考え方**：ベースライン外＋期間内に絞ったデータだけを埋め込む（目標は数万〜数十万行、HTMLで数十MB以下）。全件を見たい場合はCSV/JSONLを別途出力する。
- **画面**
  1. **サマリ**：端末情報、調査期間、主要な発見（「要確認ファイル」「ログ削除の痕跡」等のカード）。
  2. **要確認リスト**：レベル→時刻の順に並べ、レベル・ルール・期間で絞り込めるようにする。各行に、一致したルール名、条件、根拠の値、「なぜ要確認か」の平易な日本語/英語の説明を表示する。
  3. **タイムライン**：期間内のUSNイベントとMFTタイムスタンプの統合表示。スパイクの可視化。
  4. **ツリー差分**：ディレクトリツリー上で、ベースライン外のファイルをハイライトする。
  5. **詳細**：$SI/$FNの全タイムスタンプ、USNのライフサイクル、Zone.Identifier。
- **共有**：フィルタ状態をURLのハッシュに保存し、「この画面を見て」と伝えられるようにする。CSVエクスポート。

### 3.6 将来：WASM版（v2以降の候補）
- Coreを`wasm32`にビルドし、ブラウザに`collection.zip`をドラッグ＆ドロップするだけで解析できるようにする（Web Workerで処理）。
- mft crateのWASM化は、既にMFTparser（mftparser.com）で実証済み。
- 制約：大きな$MFT（GB級）ではメモリが厳しい（Memory64は今後の選択肢）。**まずはクライアント端末規模に限定して提供する**。

---

## 4. CLIインターフェース（案）

```
tool collect  [-v C:] [-o collection.zip]
tool analyze  -i collection.zip|<dir>|<image> [--from 2026-09-01] [--to 2026-09-10]
              [--baseline auto|<path>] [--extra-baseline corp.fst]
              [-o report.html] [--csv timeline.csv] [--jsonl findings.jsonl]
tool baseline update | list | build --from <collections...> -o out.fst
tool rules    update                    # Sigmaルール（自作＋SigmaHQの流用分）と正規化ルール
              [--yara <dir>]            # （オプション）residentデータへのYARA適用
```

- 出力のCSV/JSONLは、HayabusaやTakajoのタイムラインとマージできる列構成にする（時刻、ホスト名、ルール名、レベル、詳細）。Timesketch形式もオプションで出力する。

---

## 5. リポジトリ構成（案）

```
<tool>/                       # Cargo workspace
├── crates/
│   ├── core/                 # model, parse(mft/usn), resolve(rewind), normalize, baseline, fields
│   ├── sigma/                # Sigmaエンジン（検知・correlation。Hayabusaの知見を流用）
│   ├── collector/            # Windows raw volume reader（cfg(windows)）
│   ├── cli/                  # clap。collect/analyze/baseline/rules
│   └── wasm/                 # （v2）wasm-bindgen
├── viewer/                   # TypeScript + Vite（単一HTMLテンプレートを出力し、cliに埋め込む）
├── rules/                    # Sigmaルール・正規化ルール（YAML）※別リポジトリでも可
└── testdata/                 # 小さな合成$MFT/$J、4Knのケース、各OSビルドのサンプル
<tool>-baselines/             # 別リポジトリ：ベースラインDBとCI（Packer定義）
```

---

## 6. 非機能要件と品質

| 観点 | 方針 |
|---|---|
| 性能 | 1GBの$MFT＋32MBの$Jを、一般的なPCで1分以内に解析（目標）。ストリーミング処理とrayonで並列化する |
| 堅牢性 | 不正な入力でもpanicしない（`forbid(unsafe_code)`、cargo-fuzzをCIで実行） |
| 正確性 | MFTECmdとの差分テスト（パース結果）。Rewindのパス解決はusnjrnl_rewindと比較する |
| 検知精度 | 公開データセット（DFIR Madness、DEF CON DFIR CTF、NIST CFReDS等）と自作の攻撃シナリオで再現率と件数削減率を計測し、CIで回帰を監視する |
| プライバシー | ファイル名に個人情報が含まれうる。レポートは外部送信なし。オプションでユーザー名をマスクできるようにする |
| 国際化 | 日本語/英語（理由文はテンプレート化する） |
| ライセンス | OSS（Hayabusa系に合わせる）。GPLv3のコード（dfir_ntfs等）は参照のみにとどめ、取り込まない |

---

## 7. ロードマップ（案）

| フェーズ | 内容 |
|---|---|
| **v0.1（PoC）** | `analyze`：$MFTのパース＋ベースライン差分（Win11 1ビルドのみ）＋CSV出力。ベースラインのノイズ削減率を検証する |
| **v0.2** | $UsnJrnl＋Rewind＋期間フィルタ＋Sigmaエンジン（ファイル名だけで判定するSigmaHQルール＋自作ルール）＋HTMLレポート（サマリ/リスト/タイムライン） |
| **v0.3** | `collect`、ベースラインCIの整備（複数ビルド）、自組織ベースライン |
| **v1.0** | correlationルール、ルールの拡充、ツリー差分、i18n、ドキュメント、データセットによる精度評価の公開 |
| v1.x〜 | オプション（residentデータへのYARA-X）、複数端末の横断比較、WASM版 |

---

## 8. 決めるべき論点

1. **ベースラインの粒度**：ビルド単位か、UBR単位か。言語版の扱い。
2. **ベースライン生成基盤**：評価版ISOのライセンス条件、セルフホストランナーのコスト。GitHub ActionsのWindowsランナーは導入済みソフトが多く、ノイズになる。
3. **レポートへの埋め込みデータの上限**：ファイルサーバー規模の場合の扱い（ページ分割、CSVへの退避）。
4. **Collectorを提供するか**：KAPE/Velociraptorへの依存で十分か。非専門家の体験を優先するなら、提供すべき。
5. **ルールのレベル基準**：Sigmaのレベル（critical/high/medium/low/informational）を、本ツールではどういう基準で付けるかをドキュメント化し、ルールのレビュー基準にする。
6. **MFTエントリを`file_event`として扱うことの是非**：「作成された」と「存在する」は意味が違う。作成日時に$SI（改ざんされやすい）と$FNのどちらを使うかも含めて決める。
7. **Sigmaの外で扱う事実（timestomp等）の見せ方**：判定をしない「事実」として、どこまで一覧の列に出すか。
8. **ツール名**。
