# ADR 0001: ライセンス

- 状態: 採用
- 日付: 2026-10-04

## 背景
Hayabusaのコード（Sigmaエンジン）の流用、SigmaHQルールの流用、`mft` crate（Apache-2.0）の利用を、
すべて満たせるライセンスを、最初のコミットより前に決める必要があった（docs/engineering-review.md R7）。

## 決定
| 対象 | ライセンス |
|---|---|
| 本体（全クレート、viewer、collector） | **AGPL-3.0-only**（Hayabusaと同じAGPLv3。2026-10-04にGitHubで確認） |
| 検知ルール | **DRL 1.1**（SigmaHQ、hayabusa-rulesと同じ） |
| ベースラインのデータ | 未定（別のADRで決める） |

`-only` と `-or-later` のどちらにするかは、Hayabusaの方針に合わせる。Hayabusaの `Cargo.toml` には
`license` フィールドがないため、暫定で `-only` とした。変更する場合は `Cargo.toml` と `NOTICE` を直す。

## 結果
- GPLv3のコード（dfir_ntfs など）は、AGPLv3の著作物と組み合わせられる。流用するときは `NOTICE` に出典を書く。
- 依存は、AGPLv3と組み合わせられるライセンスに限る。`deny.toml` で強制する。
- HTMLレポートにはViewerのコード（AGPL）が含まれるため、フッターにライセンスとソースの入手先を表示する。
- DRL 1.1の条件により、ルールに一致した結果の出力（レポート、JSONL、CSV）には、ルールの `author` と参照先を含める。
- AGPLの第13条（ネットワーク越しの利用）は、ローカルCLIのMVPには影響しない。サーバーの形態を追加するときに見直す。
