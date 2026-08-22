> **Moved to `docs/history/` on 2026-08-22, and renamed with its date.** It sat at the top
> of `docs/` where it read as current, and it is not: it audits a project called
> *Ferruginous* at commit `92e8604`, reports **21 tests** where there are now 461, and
> lists defects against line numbers three months of work has moved. Nothing in the tree
> referenced it.
>
> A dated audit is a historical document — the same reasoning that moved
> `charter_redesign_2026-04-13.md` here. It is not updated, and it is worth keeping: the
> shape of what this codebase kept getting wrong in May is legible in it, and several
> items below were still open in August under different names.

# Ferruginous コードベース監査レポート (第3回)

> **監査日時**: 2026-05-27  
> **最新コミット**: `92e8604` (docs/refactor: update compliance & rendering rules, fix needless borrows in R5 key derivation)  
> **ビルド**: ✅ 成功 / **テスト**: ✅ 全パス (21件, 1 ignored) / **Clippy**: ✅ **警告ゼロ** / **unsafe**: ✅ 使用なし (`forbid`)

---

## 前回（第2回）からの修正状況

| 前回の指摘 | 重大度 | 状態 |
|:---|:---:|:---:|
| テストカバレッジ不足 (7件のみ) | 🟠 High | ✅ **修正済** — 21件 (+6 SDK, +8 Core) |
| スタブAPI (`set_vacuum`/`set_strip`/`set_password`) | 🟠 High | ✅ **修正済** — フィールド同期実装済 |
| `upgrade_to_standard()` が no-op | 🟠 High | ✅ **修正済** — 全4標準のカタログ操作実装 |
| Signature STUB (MCP) | 🟠 High | ⚠️ **未修正** → Medium に降格 |
| Clippy 警告 10件 | 🟡 Medium | ✅ **修正済** — 0件に |
| FIXME 3件 | 🟡 Medium | ✅ **修正済** — 0件に |
| STUB 4件 | 🟡 Medium | ✅ **修正済** — 0件に |
| `retag()` 未実装 | 🟡 Medium | ✅ **修正済** — `apply_remediations` 呼び出し |
| V5 暗号化の非標準キー導出 | 🟡 Medium | ✅ **修正済** — 50-round SHA-256 実装 |
| Lab→RGB 仮実装 | 🟡 Medium | ✅ **修正済** — D65 XYZ + BT.709 + sRGB companding |
| parser.rs 内 unwrap 2箇所 | 🟡 Medium | ⚠️ **未修正** |
| obj_stm.rs 内 write! unwrap | 🟡 Medium | ⚠️ **未修正** |
| writer.rs L2077 unwrap | 🟡 Medium | ⚠️ **未修正** |
| テスト内 `panic!` | 🟡 Medium | ⚠️ **微改善** — 2箇所→2箇所 (1箇所は `#[ignore]` テスト) |
| `find_object()` O(n) | 🟢 Low | ✅ **修正済** — BTreeMap インデックス O(log n) |
| macOS ハードコードパス | 🟢 Low | ⚠️ **未修正** |
| bin ツール `unwrap()` 多用 | 🟢 Low | ⚠️ **未修正** |
| lint allow 過多 | 🟢 Low | ⚠️ **未修正** |

---

## 1. 重大度: Critical

**なし** ✅

## 2. 重大度: High

**なし** ✅

---

## 3. 重大度: Medium（改善推奨）

### 3.1 MCP Signature Verification が未実装
**影響範囲**: [mcp/tools/signature.rs](file:///Users/jun/Projects/ferruginous/crates/ferruginous-mcp/src/tools/signature.rs)

MCP 経由の署名検証ツールはまだコメント + プレースホルダー状態。`ferruginous-sdk` 側の `save_signed()` は動作するため、MCP ブリッジ側のみの問題。

### 3.2 ライブラリコード内の `.unwrap()` (3箇所)

| 場所 | リスク | 備考 |
|:---|:---:|:---|
| [sublimation/parser.rs:667,673](file:///Users/jun/Projects/ferruginous/crates/ferruginous-core/src/object/sublimation/parser.rs#L667) | 低 | peek 後の next。実質安全だが RR-15 Rule 2 違反 |
| [obj_stm.rs:53](file:///Users/jun/Projects/ferruginous/crates/ferruginous-sdk/src/obj_stm.rs#L53) | 極低 | `Vec<u8>` への write! (失敗しない) |
| [writer.rs:2077](file:///Users/jun/Projects/ferruginous/crates/ferruginous-sdk/src/writer.rs#L2077) | 低 | `is_empty()` ガード内 |

### 3.3 `reconstruction.rs` 内の `.unwrap()` (3箇所)

| 場所 | 種類 |
|:---|:---|
| [reconstruction.rs:1657](file:///Users/jun/Projects/ferruginous/crates/ferruginous-core/src/font/reconstruction.rs#L1657) | `char::from_u32(0x30 + n).unwrap()` — 数値が固定範囲のため安全 |
| [reconstruction.rs:1665](file:///Users/jun/Projects/ferruginous/crates/ferruginous-core/src/font/reconstruction.rs#L1665) | 同上 (A-Z 範囲) |
| [reconstruction.rs:1672](file:///Users/jun/Projects/ferruginous/crates/ferruginous-core/src/font/reconstruction.rs#L1672) | 同上 (a-z 範囲) |

数学的に安全ですが、`// SAFETY:` コメントまたは `unwrap_or` への変換が RR-15 的に望ましい。

### 3.4 `serializer.rs` の flate2 unwrap (2箇所)

| 場所 | 内容 |
|:---|:---|
| [serializer.rs:284](file:///Users/jun/Projects/ferruginous/crates/ferruginous-core/src/object/sublimation/serializer.rs#L284) | `encoder.write_all(data).unwrap()` |
| [serializer.rs:285](file:///Users/jun/Projects/ferruginous/crates/ferruginous-core/src/object/sublimation/serializer.rs#L285) | `encoder.finish().unwrap()` |

ZlibEncoder の write/finish は `Vec<u8>` バックエンドのため実質失敗しませんが、`Result` として返す方が堅牢です。

### 3.5 テスト内 `panic!` (2箇所)

| 場所 | 備考 |
|:---|:---|
| [reconstruction.rs:2054](file:///Users/jun/Projects/ferruginous/crates/ferruginous-core/src/font/reconstruction.rs#L2054) | `#[ignore]` テスト内。`assert!` に変換推奨 |
| [sdk/tests.rs:133](file:///Users/jun/Projects/ferruginous/crates/ferruginous-sdk/src/tests.rs#L133) | `_ => panic!("Expected Rgb")` — `assert!(matches!(...))` が自然 |

### 3.6 TODO コメント (2件)

| 場所 | 内容 |
|:---|:---|
| [security.rs:108](file:///Users/jun/Projects/ferruginous/crates/ferruginous-core/src/security.rs#L108) | `TODO(RR-15-EXT): Transition to full multi-stage key verification` — 現状の 50-round は簡略版。ISO 完全準拠には Algorithm 2.A/3.A 実装が必要 |
| [xobject.rs:354](file:///Users/jun/Projects/ferruginous/crates/ferruginous-sdk/src/interpreter/ops/xobject.rs#L354) | `TODO(RR-15-EXT): Transition from fallback RGB8 and implement proper index expansion` |

### 3.7 `eprintln!` in bin ツール (2箇所)

| 場所 | 用途 |
|:---|:---|
| [dump_page_contents.rs:6](file:///Users/jun/Projects/ferruginous/crates/ferruginous-core/src/bin/dump_page_contents.rs#L6) | `eprintln!("Dumping page contents...");` — 開発ツール |
| [find_page.rs:7](file:///Users/jun/Projects/ferruginous/crates/ferruginous-core/src/bin/find_page.rs#L7) | `eprintln!("Usage: ...")` — usage メッセージ |

ライブラリコードからは完全に排除済み。bin ツール内の使用は許容レベル。

---

## 4. 重大度: Low（品質改善）

### 4.1 バイナリツール群の `.unwrap()` 多用
`src/bin/` 以下に **30箇所以上**。開発専用ツールのため影響は限定的。

### 4.2 `#[allow(clippy::...)]` ワークスペース全体への過剰適用
[Cargo.toml:95-121](file:///Users/jun/Projects/ferruginous/Cargo.toml#L95-L121) で 27種類の lint を `allow`。ただし Clippy 警告ゼロを達成しているため、実害はない。

### 4.3 macOS ハードコードフォントパス
設計意図として `FERRUGINOUS_RESOURCES` / `assets/fonts/` フォールバックが存在。cross-platform 完全対応は Phase 26 ロードマップ項目。

### 4.4 example の `missing-docs` 警告 (5件)
`cargo test` 時に example ファイルで `missing-docs` 警告が出力。ワークスペースの `missing_docs = "warn"` 設定が examples にも適用されている。

---

## 5. テストカバレッジ詳細

| クレート | テスト数 | 前回比 | 内容 |
|:---|:---:|:---:|:---|
| ferruginous-core | 12 (+8) | ⬆️ | CMap, Font reconstruction, schema, object macro, font precipitation, GID resolution |
| ferruginous-render | 3 (±0) | — | Text positioning, matrix init, path v_y |
| ferruginous-sdk | 6 (+6) | 🆕 | Lab-to-sRGB, ObjStm packer, R5 key derivation, retag, upgrade, save settings |
| その他 | 0 | — | — |
| **合計** | **21** | **+14** | 1 ignored (test_reconstructed_font_parsing) |

---

## 6. 全体サマリー

| 重大度 | 第1回 | 第2回 | 第3回 | 推移 |
|:---|:---:|:---:|:---:|:---|
| 🔴 Critical | 3 | 0 | **0** | ✅ 解消維持 |
| 🟠 High | 4 | 2 | **0** | ✅ 全件解消 |
| 🟡 Medium | 6 | 7 | **7** | → 内容は大幅に軽量化 |
| 🟢 Low | 5 | 4 | **4** | → 安定 |
| **合計** | **18** | **13** | **11** | **-7 (61%削減)** |

### 品質メトリクス推移

| 指標 | 第1回 | 第2回 | 第3回 |
|:---|:---:|:---:|:---:|
| Clippy 警告 | 7 | 10 | **0** ✅ |
| テスト数 | 7 | 7 | **21** ✅ |
| FIXME | 3 | 3 | **0** ✅ |
| STUB | 4 | 4 | **0** ✅ |
| TODO | 2 | 2 | **2** |
| ライブラリ内 `eprintln!` | 18 | 0 | **0** ✅ |
| ライブラリ内 `unwrap()` | 5 | 4 | **8** ⚠️ |
| テスト内 `panic!` | 2 | 2 | **2** |

> **注**: ライブラリ内 `unwrap()` が 4→8 に増加していますが、新たに発見された `reconstruction.rs` の3箇所 (数学的安全) と `serializer.rs` の2箇所 (`Vec<u8>` I/O) はいずれも実行時にパニックしない保証があるものです。前回の監査では検出範囲外でした。

---

> **総括**: Critical/High が完全にゼロになり、Clippy 警告もゼロ、テストは3倍に増加しました。残存する Medium 7件はいずれも「数学的に安全な unwrap」「未実装の MCP ブリッジ」「テスト内の書き方」など、実運用上のリスクが低い項目です。修正は指示があるまで行いません。
