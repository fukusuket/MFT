# 悪用されやすいファイルパス調査レポート

作成日: 2026-10-04
目的: $MFT / $UsnJrnl に適用するSigmaルールを設計するための基礎資料
関連: [architecture.md](architecture.md)（3.3 検知ルール（Sigma構文））

> 注記: 書き込み権限はWindowsの既定ACLに基づく。OSのバージョン、インストールされたアプリ、GPOによって変わる。実装時は、ベースライン生成用VM上で`accesschk -w`等で実測し、ベースラインDBに「標準ユーザーが書き込めるかどうか」を属性として持たせることを推奨する（6章）。

---

## 0. 要点

1. **パスから「書き込むのに最低限必要な権限」がわかる**。$MFT/$UsnJrnlには「誰がどのプロセスで」の情報がないが、パスと権限の対応から、**攻撃者の到達権限を推定**できる。
   - 標準ユーザーが書けない場所（`C:\Windows\System32`、`Program Files`、ドライバのディレクトリ）に、ベースライン外のファイルがある → 管理者/SYSTEM権限を取られている可能性が高い → **深刻度が高い**
   - 標準ユーザーが書ける場所（`%TEMP%`、`C:\Users\Public`、`C:\ProgramData`、`C:\PerfLogs`）→ **攻撃の頻度は高いが、正規ソフトのノイズも多い**
2. **攻撃者が好む場所は収束している**：`C:\Users\Public`、`C:\ProgramData`、`C:\PerfLogs`、`C:\Windows\Temp`、`%TEMP%`、`%APPDATA%`。SigmaやDFIR Reportで繰り返し観測されている。
3. **「正規の名前を、正規でない場所に」置く手口（T1036.005）**は、ベースライン差分と非常に相性が良い。`svchost.exe`が`C:\ProgramData`にある、といったケースは、パスだけで高精度に検出できる。
4. **$MFTの$SIにはSecurity IDがある**。`$Secure:$SDS`もあわせて収集すれば、**ファイル所有者のSID**（どのユーザーが作ったか）を推定できる。収集対象への追加を推奨する（数MB程度）。

---

## 1. Windowsの既定の書き込み権限（概要）

| 場所 | 標準ユーザー | 管理者 | 備考 |
|---|---|---|---|
| `C:\Users\<自分>\`（AppData、Desktop、Downloads、Documents） | ○ | ○ | 自分のプロファイル配下はすべて書き込める |
| `C:\Users\Public\` | ○ | ○ | 全ユーザーで共有。攻撃者の定番 |
| `C:\ProgramData\` | △ 新規作成のみ | ○ | BUILTIN\Usersは「ファイルの作成/フォルダの作成」が可能。既存ファイルの変更は不可（作成したファイルは自分が所有者になる） |
| `C:\`（ルート） | △ フォルダ作成のみ | ○ | Authenticated Usersにフォルダの作成権限がある → `C:\Temp`、`C:\tmp`、`C:\backup`等を任意に作れる |
| `C:\PerfLogs\` | △ | ○ | 既定で存在し、ほぼ空。標準ユーザーの書き込み可否はOSによる（要実測）。攻撃者の定番 |
| `C:\Windows\Temp\` | ○ 作成のみ | ○ | リモート実行ツール（Impacket等）の出力先としても頻出 |
| `C:\Windows\` 配下の書き込み可能なサブディレクトリ | ○ | ○ | 下表 |
| `C:\Windows\`（直下）、`System32`、`SysWOW64` | × | ○（多くはTrustedInstaller所有で変更不可） | 新規作成は管理者以上 |
| `C:\Program Files\`、`Program Files (x86)\` | × | ○ | — |
| `C:\Windows\System32\drivers\`、`DriverStore` | × | ○ | ドライバのロードにはさらに権限が必要 |

**C:\Windows配下で、標準ユーザーが既定で書き込めるディレクトリ**（api0cradle UltimateAppLockerByPassList、accesschkで調査されたもの）
- `C:\Windows\Tasks`
- `C:\Windows\Temp`
- `C:\Windows\tracing`
- `C:\Windows\Registration\CRMLog`
- `C:\Windows\System32\FxsTmp` / `C:\Windows\SysWOW64\FxsTmp`
- `C:\Windows\System32\com\dmp` / `C:\Windows\SysWOW64\com\dmp`
- `C:\Windows\System32\Microsoft\Crypto\RSA\MachineKeys`
- `C:\Windows\System32\spool\PRINTERS`
- `C:\Windows\System32\spool\SERVERS`
- `C:\Windows\System32\spool\drivers\color`
- `C:\Windows\System32\Tasks\Microsoft\Windows\SyncCenter`
- `C:\Windows\System32\Tasks_Migrated`
- `C:\Windows\SysWOW64\Tasks\Microsoft\Windows\SyncCenter`
- `C:\Windows\SysWOW64\Tasks\Microsoft\Windows\PLA\System`
- `C:\Windows\PLA\reports`、`\rules`、`\Templates`（2025年に報告されたもの）

これらは「C:\Windows配下なので安全そうに見える」うえに、AppLockerの既定ルール（`%WINDIR%\*`の実行を許可）をすり抜けられるため、**実行ファイルが置かれていたら重要度は高い**。

---

## 2. カテゴリ別の悪用されやすいパス

凡例
- 書込: 書き込みに必要な最低限の権限。U=標準ユーザー、A=管理者、S=SYSTEM/サービス
- ノイズ: 正規の活動による誤検知の多さ（高/中/低）
- レベル: 当該パスに**ベースライン外の実行可能ファイル**があった場合に、Sigmaルールの`level`に設定する値の目安（critical > high > medium > low > informational）。ルールごとに固定で、計算はしない（5章）。既存のSigmaHQルールとの対応は5.3節

### 2.1 ツール配置（ステージング）・実行の定番

| パス | 書込 | 典型的な悪用 | ノイズ | レベル | ATT&CK |
|---|---|---|---|---|---|
| `C:\PerfLogs\` | U/A | 攻撃ツール、EDRキラー、持ち出し用の.zipの置き場（Sophos、BlackCat関連のSigmaで観測） | **低**（通常ほぼ空） | high | T1074, T1105 |
| `C:\Users\Public\`（`Music`、`Documents`、`Downloads`、`Libraries`等を含む） | U | 初期アクセス後のツール置き場、rundll32で実行するDLLの置き場（DFIR Report等で多数観測） | 低〜中 | high | T1074, T1105 |
| `C:\ProgramData\`（直下、または新規作成されたサブディレクトリ） | U(作成) | ツール、バッチファイル、AD探索の結果、認証情報の書き出し。`svchost.exe`等へのなりすまし | 中（アプリのデータ） | medium（直下の実行ファイルはhigh） | T1074, T1036.005 |
| `C:\Windows\Temp\` | U/S | Impacket等の出力、サービス経由で実行されたペイロード、ダンプファイル | 中（インストーラー、更新） | medium | T1105, T1569.002 |
| `%LOCALAPPDATA%\Temp\`（`%TEMP%`） | U | フィッシングのペイロード（添付ファイルの展開、ISO/ZIPの中身）、ローダー | **高**（ブラウザ、インストーラー） | low（拡張子なしの実行ファイル、.ps1、.hta、.jsはmedium） | T1204, T1059 |
| `%APPDATA%\`（Roaming） | U | インフォスティーラー、RAT本体、正規アプリ名を装ったディレクトリ | 高 | low（直下の実行ファイルはmedium） | T1036.005 |
| `%LOCALAPPDATA%\`（直下、または正規アプリを装ったディレクトリ） | U | 同上。正規の署名付きexeとDLLを置くサイドローディング | 高 | low | T1574.002 |
| `%USERPROFILE%\Downloads\`、`Desktop\` | U | 初期アクセスのファイル（Zone.Identifierつき） | 高 | informational（Zone.Identifierがあれば別ルールで検出） | T1204 |
| `C:\<任意の新規フォルダ>\`（`C:\Temp`、`C:\tmp`、`C:\Intel`、`C:\backup`等） | U(作成) | 正規そうな名前のフォルダを作ってツールを置く | 低〜中 | medium | T1074 |
| `C:\Intel\Logs\` | U(作成) | 正規に見える名前のステージング先（Sigma） | 低 | medium | T1036.005 |
| `C:\$Recycle.Bin\` | U | ごみ箱に偽装した実行ファイル | 低 | medium | T1564 |
| `C:\Users\Default\`、`C:\Users\All Users\`、`C:\Users\NetworkService\` | A | 新規ユーザー作成時に複製されるなどの悪用。通常は実行ファイルがない場所 | 低 | medium | — |
| `\config\systemprofile\`（`C:\Windows\System32\config\systemprofile\`） | S | SYSTEM権限で動くマルウェアのAppData相当 | 低〜中 | medium | — |
| `C:\Windows\` 配下の書き込み可能なディレクトリ（1章のリスト） | U | AppLockerの回避、目立たない置き場 | 低 | high | T1218 |
| `C:\Windows\{addins,debug,Fonts,Help,IME,Media,repair,security}\` | A | 通常は実行ファイルが置かれない、システム風の場所への配置（Sigma: 実行元として不審） | 低 | high | T1036.005 |

### 2.2 永続化

| パス | 書込 | 内容 | ノイズ | レベル | ATT&CK |
|---|---|---|---|---|---|
| `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup\` | U | ログオン時に自動実行される | 低 | high | T1547.001 |
| `C:\ProgramData\Microsoft\Windows\Start Menu\Programs\StartUp\` | A | 全ユーザーのログオン時に実行される | 低 | high | T1547.001 |
| `C:\Windows\System32\Tasks\`（タスク定義XML） | S（タスクスケジューラ経由） | 新規タスクの作成痕跡。**USNの`FILE_CREATE`で作成時刻がわかる**。`\Tasks\`直下のランダム名や、正規名を装ったタスクに注意 | 中（アプリの更新タスク） | medium | T1053.005 |
| `C:\Windows\Tasks\`（.job） | U | 旧形式のタスク、またはファイルの置き場 | 低 | medium | T1053 |
| `C:\Windows\`直下のランダム名の`.exe` | A | **Impacket smbexec/psexec系、PsExec（`PSEXESVC.exe`）のサービスバイナリ**（ADMIN$ = `C:\Windows`経由） | 低 | high | T1569.002, T1021.002 |
| `C:\Windows\`直下の`__<数字>.<数字>`等 | A | **Impacket wmiexecのコマンド出力ファイル**（ADMIN$に書いて読む） | 低 | high | T1047 |
| `C:\Windows\System32\` / `SysWOW64\` の新規ファイル | A | サービスDLL、偽のシステムファイル | 中（更新） | high（ベースライン外の場合） | T1543.003 |
| `C:\Windows\System32\sethc.exe`、`utilman.exe`、`osk.exe`、`Magnify.exe`、`Narrator.exe`、`DisplaySwitch.exe` | A(TI) | アクセシビリティ機能のバックドア（cmd.exeに置き換え）。**パスは標準なので、サイズ・タイムスタンプの変化で検知**する | 低 | high | T1546.008 |
| `C:\Windows\System32\wbem\Repository\` | S | WMIイベントサブスクリプションによる永続化（`OBJECTS.DATA`の更新）。ファイルの更新だけでは判定できないため、補助的なシグナル | 中 | informational | T1546.003 |
| `%USERPROFILE%\Documents\WindowsPowerShell\profile.ps1`、`C:\Windows\System32\WindowsPowerShell\v1.0\profile.ps1` | U / A | PowerShellプロファイル | 低 | medium | T1546.013 |
| `%APPDATA%\Microsoft\Word\STARTUP\`、`Excel\XLSTART\`、`%APPDATA%\Microsoft\AddIns\` | U | Officeの起動時に読み込まれるアドイン | 低 | medium | T1137 |
| `C:\Windows\System32\GroupPolicy\Machine\Scripts\`、DCの`SYSVOL\...\Scripts\`、`Policies\{GUID}\Machine\Preferences\ScheduledTasks\` | A（DC） | GPOによる横展開・ランサムウェアの一斉配布 | 低 | high（DC） | T1484.001 |
| `*.scr`（任意の場所） | — | スクリーンセーバーによる永続化 | 低 | medium | T1546.002 |

### 2.3 なりすまし（Masquerading）・サイドローディング

| パターン | 例 | 判定方法 | レベル |
|---|---|---|---|
| システムバイナリ名が標準外のパスにある | `svchost.exe`、`lsass.exe`、`csrss.exe`、`services.exe`、`smss.exe`、`winlogon.exe`、`rundll32.exe`、`dllhost.exe`、`conhost.exe`、`taskhostw.exe`、`spoolsv.exe`、`explorer.exe`（標準は`C:\Windows\`）が、ProgramData/AppData/Public等にある | ベースラインの「名前 → 許可されたパス集合」と照合する | high |
| 似た綴り | `scvhost.exe`、`svch0st.exe`、`lsas.exe`、`expl0rer.exe` | システム名との編集距離が1〜2 | medium |
| 正規アプリの更新ツールを装う名前 | `msedge_update*.exe`、`chrome_installer*.exe`、`onedrive_setup*.exe`、`teams_update*.exe` | 標準のインストール先以外にあり、名前にランダムなhexを含む | medium |
| 二重拡張子・RLO | `invoice.pdf.exe`、U+202Eを含む名前 | 名前のパターン | medium |
| DLLサイドローディング | 署名付きの正規exeと、同名でないDLLが、ユーザー書き込み可能な同じディレクトリに同時に作成される | USNで同時刻に作成されたexeとdllの組を検出 | medium |
| PATH上のユーザー書き込み可能なディレクトリ | `%LOCALAPPDATA%\Microsoft\WindowsApps\`（Windows 10以降でユーザーのPATHに含まれる）へのDLL・exeの追加 | 標準以外のファイル | medium |
| Webシェル（正規ファイル名を模倣） | MOVEitの`human2.aspx`（正規は`human.aspx`） | Webルート配下の、ベースライン外の`.aspx`等 | high |

### 2.4 Webシェル（サーバー）

| パス | 内容 | レベル |
|---|---|---|
| `C:\inetpub\wwwroot\`、`C:\inetpub\wwwroot\aspnet_client\` | IIS/ExchangeのWebシェル（China Chopper等）。ProxyShellでは`MSExchangeMailboxReplication.exe`がランダム名の`.aspx`を書き込む | high |
| `C:\Program Files\Microsoft\Exchange Server\V15\FrontEnd\HttpProxy\owa\auth\`、`\ecp\`、`\OAB\` | ExchangeのWebシェル | high |
| `C:\Windows\Microsoft.NET\Framework*\v4.0.30319\Temporary ASP.NET Files\` | ASP.NETのコンパイル結果。Webシェルが実行された痕跡（`App_Web_*.dll`の作成） | medium |
| 各種Webアプリのルート（Tomcatの`webapps\`、PHP、各社アプライアンスのWebディレクトリ） | `.jsp/.jspx/.php/.ashx/.asmx/.aspx` | high |

### 2.5 認証情報の窃取・持ち出しの痕跡

| パス/パターン | 内容 | レベル |
|---|---|---|
| `*.dmp`（`C:\Windows\Temp\`、`C:\ProgramData\`、`C:\Users\Public\`、`%TEMP%`）、特に`lsass*.dmp` | comsvcs.dll MiniDump、ProcDump等によるLSASSのダンプ | high |
| `ntds.dit`の複製（`C:\ProgramData\`、`C:\Windows\Temp\`、`ntdsutil`のIFM出力先の`Active Directory\ntds.dit`＋`registry\SYSTEM`） | ADの認証情報DBの窃取 | high |
| `SAM`、`SYSTEM`、`SECURITY`のハイブのコピー（`*.save`、`*.hiv`等） | `reg save`によるハイブの窃取 | high |
| ブラウザの認証情報DBのコピー（`Login Data`、`Cookies`等が、標準外の場所に複製されている） | インフォスティーラー | medium |
| アーカイブ（`.zip/.7z/.rar`）がステージング場所で作成され、短時間で削除される | 持ち出しの準備 | medium |
| `%APPDATA%\rclone\rclone.conf`、`rclone.exe`（リネーム含む） | クラウドへの持ち出し | high |
| MEGAsync、WinSCP、FileZilla等の標準外の配置 | 持ち出し | medium |

### 2.6 リモート管理ツール（RMM）・トンネル

| ツール | 主なパス | レベル |
|---|---|---|
| AnyDesk | `C:\ProgramData\AnyDesk\`、`%APPDATA%\AnyDesk\`（`ad.trace`、`connection_trace.txt`） | medium（組織で未使用の場合はhigh） |
| ScreenConnect / ConnectWise Control | `C:\Program Files (x86)\ScreenConnect Client (*)\` | medium |
| TeamViewer、Atera、Splashtop、NetSupport、MeshAgent等 | 各インストール先、ポータブル版は`%TEMP%` / Downloads | medium |
| トンネル/プロキシ（ngrok、chisel、Cloudflared、plink） | 任意（多くはステージング場所） | high |

RMMは組織によっては正規に使われているので、**自組織ベースライン（オプション）で許可できる**ようにする。

### 2.7 ドライバ（BYOVD / EDRキラー）

| パターン | 内容 | レベル |
|---|---|---|
| `C:\Windows\System32\drivers\`・`DriverStore`以外にある`.sys` | 脆弱な署名付きドライバの持ち込み（BYOVD）。2025〜2026年には54種のEDRキラーが35種の脆弱ドライバを悪用しているという報告がある | high |
| `C:\Windows\System32\drivers\`内のベースライン外の`.sys` | 同上（サービスとして登録された場合） | high |
| `.sys`の作成直後に、セキュリティ製品のファイルが削除・変更されている | EDR無効化の連鎖 | high |

### 2.8 アンチフォレンジック（削除・改ざんされやすい場所）

| パス | 着目点 | レベル |
|---|---|---|
| `C:\Windows\System32\winevt\Logs\*.evtx` | USNの`FILE_DELETE` / `DATA_TRUNCATION`（`wevtutil cl`ではサイズが縮小する） | high |
| `C:\Windows\Prefetch\*.pf` | 大量削除（痕跡の消去）。逆に、`.pf`の**作成**は実行の痕跡 | high（削除） |
| `C:\Windows\AppCompat\Programs\Amcache.hve` | 削除・置き換え | medium |
| `C:\Windows\System32\sru\SRUDB.dat` | 削除 | medium |
| `$Recycle.Bin`、ユーザーの`Recent\`、`AutomaticDestinations\` | 削除の集中 | low |
| 任意の場所での`AAAA.AAA`→…→`ZZZZ.ZZZ`のリネームの連鎖 | SDelete | high |
| `$Extend\$UsnJrnl`（ジャーナルの再作成） | Journal IDの変化（Event ID 3079と対応） | high |

---

## 3. 権限からの推定（攻撃段階の判定）

| ベースライン外のファイルが見つかった場所 | 推定される攻撃者の権限 | 意味 |
|---|---|---|
| ユーザープロファイル配下のみ | 標準ユーザー | 初期アクセスの段階の可能性。権限昇格の前 |
| `C:\Users\Public`、`C:\ProgramData`、`C:\PerfLogs`、ルート直下の新規フォルダ | 標準ユーザー以上 | 汎用的なステージング。所有者SIDで切り分ける（4章） |
| `C:\Windows\Temp`（Impacket等のパターン） | 管理者（リモート） | **横展開を受けている**可能性 |
| `C:\Windows\`直下、`System32`、`Program Files`、`drivers` | 管理者/SYSTEM | **端末を掌握されている**。最優先 |
| DCの`SYSVOL`、GPOのスクリプト | ドメイン管理者 | **ドメイン全体が侵害されている**可能性 |

複数端末を解析した場合、「同じベースライン外のファイルが、同じ時間帯に、管理者権限が必要なパスに現れている」ことは、横展開の範囲の特定に直結する。

---

## 4. 所有者SIDによる補強（アーキテクチャへの提案）

- $MFTの`$STANDARD_INFORMATION`（NTFS 3.0以降）は`Security ID`を持つ。これは`$Secure:$SDS`のセキュリティ記述子を指しており、**所有者SID**を取得できる。
- 判定例
  - `C:\ProgramData\x.exe`の所有者が一般ユーザーのSID → ユーザーの権限で作成された（初期アクセスの段階）
  - 所有者が`BUILTIN\Administrators`または`SYSTEM` → 権限昇格後に作成された
  - 所有者が、存在しないユーザー/新規作成されたローカルユーザーのSID → **バックドア用のアカウント**を疑う
- `$SDS`は通常数MB以下なので、**Collectorの収集対象に`$Secure:$SDS`を追加することを推奨する**（MFTECmdでもパース可能）。
- 制約：所有者は、作成したプロセスのトークンのデフォルト所有者に依存する（昇格済みの管理者だとAdministratorsグループになる）。ユーザーの特定は「推定」にとどまる。

---

## 5. Sigmaルールへの落とし込み（スコアは使わない）

重みの合算や係数による**計算スコアは採用しない**。結果が「なぜその順位なのか」を説明できないブラックボックスになるため。検知はすべて**Sigmaルール**で記述し、ルールに一致したかどうかだけで判定する（[architecture.md](architecture.md) 3.3節）。

- レベルはSigmaの`level`（critical / high / medium / low / informational）の固定値をそのまま使う。実行時に計算・変化しない。
- 1つのファイルが複数のルールに一致した場合は、**一致したルールをすべてそのまま列挙する**（合算しない）。
- 表示順は「レベル → 時刻」の単純な並びで、ユーザーがレベル・ルール・期間で絞り込める。
- 各Findingには、一致したルールのtitle、条件、根拠となった値（パス、タイムスタンプ、USNのReason等）を必ず表示する。**同じ入力からは必ず同じ結果になる**。
- 平易な説明文には`description`を使う。日本語訳はルール本体に独自フィールドを足さず、ルールIDをキーにした翻訳ファイルで管理する（SigmaHQとの互換性を保つため）。

### 5.1 ルールの例

**ステージング場所への実行ファイル配置**（2.1節）

```yaml
title: Executable Dropped In PerfLogs
id: 00000000-0000-0000-0000-000000000001   # 実際にはUUIDを生成する
status: experimental
description: C:\PerfLogs is normally empty on a standard Windows installation and is a well-known staging location for attacker tools.
references:
  - https://www.sophos.com/en-us/blog/ungentlemanly-behavior-insights-into-a-ransomware-operation
tags:
  - attack.command-and-control
  - attack.t1105
logsource:
  product: windows
  category: file_event        # USNのFILE_CREATEと、MFTの現在の状態の両方に適用される
detection:
  selection:
    TargetFilename|startswith: 'C:\PerfLogs\'
    TargetFilename|endswith:
      - '.exe'
      - '.dll'
      - '.ps1'
      - '.bat'
      - '.vbs'
      - '.hta'
  condition: selection
level: high
```

**なりすまし**（2.3節。正規の名前が標準外のパスにある）

```yaml
title: Svchost.exe Outside Standard Locations
logsource:
  product: windows
  category: file_event
detection:
  selection:
    TargetFilename|endswith: '\svchost.exe'
  filter_standard:
    TargetFilename:
      - 'C:\Windows\System32\svchost.exe'
      - 'C:\Windows\SysWOW64\svchost.exe'
  filter_winsxs:
    TargetFilename|startswith: 'C:\Windows\WinSxS\'
  condition: selection and not 1 of filter_*
level: high
```

**timestomp**は、Sigma標準のフィールドでは表現できないため、Sigmaルールにはしない。`$SI作成 < $FN作成`などの比較結果は、判定をしない「事実」としてFindingの詳細に表示する（[architecture.md](architecture.md) 3.3節「Sigmaの外で扱う事実」）。

**時間的な連鎖**（2.5節：アーカイブを作成した直後に削除。Sigmaのcorrelationルール）

```yaml
title: Archive Created In Staging Location
name: archive_created_staging
logsource:
  product: windows
  category: file_event
detection:
  selection:
    TargetFilename|startswith:
      - 'C:\PerfLogs\'
      - 'C:\Users\Public\'
      - 'C:\ProgramData\'
    TargetFilename|endswith: ['.zip', '.7z', '.rar']
  condition: selection
---
title: Archive Deleted
name: archive_deleted
logsource:
  product: windows
  category: file_delete
detection:
  selection:
    TargetFilename|endswith: ['.zip', '.7z', '.rar']
  condition: selection
---
title: Archive Created In Staging Location Then Deleted (Possible Exfiltration)
correlation:
  type: temporal_ordered
  rules:
    - archive_created_staging
    - archive_deleted
  group-by:
    - TargetFilename
  timespan: 24h
level: high
```

使用するフィールドはSigma標準の`TargetFilename`（リネームでは`SourceFilename`も）だけ。ベースラインの内外はルールの条件にせず、Findingの属性として表示する（[architecture.md](architecture.md) 3.3節）。

### 5.2 ルール作成時の拡張子リストの目安

Sigmaにはルール間で共有するリストがないため、各ルールに列挙する。共通化したい場合は、Sigmaのプレースホルダ（`|expand`修飾子と`%placeholder%`）で定義することも検討する。

| 用途 | 拡張子 |
|---|---|
| 実行可能ファイル・スクリプト | `.exe .dll .sys .scr .ps1 .bat .cmd .vbs .js .jse .hta .wsf .lnk .msi .cpl` |
| Webシェル | `.aspx .ashx .asmx .jsp .jspx .php` |
| アーカイブ・ディスクイメージ | `.zip .7z .rar .iso .img .vhd .vhdx` |
| 認証情報のダンプ | `.dmp .dit .hiv .save` |

### 5.3 既存のSigmaHQルールとの対応

SigmaHQ（2026-10時点）の`file_*`ルールのうち、**プロセス系の項目を使わずファイル名だけで判定するもの**と、2章のパターンとの対応。これらは流用できる。対応するルールがないものは自作する。

| 2章のパターン | 流用できるSigmaHQルール（level） | 対応 |
|---|---|---|
| `C:\PerfLogs\` | `file_event_win_perflogs_susp_files`（medium） | 流用。本書の目安（high）より低いので、対象を実行ファイルに絞ったhighの自作ルール（5.1節）も用意する |
| `C:\Users\Public\` | `file_event_win_susp_public_folder_extension`（high） | 流用 |
| `C:\Windows\System32\spool\drivers\color` | `file_event_win_susp_spool_drivers_color_drop`（medium） | 流用。ほかの書き込み可能ディレクトリ（1章）は自作 |
| `%APPDATA%`の不審な場所 | `file_event_win_new_files_in_uncommon_appdata_folder`（high） | 流用 |
| `$Recycle.Bin`内の実行ファイル | なし（`file_event_win_susp_recycle_bin_fake_exec`はプロセス系の項目を使う） | 自作 |
| Startupフォルダ | `file_event_win_susp_startup_folder_persistence`（high） | 流用 |
| タスク定義（`System32\Tasks`） | `file_event_win_scheduled_task_creation`（low） | 流用 |
| Impacket wmiexecの出力 / ADMIN$ | `file_event_win_wmiexec_default_filename`（critical）、`file_event_win_impacket_file_indicators`（high）、`file_event_win_writing_local_admin_share`（medium） | 流用 |
| PsExec / RemCom / CSExec | `file_event_win_sysinternals_psexec_service`（low）、`..._psexec_service_key`（high）、`file_event_win_remcom_service`（medium）、`file_event_win_csexec_service`（medium） | 流用 |
| PowerShellプロファイル | `file_event_win_susp_powershell_profile`（medium） | 流用 |
| Officeのアドイン | `file_event_win_office_addin_persistence`（high） | 流用 |
| GPOのスクリプト | `file_event_win_susp_default_gpo_dir_write`（medium） | 流用 |
| `.scr` | `file_event_win_new_scr_file`（medium） | 流用 |
| なりすまし（システムDLL名） | `file_event_win_creation_system_dll_files`（medium）、`file_event_win_create_non_existent_dlls`（medium） | 流用。システムexe名（svchost等）は自作（5.1節） |
| 二重拡張子・RLO・ホモグリフ | `file_event_win_susp_double_extension`（high）、`..._right_to_left_override_extension_spoofing`（high）、`..._homoglyph_filename`（medium） | 流用 |
| DLLサイドローディング | `file_event_win_iphlpapi_dll_sideloading`（high）、`file_event_win_dll_sideloading_space_path`（high） | 流用。同時作成のcorrelationは自作 |
| LSASSダンプ | `file_event_win_lsass_default_dump_file_names`（high）、`file_event_win_lsass_shtinkering`（high）、`file_event_win_dump_file_creation`（low） | 流用 |
| ntds.dit / SAMなどのハイブ | `file_event_win_ntds_dit_creation`（low）、`file_event_win_ntds_exfil_tools`（high）、`file_event_win_sam_dump`（high）、`file_event_win_susp_registry_hive_file_creation`（high）、`file_event_win_hktl_hivenightmare_file_exports`（high） | 流用 |
| 認証情報窃取ツール | `file_event_win_cred_dump_tools_dropped_files`（high）、`file_event_win_hktl_mimikatz_files`（critical）ほか`hktl_*` | 流用 |
| rclone | `file_event_win_rclone_config_files`（medium） | 流用 |
| AnyDesk / ScreenConnect / TeamViewer / GoToAssist | `file_event_win_anydesk_artefact`、`..._remote_access_tools_screenconnect_artefact`、`..._install_teamviewer_desktop`、`..._gotoopener_artefact`（いずれもmedium） | 流用 |
| Webシェル | なし（`file_event_win_webshell_creation_detect`、`..._exchange_webshell_drop`はプロセス系の項目を使う） | 自作（Webルート配下の、ベースライン外の`.aspx`等） |
| BYOVD（標準外の`.sys`） | なし | 自作 |
| `.evtx`の削除 | `file_delete_win_delete_event_log_files`（medium） | 流用 |
| PowerShell履歴・IISログ・Exchangeログの削除 | `file_delete_win_delete_powershell_command_history`（medium）、`..._delete_iis_access_logs`（medium）、`..._delete_exchange_powershell_logs`（high） | 流用 |
| SDelete | `file_delete_win_sysinternals_sdelete_file_deletion`（medium） | 流用。連続リネームのcorrelationは自作 |
| Zone.Identifierの削除 | `file_delete_win_zone_identifier_ads`（low） | 流用 |
| ランサムウェアの拡張子追加 | `file_rename_win_ransomware`（medium） | 流用。大量リネームの`event_count`は自作 |
| Prefetchの大量削除 | なし | 自作（`file_delete`の`event_count`） |
| ジャーナル削除、timestomp | なし | Sigmaでは表現できない（標準フィールドがない）。判定をしない「事実」として表示する |

SigmaHQのルールの`level`は変更せず、そのまま使う。本書の目安とレベルが異なる場合は、条件を絞った自作ルールを別に追加する（既存ルールを書き換えると、SigmaHQの更新に追従しにくくなるため）。

**ノイズ対策**
- `%TEMP%`・`%APPDATA%`・`%LOCALAPPDATA%`は正規アプリ（ブラウザ、Teams、OneDrive、Zoom、インストーラー）が非常に多い。**アプリ単位の許可リスト（正規のインストール先と、ランダム名の正規パターン）**をベースラインに含める必要がある。
- ProgramDataは、アプリのデータ（ログ、キャッシュ）が多い。**ディレクトリが新規作成された時期**（OSインストールや既知アプリのインストール時期と離れているか）をシグナルにする。

---

## 6. 実装への提言

1. **ベースラインDBにディレクトリの属性を持たせる**：ベースライン生成用VMで`accesschk -w -s Users / "Authenticated Users" / Everyone`を実測し、「標準ユーザーが書き込めるかどうか」をディレクトリごとに保持する。こうすれば、パスのリストをハードコードせず、OSのバージョン差にも追従できる。
2. **検知はすべてSigmaルールで管理する**。SigmaHQからの流用分（5.3節）と自作分（`rules/windows/ntfs/…`）を分け、自作分のうち汎用的なものはSigmaHQへ還元する。
3. **システムバイナリの「名前 → 許可されたパス」の対応表**は、ベースラインから自動生成する（なりすまし検知用）。
4. **Collectorで`$Secure:$SDS`を追加収集する**（所有者SID）。
5. **レポートでは、理由を平易な文章で表示する**。例:「`C:\PerfLogs` はWindows標準では通常空のフォルダで、攻撃者がツールの置き場として好んで使う場所です」。

---

## 7. 参考資料

- [api0cradle/UltimateAppLockerByPassList - Generic-AppLockerbypasses.md](https://github.com/api0cradle/UltimateAppLockerByPassList/blob/master/Generic-AppLockerbypasses.md)（C:\Windows配下の書き込み可能なディレクトリ）
- [UltimateAppLockerByPassList Issue #23: %WINDIR%\PLA bypass (2025)](https://github.com/api0cradle/UltimateAppLockerByPassList/issues/23)
- [SigmaHQ/sigma（ルールリポジトリ）](https://github.com/SigmaHQ/sigma) / [Sigma Correlation Rules Specification](https://github.com/SigmaHQ/sigma-specification)
- [SigmaHQ: Process Execution From A Potentially Suspicious Folder](https://github.com/SigmaHQ/sigma/blob/master/rules/windows/process_creation/proc_creation_win_susp_execution_path.yml)
- [Suspicious Process Execution in PerfLogs Directory (BlackCat)](https://detection.fyi/mbabinski/sigma-rules/2022_blackcat_ransomware/win_susp_process_exec_in_perflogs_path/)
- [Sophos: Ungentlemanly behavior: Insights into a ransomware operation](https://www.sophos.com/en-us/blog/ungentlemanly-behavior-insights-into-a-ransomware-operation)（C:\PerfLogsでのステージング）
- [The DFIR Report: Blurring the Lines (2025)](https://thedfirreport.com/2025/09/08/blurring-the-lines-intrusion-shows-connection-with-three-major-ransomware-gangs/)
- [The DFIR Report: Malicious ISO File Leads to Domain Wide Ransomware (2023)](https://thedfirreport.com/2023/04/03/malicious-iso-file-leads-to-domain-wide-ransomware/)
- [MITRE ATT&CK T1036.005 Match Legitimate Resource Name or Location](https://attack.mitre.org/techniques/T1036/005/)
- [Microsoft Q&A: Default group permissions for ProgramData folder](https://learn.microsoft.com/en-us/answers/questions/4328564/default-group-permissions-for-programdata-folder)
- [Google Cloud (Mandiant): Escalating Privileges via Third-Party Windows Installers](https://cloud.google.com/blog/topics/threat-intelligence/privileges-third-party-windows-installers/)
- [Portcullis Labs: Allowing low privileged users to create directories in C:\](https://labs.portcullis.co.uk/blog/allowing-low-privileged-users-to-create-directories-in-c/)
- [itm4n: Windows DLL Hijacking (Hopefully) Clarified](https://itm4n.github.io/windows-dll-hijacking-clarified/)
- [Red Canary: Microsoft Exchange attacks](https://redcanary.com/blog/threat-intelligence/microsoft-exchange-attacks/) / [Huntress: ProxyShell](https://www.huntress.com/blog/rapid-response-microsoft-exchange-servers-still-vulnerable-to-proxyshell-exploit)
- [Splunk: MS Exchange Mailbox Replication Service Writing Active Server Pages](https://github.com/splunk/security_content/blob/develop/detections/endpoint/ms_exchange_mailbox_replication_service_writing_active_server_pages.yml)
- [The Hacker News: 54 EDR Killers Use BYOVD to Exploit 35 Signed Vulnerable Drivers (2026)](https://thehackernews.com/2026/03/54-edr-killers-use-byovd-to-exploit-34.html)
- [Red Canary 2025 Threat Detection Report](https://redcanary.com/blog/threat-detection/2025-threat-detection-report/)
- [Splunk: Detection: USN Journal Deletion](https://research.splunk.com/endpoint/b6e0ff70-b122-4227-9368-4cf322ab43c3/)
