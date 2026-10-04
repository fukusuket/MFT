# Windowsで悪用されやすいファイルパスと調査優先度

作成日: 2026-10-04

## 0. 目的

本資料は、`$MFT` と `$UsnJrnl:$J` を用いたWindows端末のトリアージにおいて、ファイルパスを調査優先度付けの一要素として利用するための調査結果をまとめたものである。

対象は主にWindows 10、Windows 11および近年のWindows Serverとする。記載するパスは既定値の例であり、システムドライブ、ユーザープロファイル、Known Folderのリダイレクト、製品構成、NTFS ACLによって実際の場所や権限は変わる。

## 1. 結論

パスだけで悪性判定してはならない。優先度は、少なくとも次の要素を組み合わせて決めるべきである。

1. **誰が書き込める場所か**：標準ユーザー、管理者、SYSTEM、特定サービス
2. **その場所で自然なファイル種別か**：キャッシュ、設定、実行形式、スクリプトなど
3. **実行や永続化につながる場所か**：Startup、Scheduled Tasks、サービス配下など
4. **ファイル活動が不自然か**：短時間での作成・リネーム・削除、ADS、属性変更
5. **正常性を裏付ける情報があるか**：Windowsベースライン、既知製品、署名、ハッシュ
6. **パスの確度が十分か**：MFT親参照とシーケンス番号を検証できているか

優先度付けでは、次の2種類を区別する。

- **利用しやすい場所**：標準ユーザーが書き込めるため、初期侵入後のペイロード配置に使いやすい
- **侵害を強く示唆する場所**：通常は管理者等しか変更できず、未知の変更があれば重大だが、Windows Updateや正規インストーラーによるノイズも多い

## 2. 権限に関する前提

### 2.1 標準ユーザーが通常書き込める範囲

WindowsのKnown Folderでは、ユーザー単位の代表的な場所として次が定義されている。

- `%USERPROFILE%`：通常は `C:\Users\<user>`
- `%APPDATA%`：通常は `C:\Users\<user>\AppData\Roaming`
- `%LOCALAPPDATA%`：通常は `C:\Users\<user>\AppData\Local`
- `%TEMP%` / `%TMP%`：通常は `C:\Users\<user>\AppData\Local\Temp`
- ユーザーのDesktop、Downloads、Documents

Microsoftは、`LOCALAPPDATA`をユーザー固有のアプリケーションデータ領域、`TEMP`を通常 `%USERPROFILE%\AppData\Local\Temp` と説明している。また、一般的な標準ユーザーは自分のDocumentsやLocal AppDataへ読み書きできる。[Microsoft: KNOWNFOLDERID](https://learn.microsoft.com/en-us/windows/win32/shell/knownfolderid)、[Microsoft: Recognized environment variables](https://learn.microsoft.com/en-us/windows/deployment/usmt/usmt-recognized-environment-variables)、[Microsoft: User Account Control for Game Developers](https://learn.microsoft.com/en-us/windows/win32/dxtecharts/user-account-control-for-game-developers)

これらの場所に攻撃者がファイルを作成できることは、それ自体では権限昇格を意味しない。フィッシング文書、ブラウザダウンロード、正規アプリ、アップデーターも大量のファイルを生成する。

### 2.2 通常は保護される範囲

代表的な保護領域は次のとおりである。

- `%WINDIR%`（通常 `C:\Windows`。ただし例外あり）
- `%WINDIR%\System32`
- `%WINDIR%\SysWOW64`
- `%WINDIR%\System32\drivers`
- `%ProgramFiles%`
- `%ProgramFiles(x86)%`
- 全ユーザー用Startup
- Scheduled Tasksの保存領域

Microsoftの権限例では、標準ユーザーに対するProgram Filesは読み取り専用として扱われる。したがって、これらの場所に未知の実行形式が新規作成された場合、管理者権限の使用、脆弱なACL、特権サービス、正規インストーラーのいずれかを疑うべきである。[Microsoft: User Account Control for Game Developers](https://learn.microsoft.com/en-us/windows/win32/dxtecharts/user-account-control-for-game-developers)

ただし `%WINDIR%` 配下のすべてが書き込み禁止ではない。Microsoftは `%WINDIR%\Temp` にUsersグループのファイル・ディレクトリ作成権限があることを、AppLockerのパスルール上の注意点として明示している。[Microsoft: Understanding AppLocker default rules](https://learn.microsoft.com/en-us/windows/security/application-security/app-control-for-business/applocker/understanding-applocker-default-rules)

### 2.3 `ProgramData`は一律に「全ユーザー書き込み可」ではない

`%ProgramData%` は通常 `C:\ProgramData` であり、アプリケーションが標準ユーザー向けデータを置く場所でもある。Microsoftの資料では、標準ユーザーは作成した項目の所有者として読み書きできる一方、既存のすべての配下を自由に変更できるわけではない。[Microsoft: ProgramData](https://learn.microsoft.com/en-us/windows-hardware/customize/desktop/unattend/microsoft-windows-shell-setup-folderlocations-programdata)、[Microsoft: User Account Control for Game Developers](https://learn.microsoft.com/en-us/windows/win32/dxtecharts/user-account-control-for-game-developers)

したがって、`ProgramData`配下という理由だけで標準ユーザーによる作成と断定してはならない。新規サブディレクトリ、作成者、継承ACL、対象製品の通常動作を確認する必要がある。

### 2.4 MFTだけでは実効書き込み権限を確定できない

MFTの`$STANDARD_INFORMATION`にはSecurity IDがあるが、これはNTFSの`$Secure`内にあるセキュリティ記述子への参照である。`$MFT`と`$J`だけを入力とする本ツールでは、通常、DACLと実効アクセス権を完全には復元できない。

そのためUIでは、次を区別すべきである。

- `default-writable`：Windows既定では標準ユーザーが書き込めると期待される
- `default-protected`：Windows既定では昇格権限等が必要と期待される
- `conditional`：配下のACL、所有者、アプリケーション構成に依存する
- `observed-acl`：別途取得したACLから実際の権限を確認済み
- `unknown`：判断材料がない

将来、収集ツールが対象パスのACL情報をJSON等で同梱できる場合は、既定値より観測値を優先する。`$Secure:$SDS`の解析はコア機能にせず、必要性を検証してから追加する。

## 3. 優先的に調べるパス

以下の優先度は「そのパスにファイルが存在するだけ」の重大度ではなく、未知の実行形式や不自然な活動が見つかった場合の調査開始順位である。

### 3.1 Priority A：実行・永続化に直結する場所

| パス | 通常の権限・主体 | リスク | 優先する条件 |
|---|---|---|---|
| `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup` | 当該標準ユーザーが書き込み可能 | ログオン時にユーザー権限で実行される | 新規EXE、DLL、LNK、BAT、CMD、PS1、VBS、JS、HTA、または短期間での作成・削除 |
| `%ProgramData%\Microsoft\Windows\Start Menu\Programs\Startup` | 通常は管理者等による変更 | 全ユーザーのログオン時に実行され得る | ベースライン外の新規ファイル、特にスクリプトや不明なLNK |
| `%WINDIR%\System32\Tasks` | 通常は管理者／Task Scheduler | タスクによる任意の時刻・権限での実行 | 新規・変更されたタスクファイル、短命なタスク、既知のMicrosoftタスク階層を模倣した名前 |
| `%WINDIR%\Tasks` | レガシータスク領域 | 古い形式のスケジュール実行 | `.job`の新規作成・変更、現行端末での未知の項目 |

現在ユーザーと全ユーザーのStartupパスはMicrosoftのKnown Folder資料に示されている。MITRE ATT&CKも、攻撃者がStartup Folderを永続化や権限昇格に利用し得るとしている。[Microsoft: Recognized environment variables](https://learn.microsoft.com/en-us/windows/deployment/usmt/usmt-recognized-environment-variables)、[MITRE ATT&CK T1547.001](https://attack.mitre.org/techniques/T1547/001/)

Scheduled Tasksのファイルは `%WINDIR%\System32\Tasks` に保存される。MFTだけではタスクの実行ユーザー、Trigger、Actionを理解できないため、このパスで新規ファイルを見つけたら、RegistryのTaskCacheおよびTask Schedulerログの追加取得を推奨する。[Microsoft: Task Scheduler troubleshooting](https://learn.microsoft.com/en-us/troubleshoot/windows-client/system-management-components/task-schedular-service-is-not-available)

### 3.2 Priority A：保護領域にある未知の実行コード

| パス | 通常の権限・主体 | リスク | 優先する条件 |
|---|---|---|---|
| `%WINDIR%\System32` | TrustedInstaller、SYSTEM、管理者、正規更新 | OSバイナリの偽装・置換、DLLロード | ベースライン外のPE、既存システムファイルに酷似した名前、短期間での置換 |
| `%WINDIR%\SysWOW64` | 同上 | 32bitシステムバイナリの偽装・置換 | System32と同様。アーキテクチャ差を考慮する |
| `%WINDIR%\System32\drivers` | 管理者以上、正規ドライバー導入 | カーネルドライバー、永続化 | 新規SYS、既知ドライバーと似た名称、作成後の削除 |
| `%ProgramFiles%` / `%ProgramFiles(x86)%` | 管理者、インストーラー、更新サービス | DLLサイドローディング、アプリ改変 | 正規EXEと同時期に置かれた未知DLL、既存DLLの置換、新規スクリプト |

これらの変更は高い権限やACL不備を示唆する一方、Windows Update、ドライバー更新、ソフトウェアインストールによる正常変更も多い。期間内に大量の正規更新がある場合は、更新クラスタとして抑制する。

MITRE ATT&CKは、正規アプリケーションと悪性DLLを同一ディレクトリに配置してロードさせるDLL side-loadingや、正規DLLの置換を説明している。したがって、Program Files内ではDLL単体ではなく、同じ親ディレクトリに作成されたEXE・DLLの組と作成順序を評価する。[MITRE ATT&CK T1574.001](https://attack.mitre.org/techniques/T1574/001/)

### 3.3 Priority B：標準ユーザーが利用しやすい実行コード配置場所

| パス | 通常の権限・主体 | 正常ノイズ | 優先する条件 |
|---|---|---:|---|
| `%LOCALAPPDATA%\Temp` | 当該ユーザーが書き込み可能 | 非常に多い | EXE/DLL/スクリプト、作成直後の削除、ランダム名、Prefetchとの相関 |
| `%LOCALAPPDATA%` | 当該ユーザーが書き込み可能 | 多い | アプリの通常階層外のEXE/DLL、偽装ディレクトリ、Startup等から参照される候補 |
| `%APPDATA%` | 当該ユーザーが書き込み可能、ローミング対象になり得る | 多い | EXE/DLL/スクリプト、Microsoft風の偽装名、長期残存する未知ファイル |
| `%USERPROFILE%\Downloads` | 当該ユーザーが書き込み可能 | 非常に多い | EXE/MSI/ISO/LNK/スクリプト、Zone.Identifier、後続の移動・削除 |
| `%USERPROFILE%\Desktop` | 当該ユーザーが書き込み可能 | 多い | LNK、スクリプト、二重拡張子、短命な実行形式 |
| `%USERPROFILE%\Documents` | 当該ユーザーが書き込み可能 | 多い | マクロ文書、LNK、ISO、スクリプト、アーカイブ生成 |

MITRE ATT&CKは、ソフトウェア制限ポリシーで `%AppData%` や `%Temp%` からのEXE、BAT、MSI、JS、VBS等の実行を制限する例を示しており、これらがユーザー書き込み可能な実行元として重要であることを裏付ける。[MITRE ATT&CK M1033](https://attack.mitre.org/mitigations/M1033/)

ただしAppDataやTempの全ファイルを高優先度にすると実用にならない。以下を満たす場合のみ強く加点する。

- 実行可能・スクリプト系の拡張子
- PEヘッダーと拡張子の不一致
- 作成後すぐにリネームまたは削除
- 同名または関連名のPrefetch作成・更新
- ベースラインや既知アプリのディレクトリ構造に一致しない
- ADS作成を示す`NAMED_DATA_*`
- 永続化場所または設定からの参照が別アーティファクトで確認された

### 3.4 Priority B：共有・隠蔽・ステージングに使われやすい場所

| パス | 通常の権限・主体 | リスク | 優先する条件 |
|---|---|---|---|
| `%ProgramData%` | 条件付き。作成項目の所有者、管理者、サービス等 | 目立ちにくい全ユーザー向け配置、永続化ペイロード | 新規トップレベル階層、実行形式、スクリプト、Microsoft製品を模倣する名称 |
| `%PUBLIC%`配下 | 複数ユーザーで共有。実効ACLを要確認 | ユーザー間の受け渡し、共有ステージング | Public Documents/Desktop等の未知EXE、LNK、スクリプト、アーカイブ |
| `%WINDIR%\Temp` | Usersが項目を作成可能 | Windows配下という見た目、弱いパス許可ルールの悪用 | EXE/DLL/スクリプト、ダンプ、アーカイブ、列挙結果、短命ファイル |
| `$Recycle.Bin\<SID>` | ユーザーごと、システム管理 | 隠蔽、ステージング、削除に見せかけた保存 | 通常の`$I`/`$R`形式から外れる実行形式・アーカイブ、直接作成された不自然な名称 |
| ドライブ直下または未知の隠しディレクトリ | 実効ACL依存 | ツール・収集物の簡易配置 | 新規の隠し属性ディレクトリ、短い名称、実行形式と出力ファイルの組 |

`%WINDIR%\Temp`は「Windows配下だから信頼できる」というパスルールと、標準ユーザーがファイルを作成できるという性質が衝突するため、特に重要である。[Microsoft: Understanding AppLocker default rules](https://learn.microsoft.com/en-us/windows/security/application-security/app-control-for-business/applocker/understanding-applocker-default-rules)

MITRE ATT&CKには、`C:\Windows\Temp`を出力、ダンプ、除外パス悪用に用いる検出例や、`ProgramData`にペイロードを置いた実例が掲載されている。[MITRE ATT&CK DET0051](https://attack.mitre.org/detectionstrategies/DET0051/)、[MITRE ATT&CK DET0363](https://attack.mitre.org/detectionstrategies/DET0363/)、[MITRE ATT&CK: Rifdoor](https://attack.mitre.org/software/S0433/)

### 3.5 Priority C：ノイズが多いが相関材料になる場所

| パス | 主な正常用途 | 調査上の使い方 |
|---|---|---|
| ブラウザキャッシュ、`INetCache`等 | Webコンテンツ、ダウンロード途中データ | 単独では低優先。実行形式へのリネーム・移動を追跡する |
| `%LOCALAPPDATA%\Packages` | Microsoft Store/MSIXアプリデータ | パッケージ構造を考慮。任意の差分だけでは加点しない |
| `%LOCALAPPDATA%\Microsoft` | WindowsとMicrosoft製品のユーザーデータ | ベースラインと製品知識なしでは低優先。実行コードと短命活動のみ加点 |
| `Recent`、Jump List関連 | ユーザー操作履歴 | ファイル実行・アクセスの補助証拠として利用する |
| Prefetch | プログラム実行の補助証拠 | 対応するEXEが既に削除されている場合に強く加点する |

キャッシュやパッケージ領域はファイル数と変更頻度が大きい。最初から列挙対象外にするのではなく、Finding生成時の既定優先度を下げ、他の証拠との相関で昇格させる。

## 4. 注目するファイル種別

### 4.1 実行可能コード

- `.exe`, `.com`, `.scr`, `.cpl`
- `.dll`, `.ocx`
- `.sys`
- `.msi`, `.msp`, `.msix`, `.appx`

### 4.2 スクリプト・実行設定

- `.ps1`, `.psm1`, `.psd1`
- `.bat`, `.cmd`
- `.vbs`, `.vbe`
- `.js`, `.jse`
- `.wsf`, `.wsh`
- `.hta`, `.sct`
- `.inf`
- `.lnk`, `.url`
- 拡張子のないScheduled Taskファイル

### 4.3 配送・ステージング

- `.zip`, `.7z`, `.rar`, `.cab`
- `.iso`, `.img`, `.vhd`, `.vhdx`
- `.docm`, `.xlsm`, `.pptm`
- `.one`
- `.dmp`, `.dump`, `.bin`, `.dat`

拡張子は表示名にすぎないため、可能ならresident `$DATA` または別途取得したファイル内容のmagic bytesと比較する。内容を取得できない場合は「スクリプトに見える」「PEである」と断定しない。

## 5. パスと活動を組み合わせた優先度付け

### 5.1 推奨する独立したスコア軸

単一の総合スコアだけでなく、UI内部では次の軸を分離して保持する。

| 軸 | 意味 | 例 |
|---|---|---|
| `writability` | 攻撃者が現在の権限で配置しやすいか | ユーザーTempは高、System32は低 |
| `execution` | その場所や種類が実行につながるか | Startup、Tasks、EXE、スクリプト |
| `privilege-impact` | 変更が高い権限を示唆するか | drivers、System32、全ユーザーStartup |
| `stealth` | 隠蔽や偽装に適するか | ProgramData、Recycle Bin、ADS、偽装名 |
| `temporal-anomaly` | 活動列が不自然か | 作成後すぐ削除、大量リネーム |
| `baseline-deviation` | 正常状態から外れるか | ビルド別ベースライン外、属性不一致 |
| `evidence-confidence` | 証拠の確からしさ | 正確なパス、MFT/USN双方で確認 |

`writability`が高いことは「悪性度が高い」のではなく「標準ユーザーでも配置可能」を意味する。System32は`writability`が低い一方、未知の変更に対する`privilege-impact`は高い。

### 5.2 ルール例

#### 高優先度

- ユーザーStartupに新規LNK、EXEまたはスクリプト
- 全ユーザーStartupにベースライン外ファイル
- System32、SysWOW64、driversにベースライン外のPEまたはSYS
- System32 Tasksに指定期間内に作成された未知タスク
- Program Files内で、正規EXEと同時期に作成された未知DLL
- ユーザー書き込み可能場所のEXE作成と、対応するPrefetch作成、その後のEXE削除
- Windows Tempに作られたダンプまたは実行コードが短時間で削除

#### 中優先度

- AppDataまたはProgramDataの未知EXE/DLL/スクリプト
- DownloadsからAppDataへの実行形式のリネーム・移動
- Public配下の実行形式またはアーカイブ
- Recycle Bin内の通常命名規則から外れるファイル
- ユーザー書き込み可能場所のADS作成

#### 低優先度から相関で昇格

- Temp内の単独ファイル
- ブラウザキャッシュ内の実行形式らしい名前
- AppData内の正規アプリ階層にあるDLL
- ProgramData内のログ、データベース、設定ファイル

### 5.3 例示スコア

以下は初期実装用の仮値であり、正解付きコーパスで調整する。

| 条件 | 点数例 |
|---|---:|
| StartupまたはScheduled Tasksへの新規項目 | +40 |
| 保護領域のベースライン外実行コード | +35 |
| ユーザー書き込み可能場所の実行コード・スクリプト | +15 |
| 作成後10分以内に削除 | +20 |
| リネーム前後で拡張子が実行形式へ変化 | +15 |
| MFTとUSNの双方が活動を裏付ける | +10 |
| 対応するPrefetch活動がある | +20 |
| ADSまたは`NAMED_DATA_*` | +15 |
| Windowsビルド別ベースライン外 | +10 |
| 既知の正規更新クラスタと一致 | -25 |
| 信頼済み製品の既知パス・ファイルと一致 | -20 |
| パスが推定のみ | -10 |
| 親のシーケンス不一致でパス不明 | -20 |

点数は悪性確率ではなく、確認順序を決めるための値とする。重大度と証拠確度は別フィールドで表示する。

## 6. MFT・USNで実装可能な検出

### MFTから得られる信号

- 現在または削除済みのパス候補
- ファイル名と拡張子
- ディレクトリ、使用中、削除済み等の状態
- `$SI` / `$FN`タイムスタンプ
- resident `$DATA`の有無と、取得できる場合の先頭バイト
- 名前付き`$DATA`、すなわちADS
- ファイル属性（Hidden、System等）
- MFTエントリ番号、シーケンス番号、親参照

### USNから得られる信号

- `FILE_CREATE`、`FILE_DELETE`
- `RENAME_OLD_NAME`、`RENAME_NEW_NAME`
- `DATA_OVERWRITE`、`DATA_EXTEND`、`DATA_TRUNCATION`
- `BASIC_INFO_CHANGE`
- `NAMED_DATA_*`
- USNとタイムスタンプによる活動順序

### MFT・USNだけでは原則分からないこと

- ファイルを作成・実行したプロセス
- 実行したユーザー
- ファイルが実際に実行されたことの確定
- デジタル署名の有効性（ファイル内容を別途取得できない場合）
- 現在または作成時点の完全なACL
- Registry Run Key、サービス、Scheduled TaskのActionとの参照関係
- ファイル内容がマルウェアであること

UIとレポートでは、「実行された」ではなく「Prefetch活動から実行が示唆される」、「標準ユーザーが書き込める既定パス」など、証拠の範囲に合わせた表現を用いる。

## 7. 実装への提案

### 7.1 ハードコードではなくパスカテゴリとして管理する

```yaml
id: windows.user_temp
scope: per_user
patterns:
  - "C:/Users/*/AppData/Local/Temp/**"
default_access: standard_user_writable
normal_noise: very_high
capabilities:
  - payload_staging
  - transient_execution
base_priority: low
raise_when:
  - executable_or_script
  - short_lived
  - prefetch_correlated
```

実際のドライブ文字、ユーザープロファイル位置、Known Folderリダイレクトを吸収するため、入力パスを正規化し、環境変数相当の論理パスへ変換してから評価する。

### 7.2 環境依存の許可・除外を持たせる

- 組織固有のソフトウェア配置先
- EDR・バックアップ・管理ツールの作業ディレクトリ
- AV/EDR除外パス
- 開発端末のビルド出力
- VDI、キオスク、サーバーロール固有パス
- ソフトウェア配布システムのキャッシュ

特にAV/EDR除外パスは一般化できない。利用者が環境固有の除外設定をインポートした場合、その中での新規実行コードを加点する。MITRE ATT&CKも、セキュリティ製品の除外パスに対するファイル作成・移動を監視対象としている。[MITRE ATT&CK DET0051](https://attack.mitre.org/detectionstrategies/DET0051/)

### 7.3 Findingの表示例

> **High / Confidence: Medium — ユーザーStartupに新規スクリプト**
>
> `C:\Users\alice\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Startup\UpdateCheck.vbs`
>
> 事実：指定期間内にFILE_CREATEを確認。MFTにも同一Identityのファイルが存在する。
>
> 解釈：このフォルダーの項目はユーザーログオン時に実行され得る。当該ユーザーが既定で書き込み可能なため、管理者権限なしでも配置できる。
>
> 制約：NTFSアーティファクトだけでは、スクリプトの内容、作成プロセス、実行の有無は確定できない。
>
> 推奨：ファイル内容、Run Key、PowerShell・Script Hostログ、Prefetch、EDRテレメトリを確認する。

## 8. 誤検知を避けるための注意事項

- AppData、Temp、ProgramDataにあるという理由だけで不審判定しない
- Windows配下にあるという理由だけで信頼しない
- ProgramData全体を標準ユーザー書き込み可能とみなさない
- MFTのSecurity IDだけからユーザーやACLを断定しない
- Windows Updateや正規インストールの大量変更を個別Findingに分解しすぎない
- Prefetchファイルの存在だけで悪性・実行ユーザー・実行回数を断定しない
- 削除済み親レコードや再利用済みMFTエントリから復元したパスを確定表示しない
- ファイル拡張子だけから内容を断定しない
- Known Folderはリダイレクトされ得るため、固定の`C:\Users`だけに依存しない
- OneDrive等による同期・Files On-Demand、MSIXのリダイレクトを正常差分として考慮する

## 9. 参考資料

- [Microsoft: KNOWNFOLDERID](https://learn.microsoft.com/en-us/windows/win32/shell/knownfolderid)
- [Microsoft: Recognized environment variables](https://learn.microsoft.com/en-us/windows/deployment/usmt/usmt-recognized-environment-variables)
- [Microsoft: User Account Control for Game Developers](https://learn.microsoft.com/en-us/windows/win32/dxtecharts/user-account-control-for-game-developers)
- [Microsoft: ProgramData](https://learn.microsoft.com/en-us/windows-hardware/customize/desktop/unattend/microsoft-windows-shell-setup-folderlocations-programdata)
- [Microsoft: Understanding AppLocker default rules](https://learn.microsoft.com/en-us/windows/security/application-security/app-control-for-business/applocker/understanding-applocker-default-rules)
- [Microsoft: AppLocker](https://learn.microsoft.com/en-us/windows/security/application-security/app-control-for-business/applocker/applocker-overview)
- [MITRE ATT&CK T1547.001: Registry Run Keys / Startup Folder](https://attack.mitre.org/techniques/T1547/001/)
- [MITRE ATT&CK T1574.001: DLL](https://attack.mitre.org/techniques/T1574/001/)
- [MITRE ATT&CK M1033: Limit Software Installation](https://attack.mitre.org/mitigations/M1033/)
- [MITRE ATT&CK DET0051: File/Path Exclusions](https://attack.mitre.org/detectionstrategies/DET0051/)
- [MITRE ATT&CK DET0363: LSASS Memory Dump Sequence](https://attack.mitre.org/detectionstrategies/DET0363/)

