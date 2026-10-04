# NTFSアーティファクト調査レポート（$MFT / $UsnJrnl / $LogFile）

作成日: 2026-10-04（Web調査により更新）

> 注記: 公開情報・研究成果・各ツールのリポジトリをもとにまとめた。出典は各節末と「6. 参考資料」に記載。「要確認」と付記した項目は、情報源の間で記述が食い違う、または一次情報がないため、実機で確認すること。

---

## 0. サマリ

| 項目 | $MFT | $UsnJrnl ($J) | $LogFile |
|---|---|---|---|
| 役割 | ボリューム上の全ファイル/ディレクトリのメタデータ台帳 | ファイル変更の「理由」付き変更履歴 | メタデータ更新のトランザクションログ（クラッシュ復旧用） |
| 公式の公開仕様 | **部分的**（MS DevNotesに一部構造体のみ） | **あり**（USN_RECORD_V2/V3/V4、Reasonコード、FSCTL） | **なし** |
| 記録の性質 | 現在の状態のスナップショット | 時系列のイベント（ファイル名＋理由） | 低レベルのRedo/Undo操作 |
| 既定サイズと保持期間の目安 | 削除エントリは再利用されるまで残る | クライアントは既定32MB（サーバー/DCは512MB以上の例あり）で、数日〜数週間 | 既定64MBで、活動の多いシステムでは約4時間程度という報告あり |
| DFIR上の強み | 全ファイル一覧、MACBタイムスタンプ、timestomp検知、ADS | 削除済みファイルの作成〜削除の履歴、リネーム、実行の痕跡（Prefetch作成）、ランサムウェア | timestomp前の値の復元、直近の細粒度操作 |
| 解析の難易度 | 中 | 低〜中 | 高 |
| OSSの成熟度 | 高 | 高 | 中（2024年以降、Rust実装も登場） |

---

## 1. $MFT（Master File Table）

### 1.1 仕様

**どのようなファイルか**
- NTFSボリュームのメタデータの中心。ボリューム上のすべてのファイル・ディレクトリ（$MFT自身を含む）が、1つ以上の **FILEレコード（File Record Segment, FRS）** として登録される。
- 0〜15番のエントリはシステムファイルとして予約されている。
  - 0: `$MFT` / 1: `$MFTMirr` / 2: `$LogFile` / 3: `$Volume` / 4: `$AttrDef` / 5: `.`（ルート）/ 6: `$Bitmap` / 7: `$Boot` / 8: `$BadClus` / 9: `$Secure` / 10: `$UpCase` / 11: `$Extend`（配下に `$UsnJrnl`, `$ObjId`, `$Quota`, `$Reparse`, `$RmMetadata` など）

**レコード構造**
- 1レコードは通常1024バイト。**4Kセクタ（Advanced Format / 4Kn）のディスクでは4096バイト**になることがある。実際のサイズは`$Boot`の「Bytes per FileRecord Segment」から取得する。
  - 4096バイトのレコードでは、residentデータが**最大約3.7KB**まで格納されうる（1024バイトなら約700〜750バイト以下）。
  - **実装上の落とし穴**：fixup（Update Sequence Array）は、セクタサイズにかかわらず**512バイト単位**で適用される。セクタサイズ単位で適用すると、4Knボリュームで失敗する（実際にRust実装でバグ報告がある）。
  - 4Knでは、商用ツールでさえクラッシュや未認識の事例が報告されている。
- ヘッダ（Microsoft DevNotesの`FILE_RECORD_SEGMENT_HEADER`。MS自身が「NTFS major version 3、minor version 0/1でのみ有効」と明記している）
  - シグネチャ`FILE`（破損時は`BAAD`）
  - Update Sequence Array
  - `$LogFile`のLSN
  - シーケンス番号（再利用のたびに増える）
  - ハードリンク数
  - フラグ（使用中/ディレクトリ）
  - 使用サイズ/割当サイズ
  - ベースレコード参照
- 属性（TLV形式で並ぶ）

| Type | 属性 | DFIR上の主な用途 |
|---|---|---|
| 0x10 | `$STANDARD_INFORMATION` ($SI) | MACBタイムスタンプ、ファイル属性、Owner ID / Security ID、USN |
| 0x20 | `$ATTRIBUTE_LIST` | 属性が複数レコードにまたがる場合 |
| 0x30 | `$FILE_NAME` ($FN) | ファイル名、親ディレクトリ参照、MACBタイムスタンプ（もう1組）。名前空間（Win32/DOS/POSIX）ごとに複数持つことがある |
| 0x40 | `$OBJECT_ID` | Link Tracking（LNK/ジャンプリストと相関可能） |
| 0x50 | `$SECURITY_DESCRIPTOR` | 現行NTFSでは通常`$Secure:$SDS`で集中管理 |
| 0x80 | `$DATA` | ファイル本体。名前付き$DATA＝ADS（例: `Zone.Identifier`） |
| 0x90 / 0xA0 | `$INDEX_ROOT` / `$INDEX_ALLOCATION` | ディレクトリのインデックス（$I30） |
| 0xB0 | `$BITMAP` | インデックス/MFTの割り当て状況 |
| 0xC0 | `$REPARSE_POINT` | シンボリックリンク、ジャンクション、クラウドファイル等 |
| 0x100 | `$LOGGED_UTILITY_STREAM` | EFS等 |

- 属性はresident（レコード内）とnon-resident（data runsで外部クラスタを指す）の2種類。
- タイムスタンプはFILETIME形式（1601-01-01からの100ns単位、UTC）。

**公開仕様の有無**
- Microsoftは**NTFSのオンディスクフォーマット全体の公式仕様を公開していない**。
- 部分的な公開情報（Microsoft Learn / Win32 DevNotes）
  - 「Master File Table (Developer Notes)」
  - `FILE_RECORD_SEGMENT_HEADER`、`ATTRIBUTE_RECORD_HEADER`、`MFT_SEGMENT_REFERENCE`、`FILE_NAME`、`STANDARD_INFORMATION`などの構造体の定義（対応するヘッダファイルはない）
  - `FSCTL_GET_NTFS_VOLUME_DATA`、`FSCTL_GET_NTFS_FILE_RECORD`等のAPI
- **代替となる情報源**（事実上の標準）

| 情報源 | 内容 |
|---|---|
| Brian Carrier『File System Forensic Analysis』(2005) | NTFSのフォレンジック的解説の古典 |
| libyal/libfsntfs ドキュメント（Joachim Metz） | 「New Technologies File System (NTFS)」。最も網羅的で更新が続いている |
| Linux-NTFS Project「NTFS Documentation」 | リバースエンジニアリングによる構造の解説 |
| ntfs-3g / Linuxカーネル ntfs3ドライバのソース | 実装上の挙動の確認 |
| Microsoft Learn DevNotes | 部分的な公式の裏付け |

### 1.2 DFIRでの有用性

1. **ボリューム全体のファイル一覧**：パス、サイズ、属性、タイムスタンプを数分で取得できる。
2. **削除ファイルの検出**：使用中フラグがOFFでも、再利用されていなければレコードは残る。residentなら中身も復元できる。
3. **タイムライン**：$SIと$FNで計8つのタイムスタンプがある。
4. **timestompの検知**
   - $SIは`SetFileTime`等で容易に変更できるが、$FNは通常カーネルしか更新しない。
   - 代表的なヒューリスティック
     - $SIの作成日時が$FNの作成日時より古い（`SI<FN`）
     - $SIの秒未満がゼロ（`uSecZeros`）
     - エントリ番号の順序との矛盾
     - USN/LSNとの矛盾
   - MFTECmdのCSV出力には`SI<FN`、`uSecZeros`等のフラグ列がある（判定はアナリスト任せ）。
5. **ADS**：`Zone.Identifier`（ZoneId=3、HostUrl、ReferrerUrl）からダウンロード元を特定できる。ADSへのペイロード隠蔽も検出できる。
6. **シーケンス番号**：$UsnJrnlの`FileReferenceNumber`との突合や、再利用の検出に必須。
7. **$I30インデックスのスラック**：削除済みファイル名のエントリを回収できることがある。
8. **ベースライン差分との相性**：「標準Windowsに存在しないファイル」を全量から抽出でき、本ツールのコア機能の入力に最適。

### 1.3 制約事項

- **現在の状態しか持たない**（履歴は$UsnJrnl / $LogFileで補う）。
- **削除レコードは再利用で上書きされる**。
- **$SIは改ざんが容易**：$FNも、改ざん後のファイル移動/リネームで$SIの値をコピーさせる手口がある。また、ファイルシステムトンネリング（同名ファイルを短時間で削除→再作成すると作成日時が引き継がれる）による誤検知にも注意が必要。
- **最終アクセス時刻は信頼性が低い**
  - 既定の挙動：XP/2003までは更新が有効、Vista/7以降は無効。
  - Windows 10 1803以降は`NtfsDisableLastAccessUpdate`の値が「System Managed」（`0x80000002` / `0x80000003`）になり、**128GB以下のシステムボリュームでは更新が有効**、それより大きいボリュームでは無効になる。
  - 解析時は必ず実機のレジストリ値（`HKLM\SYSTEM\CurrentControlSet\Control\FileSystem\NtfsDisableLastAccessUpdate`）を確認する。
- **「誰が」「どのプロセスが」は記録されない**。
- **取得にはRawアクセスが必要**（FTK Imager、KAPE、Velociraptor、RawCopy等）。
- **サイズ**：数百MB〜数GB（フルイメージより桁違いに小さい）。
- **パス復元の限界**：親が削除・再利用されているとパスが不完全になる（→ $UsnJrnlを使ったRewindアルゴリズムで改善できる。2.4節参照）。

### 1.4 既存のオープンソース解析ツール

| ツール | 言語 / ライセンス | 概要 |
|---|---|---|
| [omerbenamram/mft](https://github.com/omerbenamram/mft) | Rust / Apache-2.0 or MIT | 100% safe Rust。JSON/JSONL/CSV出力、residentデータの抽出、Pythonバインディング（pymft-rs）。**本ツールの基盤候補** |
| [SecurityRonin/ntfs-forensic](https://github.com/SecurityRonin/ntfs-forensic) | Rust / Apache-2.0 | 独自実装のNTFSリーダー（ntfs-core）と異常監査ツール。timestomp、ADS、削除レコード、MFTスラック、$MFTMirr不一致、ジャーナル消去を、深刻度付きで報告する。`forbid(unsafe_code)`、fuzzing済み。star数は少なく、新しいプロジェクト |
| [ColinFinck/ntfs](https://github.com/ColinFinck/ntfs) | Rust | `no_std`対応のNTFS読み取りライブラリ |
| [EricZimmerman/MFTECmd](https://github.com/EricZimmerman/MFTECmd) | C# / MIT | v0.5.0.1。$MFT / $J / **$LogFile** / $Boot / $SDS / $I30に対応。DFIR界の事実上の標準 |
| [msuhanov/dfir_ntfs](https://github.com/msuhanov/dfir_ntfs) | Python / GPLv3 | v1.1.20。$MFT / $UsnJrnl / $LogFile / $I30 / VSS / FAT / exFAT |
| [rowingdude/analyzeMFT](https://github.com/rowingdude/analyzeMFT) | Python | v3.1.1。メンテナンスが再開され、テスト・CIが整備された |
| [INDXParse](https://github.com/williballenthin/INDXParse) | Python | $I30 / MFTの解析 |
| [The Sleuth Kit](https://github.com/sleuthkit/sleuthkit) | C | `fls` / `istat` / `icat`など |
| [libyal/libfsntfs](https://github.com/libyal/libfsntfs) | C | ライブラリ＋ドキュメント。plasoの基盤 |
| [plaso](https://github.com/log2timeline/plaso) | Python | スーパータイムライン |
| [Velociraptor](https://docs.velociraptor.app/docs/forensic/filesystem/ntfs/) / [go-ntfs](https://github.com/Velocidex/go-ntfs) | Go | `Windows.NTFS.MFT`等。ライブ環境からのRawアクセス、$I30、ADS、timestomp検知 |
| [thewhiteninja/ntfstool](https://github.com/thewhiteninja/ntfstool) | C++ | MFT/USN/LogFile/BitLockerなど多機能 |
| [jschicht/Mft2Csv](https://github.com/jschicht/Mft2Csv) | AutoIt | Joakim SchichtのNTFSツール群 |
| [MFTparser（mftparser.com）](https://www.mftparser.com/) | Rust→WASM | **ブラウザ完結型のMFTビューア**。omerbenamram/mftをWebAssemblyにコンパイルし、Web Workerで実行する。ファイルはアップロードされない。表示は検索・ソート可能な表のみで、解析の示唆はない |

**ギャップ**：大半は「パースしてCSV/JSONで出す」まで。ブラウザ完結型も既に存在するが、ビューアにとどまる。**ベースライン差分による絞り込みと優先順位付け**を行うツールは見当たらない。

---

## 2. $UsnJrnl（Update Sequence Number Change Journal）

### 2.1 仕様

**どのようなファイルか**
- `\$Extend\$UsnJrnl`に存在し、2つのストリームを持つ。
  - **`$J`**：変更レコード本体。スパースファイルで、古い領域は割当デルタ単位で先頭から解放される（ゼロ領域になる）。
  - **`$Max`**：最大サイズ、割当デルタ、Journal ID。
- ファイル/ディレクトリの変更ごとに、Reasonフラグ付きでレコードを追記する。本来の用途は、バックアップ、Windows Search、DFSR/FRSなどの変更検知。
- **既定サイズ**
  - Windows Vista以降のクライアント：最大サイズ32MB（`0x2000000`）
  - Windows Server 2019：32MB（MS Q&Aでの実測）
  - Windows Server 2003：512MB
  - DC/ファイルサーバーでは、DFSR等により512MB以上に拡張されていることが多い
  - 割当デルタ：情報源により4MB/8MBの記述がある（要確認：`fsutil usn queryjournal C:`で実測する）
  - **Microsoftの公式ドキュメントには既定値が明記されていない**

**レコード構造（USN_RECORD_V2）**
- `RecordLength`, `MajorVersion`/`MinorVersion`
- `FileReferenceNumber`（エントリ番号＋シーケンス番号）
- `ParentFileReferenceNumber`
- `Usn`（$J内のオフセット。単調増加）
- `TimeStamp`
- `Reason`
- `SourceInfo`, `SecurityId`, `FileAttributes`
- `FileName`（**ファイル名のみ**）

**バージョン**

| バージョン | 内容 |
|---|---|
| V2 | 従来の形式（64bitファイル参照） |
| V3 | Windows 8 / Server 2012以降。128bitファイルID（ReFSで必須） |
| V4 | Windows 8.1 / Server 2012 R2以降。範囲追跡（`FSCTL_USN_TRACK_MODIFIED_RANGES`）を明示的に有効化した場合のみ。変更範囲（extent）を持つ |

- オンディスクにどのバージョンが記録されるかは、ジャーナルを作成したOSや条件によって変わる。**パーサーはV2/V3/V4の混在を前提にする**。

**主なReasonフラグ**
- `FILE_CREATE` / `FILE_DELETE`
- `RENAME_OLD_NAME` / `RENAME_NEW_NAME`
- `DATA_OVERWRITE` / `DATA_EXTEND` / `DATA_TRUNCATION`
- `BASIC_INFO_CHANGE`（タイムスタンプ・属性の変更）
- `SECURITY_CHANGE`
- `NAMED_DATA_*`（ADS）
- `HARD_LINK_CHANGE`
- `REPARSE_POINT_CHANGE`
- `CLOSE`

ファイルを開いている間のReasonは累積的にORされ、`CLOSE`で確定する。

**公開仕様の有無**
- **3つのアーティファクトの中で唯一、Microsoftが公式に仕様を公開している。**
  - Microsoft Learn（winioctl.h）: `USN_RECORD_V2` / `V3` / `V4`、`USN_RECORD_COMMON_HEADER`、`USN_RECORD_EXTENT`、`USN_JOURNAL_DATA`、Reasonコード
  - `FSCTL_QUERY_USN_JOURNAL` / `FSCTL_READ_USN_JOURNAL` / `FSCTL_ENUM_USN_DATA` / `FSCTL_USN_TRACK_MODIFIED_RANGES`
  - `fsutil usn`コマンドリファレンス
- $Jのオンディスク形式は、上記のレコードがそのまま並び、先頭にスパース領域があるもの。補完情報としてlibyal/libfusnのドキュメントがある。

### 2.2 DFIRでの有用性

1. **ファイルのライフサイクルの追跡**：MFTレコードが再利用された後でも、作成→変更→リネーム→削除の履歴が残る。
2. **実行痕跡の間接証拠**
   - `C:\Windows\Prefetch\*.pf`の作成/更新は、実行の強い示唆になる（Prefetch自体が削除されていても残る）。
   - `.exe` / `.dll` / `.ps1` / `.bat`の作成は、ツール配置の痕跡になる。
3. **ランサムウェアの検知**：大量のリネーム（拡張子変更）や`DATA_OVERWRITE`の集中から、被害の開始時刻とスコープを特定できる。
4. **持ち出しの準備の検知**：アーカイブの作成→削除。
5. **アンチフォレンジックの検知**
   - `.evtx`の削除・切り詰め。**イベントログが消されても、消した事実がUSNに残る**。
   - SDeleteのパターン：ファイルを`AAAA.AAA`〜`ZZZZ.ZZZ`のように連続リネームしてから削除する。
   - ジャーナル自体の削除（`fsutil usn deletejournal /d C:`）
     - Applicationイベントログに**Event ID 3079**が記録される
     - Journal IDが変化する
     - $Jの先頭が新しいUSNから始まる
   - 短時間に集中するリネーム/上書き/削除（ワイパー）
6. **timestompの裏付け**
   - `BASIC_INFO_CHANGE`のレコード時刻と、$SIの値の矛盾を見る。
   - DFRWS 2020の論文（"Artifacts for Detecting Timestamp Manipulation in NTFS on Windows and Their Reliability"）は、$UsnJrnl / LNK / Prefetch / イベントログをtimestomp検知に使う手法と、その信頼性を評価している。
   - 2024〜2025年には、$UsnJrnlの前処理と特徴量抽出による検知手法も発表されている（ScienceDirect, Expert Systems with Applications 2025）。
7. **イベントログが削除されていても機能する**：Hayabusaの補完として最重要。

### 2.3 制約事項

- **保持期間が短い**：クライアントの32MBでは数日〜数週間。侵入初期は既に消えていることが多い（サーバー/DCは512MB以上で比較的長い）。
- **ファイル名のみでフルパスがない**
  - 従来の手法（MFTの親参照＋シーケンス番号）では、**親が再利用されているとUNKNOWNになる**。MFTECmdもこのケースではUNKNOWNを出力する。
  - **CyberCX「Rewind」アルゴリズム**：ジャーナルを**新しい方から逆順に**辿り、各エントリと親の状態を保持しながら巻き戻すことで、再利用済みエントリのパスも復元できる。ジャーナルの末尾が現在のボリューム状態と一致していれば、理論上UNKNOWNはゼロになる。
- **中身や変更内容は記録されない**。
- **主体（ユーザー/プロセス）がわからない**。
- **管理者権限で削除できる**（ただし削除の痕跡は残る。前節参照）。
- **古いレコードの回収**：VSS内の$J、未割当領域、ファイルスラック、pagefile.sys、メモリダンプからカービングできる。
- **ノイズが多い**：Windows Update、Defender、ブラウザキャッシュ、Searchインデクサー等。→ **ベースライン/許可リストによるノイズ除去が本ツールの価値になる**。
- **$Jはスパースファイル**：取得ツールによっては先頭のゼロ領域を含めて巨大になる。先頭ゼロを除去すると軽量化できる。

### 2.4 既存のオープンソース解析ツール

| ツール | 言語 / ライセンス | 概要 |
|---|---|---|
| [SecurityRonin/usnjrnl-forensic](https://github.com/SecurityRonin/usnjrnl-forensic) | Rust / Apache-2.0 | **本ツールのコンセプトに最も近い先行事例**。<br>・E01/ddから$J・$MFT・$LogFile・$MFTMirrを抽出し、4つを相関（QuadLink）<br>・Rewindによる100%のパス解決<br>・カービング、アンチフォレンジック検知、カスタムルール<br>・出力：自己完結型HTMLトリアージレポート（Story/Exploreタブ、「12のIR質問」に回答）、CSV/JSONL/SQLite/body/TLN<br>・15GiBのE01を約4秒で処理<br>・**2026-07-28にアーカイブ（開発終了）**、star 31 |
| [forensicmatt/RustyUsn](https://github.com/forensicmatt/RustyUsn) | Rust | USNパーサー。カービング、MFTを使ったパス解決、JSONL出力 |
| [no1qq/458-JT](https://github.com/no1qq/458-JT) | Rust | USNパーサー＋アンチフォレンジック検知。PCチェック（ゲームのチート調査）用途も想定 |
| [CyberCX-DFIR/usnjrnl_rewind](https://github.com/CyberCX-DFIR/usnjrnl_rewind) | Python | Rewindアルゴリズムの原典。MFTECmdのCSVを入力に、パスを補正したCSV/SQLiteを出力 |
| [EricZimmerman/MFTECmd](https://github.com/EricZimmerman/MFTECmd) | C# / MIT | $Jに対応。$MFTを指定するとパスを解決できる |
| [msuhanov/dfir_ntfs](https://github.com/msuhanov/dfir_ntfs) | Python / GPLv3 | $UsnJrnl対応。$MFTと統合 |
| [PoorBillionaire/USN-Journal-Parser](https://github.com/PoorBillionaire/USN-Journal-Parser) | Python | `usn.py` |
| [jschicht/UsnJrnl2Csv](https://github.com/jschicht/UsnJrnl2Csv) | AutoIt | CSV変換、カービング |
| [The Sleuth Kit](https://github.com/sleuthkit/sleuthkit) `usnjls` | C | USNジャーナルの一覧 |
| [libyal/libfusn](https://github.com/libyal/libfusn) | C | plasoで使用 |
| [Velociraptor `Windows.Forensics.Usn`](https://docs.velociraptor.app/artifact_references/pages/windows.forensics.usn/) | Go | ライブ取得・フィルタ・カービング |
| [thewhiteninja/ntfstool](https://github.com/thewhiteninja/ntfstool) | C++ | USNのダンプ |

---

## 3. $LogFile（NTFSトランザクションログ）

### 3.1 仕様

**どのようなファイルか**
- MFTエントリ2番。NTFSの**Log File Service (LFS)** が使う、メタデータ変更のトランザクションログ。クラッシュ時にRedo/Undoで整合性を回復するためのもの。
- 原則としてユーザーデータは記録しない。ただし、resident属性の更新は記録されるので、小さなファイルの内容が含まれることがある。
- **サイズ**：既定は64MB（65536KB）。`chkdsk C: /L`で確認でき、`/L:size`で変更できる。**循環ログ**で、活動の多いシステムでは**約4時間分**という目安が報告されている。
- **LFSのバージョン**：Windows 7以前は1.1、**Windows 8以降は2.0**。
  - 古いOSで書き込んだボリュームを8以降でマウントすると、2.0に上がることがある。
  - バージョン番号はRestart Pageのオフセット26（minor）と28（major）にある。
  - **パーサーは両バージョンへの対応が必須**。

**構造の概要（リバースエンジニアリングによる）**
- **Restart Area**：先頭の2ページ（`RSTR`、2重化）。現在のLSN、クライアント情報、ページサイズ。
- **Logging Area**：`RCRD`ページの連続（各ページにfixupがある）。
- **ログレコード**
  - `this LSN` / `previous LSN` / `client undo-next LSN`
  - トランザクションID
  - Redo/Undoのオペレーションコード、対象（MFTレコード、属性オフセット、クラスタ）
  - Redo/Undoのデータ本体
- 代表的なオペレーションコード
  - `InitializeFileRecordSegment` / `DeallocateFileRecordSegment`
  - `CreateAttribute` / `DeleteAttribute`
  - `UpdateResidentValue`（$SIの更新など。**変更前と変更後の値が両方残る**）
  - `UpdateNonresidentValue` / `UpdateMappingPairs`
  - `AddIndexEntryRoot` / `AddIndexEntryAllocation` / `DeleteIndexEntry*`
  - `SetBitsInNonresidentBitMap` / `ClearBitsInNonresidentBitMap`
  - `UpdateFileNameRoot` / `UpdateFileNameAllocation`
  - `ForgetTransaction` / `CommitTransaction` / チェックポイント系
- **$LogFileにはUSNレコードが埋め込まれている**ことがある（$UsnJrnl:$Jへの書き込みもログに残る）。$Jが消された場合の回収源になりうる（usnjrnl-forensicがこれを利用している）。

**公開仕様の有無**
- **Microsoftによる公式の公開仕様はない**。
- 代替となる情報源

| 情報源 | 内容 |
|---|---|
| libyal/libfsntfs ドキュメントの$LogFileの章 | 最も体系的な構造記述 |
| Linux-NTFS / ntfs-3gの`logfile.h`、Linuxカーネルの`fs/ntfs/logfile.c` | Restart Area・レコードヘッダ |
| dfir_ntfsのソース（Maxim Suhanov） | 実装による事実上の仕様 |
| Joakim Schicht「LogFileParser」のソースとWiki | オペレーションコードごとの解釈。v2.0.0.0でLFS 2.0に対応 |
| Junghoon Oh「NTFS Log Tracker」の発表資料（FIT 2013等） | $LogFile / $UsnJrnl / $MFTの統合解析の手法 |
| 研究論文 | 「Forensic Detection of Timestamp Manipulation for Digital Forensic Investigation」（2024）：NTFS Log Tracker v1.9の改良版$LogFile検知アルゴリズムは、$SIの改ざんを、トンネリングによる誤検知なしに検出できたと報告 |

### 3.2 DFIRでの有用性

1. **timestompの決定的な証拠**：`UpdateResidentValue`に$SIの改ざん前後の値が残る。2024年の研究でも、$LogFileベースの検知が最も誤検知が少ないと報告されている。
2. **直近の細粒度な操作履歴**：$UsnJrnlより細かいトランザクション単位で追える。
3. **$UsnJrnlの削除/無効化への耐性**：埋め込みUSNレコードの回収や、$J消去後の直近操作の確認。
4. **削除直前の状態の復元**：Undoデータから、削除されたMFTレコードやresidentデータを回収できる。
5. **改ざん検知**：MFTレコードヘッダのLSNと$LogFileを突合し、ログの欠落（gap）を検知できる（ntfs-forensicが実装している）。

### 3.3 制約事項

- **保持期間が非常に短い**（64MBの循環で、活動の多いシステムでは数時間）。インシデント発覚後すぐに取得しないと意味をなさないことが多い。
- **公式仕様がなく解釈が難しい**：LFS 1.1/2.0の差異、オペレーションコードの意味、Redo/Undoデータの解釈。
- **ログレコード自体にタイムスタンプがない**：含まれる$SI値や$UsnJrnl / $MFTとの相関から推定する。
- **単独では意味が取りにくい**：$MFTと組み合わせないとファイル名・パスがわからない。
- **ツール間で結果が一致しないことがある**：複数のツールを参照実装にした差分テストが有効（ntfs-forensicは、LogFileParserとの比較で74,754レコードの不一致ゼロを報告）。
- **誤解釈のリスク**：コミットされなかったトランザクションやチェックポイントの扱い。

### 3.4 既存のオープンソース/フリー解析ツール

| ツール | 言語 / ライセンス | 概要 |
|---|---|---|
| [EricZimmerman/MFTECmd](https://github.com/EricZimmerman/MFTECmd) | C# / MIT | v0.5系で**$LogFileに対応**（以前は「対応予定」だった） |
| [msuhanov/dfir_ntfs](https://github.com/msuhanov/dfir_ntfs) | Python / GPLv3 | $LogFileのパースに対応する、信頼性の高い実装 |
| [jschicht/LogFileParser](https://github.com/jschicht/LogFileParser) | AutoIt | $LogFile専用の詳細パーサー。v2.0.0.0でLFS 2.0に対応。Windows専用 |
| [SecurityRonin/ntfs-forensic](https://github.com/SecurityRonin/ntfs-forensic) | Rust / Apache-2.0 | RCRDページのfixup、Redo/Undoのデコード、操作分類、トランザクション再構成、LSNのgap検知。LogFileParserとの差分検証済み |
| [SecurityRonin/usnjrnl-forensic](https://github.com/SecurityRonin/usnjrnl-forensic) | Rust / Apache-2.0 | $LogFileからの埋め込みUSN回収と相関（アーカイブ済み） |
| [thewhiteninja/ntfstool](https://github.com/thewhiteninja/ntfstool) | C++ | `logfile`コマンドでダンプ |
| [Velociraptor](https://docs.velociraptor.app/docs/forensic/filesystem/ntfs/) | Go | VQLのNTFSパーサー関数で$LogFileにアクセスできる |
| ntfs-3g（`ntfsrecover`等） | C | リカバリ目的 |
| [NTFS Log Tracker](https://sites.google.com/site/forensicnote/ntfs-log-tracker)（Junghoon Oh） | フリーウェア（**OSSではない**） | $LogFile / $UsnJrnl / $MFTの統合解析、LFS 2.0対応。VSS、pagefile、未割当領域からのUSNカービング。Windows GUI |

**更新点**：前版では「Rust製の成熟した$LogFileパーサーは見当たらない」としていたが、**ntfs-forensic（Rust、Apache-2.0）が存在する**。ただし開発者・利用者ともに少ない若いプロジェクトなので、依存先として使うか参照実装にとどめるかは評価が必要。

---

## 4. 競合・先行事例の整理（本ツールとの差分）

| 観点 | 本ツール（構想） | usnjrnl-forensic | MFTparser（ブラウザ） | MFTECmd | ntfs-forensic |
|---|---|---|---|---|---|
| 言語 | Rust | Rust | Rust→WASM | C# | Rust |
| ブラウザで閲覧 | ◎ | △（静的HTMLレポート） | ◎（ビューアのみ） | × | × |
| $MFT / $J / $LogFile | ○ / ○ / ○ | ○ / ○ / ○ | ○ / × / × | ○ / ○ / ○ | ○ / ○ / ○ |
| Rewindによるパス解決 | 採用推奨 | ○ | × | × | ○ |
| timestomp・アンチフォレンジック検知 | ○ | ○ | × | 列のみ | ○ |
| **Windows標準ベースラインとの差分** | **◎（コア）** | × | × | × | × |
| 期間フィルタ | ◎ | ○ | △ | × | × |
| 開発状況 | — | **アーカイブ済み** | 稼働中 | 活発 | 若い |

**示唆**
- 「Rustで高速に3つのアーティファクトを相関し、HTMLレポートを出す」という形は既に試されている（usnjrnl-forensic）。ただしアーカイブされており、**ベースライン差分という切り口はどのツールにもない**。差別化の軸として妥当。
- ブラウザ完結（WASM）は、mft crateで実現可能なことが実証済み。「データをアップロードしない」点は、非専門家や機密性の高い環境への訴求になる。
- Rewindアルゴリズムは事実上の標準になりつつあり、採用すべき。
- ライセンス：mft crate、ntfs-forensic、usnjrnl-forensicはApache-2.0/MIT系で再利用しやすい。dfir_ntfsはGPLv3なので、コードの流用は避けて参照のみにする。

---

## 5. 3つのアーティファクトの相関（本ツールへの示唆）

```
            時間軸（過去 ← → 現在）
$MFT      :                                   [ 現在の状態 + 削除済み未再利用エントリ ]
$UsnJrnl  :              [------ 数日〜数週間の変更履歴 ------]
$LogFile  :                                          [-- 数時間 --]
```

| 相関キー | 内容 |
|---|---|
| MFTエントリ番号＋シーケンス番号 | $UsnJrnlと$MFTの突合 |
| 親参照＋Rewind | $UsnJrnlのファイル名をフルパスに復元（再利用済みエントリを含む） |
| USN | $SIにある最終USN。単調増加性による時系列検証 |
| LSN | MFTレコードヘッダのLSNと$LogFileの突合、gap検知 |

**本ツールのコンセプトから見た位置づけ**
- **$MFT**：ベースライン差分の主対象。timestomp判定、ADS（Zone.Identifier）。
- **$UsnJrnl**：期間の絞り込みの主対象。ベースラインでノイズを除去する。Rewindでパスを解決する。
- **$LogFile**：補助的。timestompの確定、$J消去時の補完。フェーズを分けて実装するのが妥当（ntfs-forensicの評価を先に行う）。

**実装上の推奨**
- $MFT：`mft` crateを第一候補とする。4Kn（4096バイトのレコード、512バイト単位のfixup）のテストデータを必ずCIに含める。
- $UsnJrnl：仕様が公開されているので自前実装でも容易。V2/V3/V4の混在に対応し、Rewindを実装する。
- $LogFile：LFS 1.1/2.0の両対応。dfir_ntfs、LogFileParser、ntfs-forensicを参照実装とした差分テストをCIに組み込む。
- パス解決：Rewindでも解決できない場合は、「推定」「不明」を明示する。
- テストデータ候補：DFIR Madness、DEF CON DFIR CTF、NIST CFReDS（Hacking Case）等の公開イメージ。

---

## 6. 参考資料

### 公式（Microsoft）
- [Master File Table (Developer Notes)](https://learn.microsoft.com/en-us/windows/win32/devnotes/master-file-table)
- [FILE_RECORD_SEGMENT_HEADER](https://learn.microsoft.com/en-us/windows/win32/devnotes/file-record-segment-header)
- [USN_RECORD_V2](https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ns-winioctl-usn_record_v2) / [V3](https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ns-winioctl-usn_record_v3) / [V4](https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ns-winioctl-usn_record_v4) / [USN_RECORD_COMMON_HEADER](https://learn.microsoft.com/en-us/windows/win32/api/WinIoCtl/ns-winioctl-usn_record_common_header) / [USN_TRACK_MODIFIED_RANGES](https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ns-winioctl-usn_track_modified_ranges)
- [fsutil usn](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/fsutil-usn)
- [Default Journal Size of Windows Server 2019 (Microsoft Q&A)](https://learn.microsoft.com/en-us/answers/questions/380868/default-journal-size-of-windows-server-2019)

### 解説・研究
- Brian Carrier, *File System Forensic Analysis*, Addison-Wesley, 2005
- [libyal/libfsntfs documentation](https://github.com/libyal/libfsntfs)
- [CyberCX: NTFS Usnjrnl Rewind](https://cybercx.com.au/blog/ntfs-usnjrnl-rewind/)
- [Artifacts for Detecting Timestamp Manipulation in NTFS on Windows and Their Reliability (DFRWS / FSI: Digital Investigation, 2020)](https://www.sciencedirect.com/science/article/pii/S2666281720300159)
- [Forensic Detection of Timestamp Manipulation for Digital Forensic Investigation (2024)](https://www.researchgate.net/publication/380284178_Forensic_Detection_of_Timestamp_Manipulation_for_Digital_Forensic_Investigation)
- [A practical approach to detecting file timestamp manipulation for digital forensic investigations (2025)](https://www.sciencedirect.com/science/article/abs/pii/S0957417425022493)
- [NTFS Forensics: The USN Change Journal (Digital Investigator, 2026)](https://digitalinvestigator.blogspot.com/2026/05/ntfs-forensics-usn-change-journal.html)
- [Ntfs Last Access Update rules by Windows version (gist)](https://gist.github.com/jipegit/4f6602456f0c2fe256642cecee09b425)
- [Enable or Disable NTFS Last Access Time Stamp Updates in Windows 10 (TenForums)](https://www.tenforums.com/tutorials/139015-enable-disable-ntfs-last-access-time-stamp-updates-windows-10-a.html)
- [4096 byte sector drives, NTFS and forensic tools (Writeblocked)](https://writeblocked.org/2Blog/4k_sectors.html)
- [A Quick Look at $MFT Resident Data on Advanced Format Disks](http://traceevidence.blogspot.com/2013/03/a-quick-look-at-mft-resident-data-on.html)
- [A $LogFile parser utility for NTFS (Forensic Focus)](https://www.forensicfocus.com/forums/general/a-logfile-parser-utility-for-ntfs/)
- [NTFS $LogFile (Count Upon Security)](https://countuponsecurity.com/tag/ntfs-logfile/)
- [Splunk: Detection: USN Journal Deletion](https://research.splunk.com/endpoint/b6e0ff70-b122-4227-9368-4cf322ab43c3/)
- [Elastic: Delete Volume USN Journal with Fsutil](https://www.elastic.co/guide/en/security/8.19/delete-volume-usn-journal-with-fsutil.html)
- [MFT parsers that actually hold up (mftparser.com)](https://www.mftparser.com/en/blog/mft-parser-tools)

### ツール
- 1.4節、2.4節、3.4節の表にあるリンクを参照
