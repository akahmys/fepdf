# Acrobat 機能一覧と fepdf の対応表（2026-10-10）

`acrobat-features.toml` の827項目に fepdf の対応状況を付けたもの。判定はコミット `8eb9c77` 時点のCLIヘルプ、`Operation` の54種、`fepdf-mcp` のツール名、GUIの表示文言、ROADMAP.md から行った。項目ごとに動かして確かめたものではない。

| 記号 | 意味 |
| :-- | :-- |
| ✅ | 対応 |
| 🔶 | 一部対応 |
| ❌ | 未対応 |
| ⛔ | 方針として対象外（ADRかROADMAPに理由） |
| ❓ | 未確認 |
| ☁ | Adobeクラウド専用（一覧の tier N/A） |

## 集計

| 分野 | 項目数 | ✅ | 🔶 | ❌ | ⛔ | ❓ | ☁ |
| :-- | --: | --: | --: | --: | --: | --: | --: |
| A 基本 | 55 | 31 | 6 | 9 | 4 | 5 | 0 |
| B 表示 | 92 | 23 | 10 | 44 | 0 | 15 | 0 |
| C 編集 | 61 | 12 | 10 | 23 | 3 | 12 | 1 |
| D ページ整理 | 76 | 28 | 5 | 27 | 0 | 16 | 0 |
| E 注釈 | 69 | 19 | 4 | 34 | 0 | 8 | 4 |
| F フォーム | 81 | 21 | 9 | 29 | 5 | 14 | 3 |
| G 保護 | 42 | 14 | 7 | 14 | 0 | 6 | 1 |
| H 電子署名 | 47 | 5 | 5 | 30 | 1 | 3 | 3 |
| I OCR | 27 | 0 | 3 | 19 | 4 | 0 | 1 |
| J 作成と変換 | 35 | 3 | 2 | 25 | 2 | 2 | 1 |
| K 最適化と規格 | 58 | 5 | 3 | 41 | 4 | 5 | 0 |
| L 印刷 | 39 | 3 | 1 | 31 | 0 | 4 | 0 |
| M アクセシビリティ | 61 | 37 | 8 | 11 | 0 | 5 | 0 |
| N その他 | 84 | 8 | 7 | 51 | 6 | 3 | 9 |
| **合計** | **827** | **209** | **80** | **388** | **29** | **98** | **23** |
| うちP0 | 250 | 123 | 41 | 29 | 9 | 48 | 0 |

## A 基本

| 状態 | tier | id | 機能 | 根拠・備考 |
| :-: | :-: | :-- | :-- | :-- |
| ✅ | P0 | `core.open-pdf` | Open PDF files | Document::open（独自リーダー） |
| ✅ | P0 | `core.repair-broken-files` | Open damaged files by reconstructing the cross-reference table | 7.5: スキャンによる回復、オブジェクトストリーム展開 |
| ✅ | P0 | `core.malformed-input-safety` | Reject garbage or hostile input without crashing | Phase Z、ファザーを毎晩実行、clippyでindex/sliceのpanicを禁止 |
| ✅ | P0 | `core.repair-log` | Record repairs and show them to the user | Decision（条項付き）、inspect info / GUIの判断一覧 |
| ✅ | P0 | `core.xref-streams` | Read cross-reference streams and compressed object streams | 7.5 |
| ✅ | P0 | `core.hybrid-xref` | Hybrid-reference files (/XRefStm) and quirky xref subsections | 7.5: ハイブリッド参照 |
| ✅ | P0 | `core.revision-chain` | Follow /Prev revision chains with loop protection | 7.5: /Prev チェーン |
| ✅ | P0 | `core.stream-filters` | Standard stream filters (Flate, LZW, ASCII85, ASCIIHex, RunLength, predictors) | 7.4: 10種のうち9種＋予測子 |
| ✅ | P0 | `core.image-filters` | Image codecs in the filters crate (DCT, JPX, JBIG2, CCITT) | DCT、JPX/JBIG2/CCITT は hayro 経由（Phase M） |
| ✅ | P0 | `core.decompression-limits` | Bounded decompression (filter bomb protection) | filters/flate.rs, lzw.rs の上限 |
| ✅ | P0 | `core.open-rc4` | Open RC4-encrypted documents (40/128-bit) | 7.6 読み込み |
| ✅ | P0 | `core.open-aes` | Open AES-128 and AES-256 encrypted documents (R4-R6) | AES-128/256 R5/R6 |
| ✅ | P0 | `core.password-prompt` | Ask for the user or owner password, with retry | GUIのUnlockダイアログ、CLI --password |
| ✅ | P0 | `core.unicode-passwords` | Unicode passwords normalised with SASLprep | SASLprep |
| ✅ | P0 | `core.owner-password-unlock` | Owner password lifts document restrictions | 両方のパスワードで認証 |
| ⛔ | P0 | `core.permissions-enforced` | Honour document permissions (print, modify, assemble, copy) | /P は報告のみで強制しない（ROADMAP 7.6） |
| ✅ | P0 | `core.crypt-filters` | Crypt filters (Identity, /StmF, /StrF) | V4/V5 のクリプトフィルタ |
| ❓ | P1 | `core.attachment-only-encryption` | Attachment-only encryption (/EFF) opens without a password | /EFF の扱いは未確認 |
| ✅ | P1 | `core.unencrypted-metadata` | Leave XMP metadata readable when /EncryptMetadata is false | /EncryptMetadata（access.rs） |
| 🔶 | P1 | `core.unsupported-handlers` | Clear message for unsupported security handlers | inspect encryption が報告 |
| ✅ | P1 | `core.open-public-key` | Open certificate-encrypted (public-key handler) documents | 7.6.5 読み書き、--recipient-certificate |
| ⛔ | P0 | `core.incremental-save` | Save as an incremental update (original bytes kept, atomic write) | ADR-0014: 原本を保つ保存経路は作らない |
| ✅ | P0 | `core.save-as-full-rewrite` | Save as a new file with a full rewrite and garbage collection | 保存は常に全体の書き直し（ADR-0012） |
| ✅ | P0 | `core.object-stream-compression` | Compress objects into object streams behind an xref stream on full save | 既定でオブジェクトストリーム（ADR-0016） |
| ✅ | P0 | `core.encrypted-save` | Save encrypted documents re-encrypted under the same keys | AES-256 R6 のみ（ADR-0015） |
| ⛔ | P0 | `core.copy-on-write-fidelity` | Preserve untouched objects byte-for-byte (copy-on-write edits) | ADR-0014: 原本を保つ保存経路は作らない |
| ✅ | P0 | `core.write-validity` | Written files pass qpdf --check | Arlington に照らすテスト |
| ✅ | P2 | `core.deterministic-save` | Deterministic full-save mode | コアで非決定的なコレクションを禁止（RR-15） |
| ✅ | P1 | `core.linearization` | Linearized save (Fast Web View) | 書き出しウィザードの Fast Web View |
| 🔶 | P1 | `core.revision-list` | List document revisions from the xref chain | inspect structure がリビジョンを報告（閲覧は不可） |
| ❌ | P1 | `core.view-revision` | View an earlier revision as a read-only snapshot |  |
| ❓ | P0 | `core.lazy-loading` | Lazy loading of very large (GB) files within a memory budget |  |
| ✅ | P1 | `core.pdf20-utf8-strings` | PDF 2.0 UTF-8 text strings | 7.9.2.2 のテキスト文字列 |
| ✅ | P1 | `core.pdf20-associated-files` | PDF 2.0 associated files (/AF) | /AF、AttachAssociatedFile |
| ❌ | P2 | `core.pdf20-dpart` | PDF 2.0 document parts (DPart) | /DPartRoot はコーパスに0件、未モデル化 |
| ✅ | P1 | `core.pdf20-namespaces` | PDF 2.0 structure namespaces | SetStructNamespace |
| 🔶 | P1 | `core.arlington-validation` | Structural validation against the Arlington PDF model | テストでのみ（ユーザー向け機能ではない） |
| ✅ | P0 | `core.undo-redo` | Undo and redo every document edit | GUI Cmd+Z / Shift+Cmd+Z |
| ❓ | P1 | `core.undo-labels` | Edit menu names the step to undo or redo |  |
| ❓ | P1 | `core.undo-after-save` | Undo still works after saving |  |
| 🔶 | P1 | `core.atomic-batch-edits` | Multi-step edits are one undo step and all-or-nothing | ReorderBatch など一部の操作 |
| 🔶 | P2 | `core.history-panel` | History panel with named snapshots | ウィンドウが履歴を記録、パネルはなし |
| ❌ | P0 | `core.autosave` | Autosave changed documents periodically |  |
| ❌ | P0 | `core.crash-recovery` | Offer to recover unsaved work after a crash |  |
| ❌ | P0 | `core.encrypted-autosave` | Autosaves of encrypted documents stay encrypted |  |
| ❌ | P1 | `core.web-recovery` | Crash recovery in the web app |  |
| ✅ | P0 | `core.unsaved-changes-prompt` | Ask before closing or quitting with unsaved changes | 「書き出さずに閉じますか」 |
| ❓ | P1 | `core.dirty-indicator` | Unsaved-changes marker on document tabs |  |
| ✅ | P0 | `core.close-document` | Close a document | GUI |
| ❌ | P1 | `core.close-all` | Close all documents |  |
| ❌ | P1 | `core.revert` | Revert to the last saved version |  |
| 🔶 | P0 | `core.signed-save-policy` | Force incremental saves for signed documents | 署名が保存で残らないことを Decision で通知 |
| ✅ | P1 | `core.drag-drop-open` | Open files by drag and drop | GUI dropped_files |
| ❌ | P1 | `core.recent-files` | Local recent-files list |  |
| ⛔ | P3 | `core.usage-rights-preserve` | Preserve usage-rights (UR3) signatures from Reader-extended files | ADR-0014: 原本を保つ保存経路は作らない |

## B 表示

| 状態 | tier | id | 機能 | 根拠・備考 |
| :-: | :-: | :-- | :-- | :-- |
| ✅ | P0 | `view.render-pages` | Render pages (text, vectors, images) | fepdf-render（Vello/wgpu）、CLI publish render |
| ✅ | P0 | `view.render-shadings-patterns` | Shadings, gradients and tiling patterns | シェーディング1〜7、パターン |
| ✅ | P0 | `view.render-transparency` | Blend modes, soft masks and transparency groups | ブレンドモード、アルファ、ソフトマスク |
| ❌ | P1 | `view.render-knockout` | Knockout transparency groups | 11.6.6 の分離・ノックアウトは未読（記録のみ） |
| ❓ | P0 | `view.render-type3` | Type 3 fonts |  |
| ✅ | P0 | `view.render-cid-fonts` | CID fonts and CMaps incl. CJK without ToUnicode | CMap/CID、日本語縦書き |
| ✅ | P0 | `view.render-image-codecs` | JBIG2, JPEG 2000, CCITT and JPEG images | hayro 経由 |
| ✅ | P0 | `view.render-annotation-appearances` | Annotation and widget appearances incl. appearance states | ADR-0023 |
| ✅ | P0 | `view.render-error-isolation` | Per-page error isolation (a bad page never takes down the app) | デコードできない画像は飛ばして面積を記録 |
| ❓ | P0 | `view.render-priority` | Render visible pages first, then neighbours and thumbnails |  |
| 🔶 | P0 | `view.deep-zoom-tiles` | Tiled rendering keeps deep zoom sharp | タイル表示はある |
| ❓ | P0 | `view.layout-continuous` | Single page continuous scrolling |  |
| ✅ | P0 | `view.layout-single-page` | Single page view | 「1ページ」 |
| ✅ | P0 | `view.layout-two-up` | Two-page view | 「見開き」 |
| ❌ | P1 | `view.layout-two-up-scrolling` | Two-page scrolling |  |
| ❌ | P1 | `view.cover-page` | Show cover page in two-page view |  |
| ❌ | P2 | `view.page-gaps` | Show or hide gaps between pages |  |
| ❌ | P2 | `view.auto-scroll` | Automatic scrolling |  |
| ✅ | P0 | `view.zoom-in-out` | Zoom in and out around the cursor (keys, wheel, pinch) |  |
| ❓ | P0 | `view.zoom-to-percent` | Zoom to a chosen percentage |  |
| ✅ | P0 | `view.zoom-actual-size` | Actual size | 100%へリセット |
| ❓ | P0 | `view.zoom-fit-page` | Fit page |  |
| ❓ | P0 | `view.zoom-fit-width` | Fit width |  |
| ❌ | P1 | `view.zoom-fit-height` | Fit height |  |
| ❌ | P1 | `view.zoom-fit-visible` | Fit visible content |  |
| ❌ | P1 | `view.marquee-zoom` | Marquee zoom tool |  |
| ❌ | P2 | `view.dynamic-zoom` | Dynamic zoom (drag to zoom) |  |
| ❌ | P2 | `view.loupe` | Loupe magnifier tool |  |
| ❌ | P2 | `view.pan-and-zoom` | Pan and zoom overview window |  |
| ✅ | P0 | `view.hand-tool` | Hand tool for panning | ドラッグでパン |
| 🔶 | P0 | `view.rotate-view` | Rotate the view without changing the file | ページを90°回転（表示のみの回転かは未確認） |
| ❌ | P0 | `view.read-mode` | Read mode (hide tool chrome) |  |
| ❌ | P0 | `view.full-screen` | Full screen mode |  |
| ❌ | P2 | `view.full-screen-presentation` | Full-screen presentation options (auto-advance, loop, click to advance) |  |
| ❌ | P2 | `view.reflow` | Reflow text to the window width |  |
| ✅ | P0 | `view.page-navigation` | Page navigation (next, previous, first, last, go to page) | 最初/前/次/最後 |
| ❓ | P1 | `view.goto-page-label` | Go to a page by its label (logical page numbers) |  |
| ❓ | P1 | `view.view-history` | Previous view / next view |  |
| 🔶 | P0 | `view.follow-links` | Follow links and destinations | しおりからの移動は可、ページ上のリンクは未確認 |
| ❓ | P1 | `view.annotation-hover` | Show comment text on hover |  |
| ❌ | P1 | `view.document-tabs` | Tabbed documents |  |
| ✅ | P1 | `view.multiple-windows` | Multiple windows; drag a tab out into a window | 第2ウィンドウ |
| ❌ | P2 | `view.split-window` | Split window (two panes on one document) |  |
| ❌ | P2 | `view.spreadsheet-split` | Spreadsheet split (four synchronised panes) |  |
| ✅ | P0 | `view.thumbnails-panel` | Page thumbnails panel | ページ操作のサムネイル |
| ✅ | P1 | `view.thumbnail-context-ops` | Page operations from the thumbnails panel (drag reorder, context menu) | 回転・抽出・複製・分割など |
| ✅ | P0 | `view.bookmarks-panel` | Bookmarks panel with navigation | 閲覧と編集 |
| ❌ | P2 | `view.highlight-current-bookmark` | Highlight the current bookmark |  |
| 🔶 | P0 | `view.attachments-panel` | Attachments panel: list, open and save embedded files | サイドバーにファイル仕様の表示 |
| ✅ | P0 | `view.layers-panel` | Layers panel: toggle optional content visibility | /OCProperties に従うレイヤーパネル |
| ❌ | P2 | `view.layers-reset` | Reset layers to initial visibility; list for all or current pages |  |
| ❓ | P2 | `view.destinations-panel` | Named destinations panel |  |
| ❌ | P3 | `view.articles-panel` | Articles panel and article reading |  |
| ❌ | P3 | `view.model-tree` | 3D model tree panel |  |
| ❓ | P0 | `view.page-labels-display` | Show page labels (i, ii, 1, 2...) |  |
| 🔶 | P0 | `view.find` | Find text across the document | 墨消しスタジオ内の検索（正規表現・大小区別）、通常表示の検索は未確認 |
| ❓ | P0 | `view.find-next-previous` | Find next / previous match |  |
| ✅ | P1 | `view.find-options` | Find options: whole words, case-sensitive | 正規表現、大文字小文字 |
| ❌ | P2 | `view.find-in-comments-bookmarks` | Find in comments and bookmarks |  |
| ❌ | P1 | `view.find-results-panel` | Find results list |  |
| ❌ | P1 | `view.advanced-search` | Advanced search window |  |
| ❌ | P1 | `view.search-folder` | Search all PDFs in a folder |  |
| ❌ | P2 | `view.search-boolean` | Boolean, proximity and stemming search |  |
| ❌ | P2 | `view.search-index` | Build and search a full-text index across files |  |
| ❌ | P2 | `view.embedded-index` | Embedded search index |  |
| ❓ | P0 | `view.text-select` | Select text in reading order |  |
| 🔶 | P0 | `view.copy-text` | Copy selected text | スナップショットは画像としてコピー |
| ❓ | P1 | `view.select-all` | Select all text on a page |  |
| ❌ | P2 | `view.copy-with-layout` | Copy text keeping layout |  |
| ✅ | P1 | `view.snapshot` | Snapshot tool (copy a region as an image) | W-20: 範囲を96/192/384 DPIでクリップボードへ |
| ❌ | P2 | `view.rulers` | Rulers |  |
| ❌ | P2 | `view.grid` | Grid and snap to grid |  |
| ❌ | P2 | `view.guides` | Guides |  |
| ❌ | P2 | `view.line-weights` | Show line weights |  |
| 🔶 | P2 | `view.cursor-coordinates` | Cursor coordinates | CADキャリパー |
| ❌ | P1 | `view.theme-light-dark` | Light and dark themes |  |
| ❌ | P1 | `view.theme-system` | Follow the system theme |  |
| ❌ | P1 | `view.home-view` | Home view with recommended tools and recent files |  |
| 🔶 | P0 | `view.all-tools` | All tools catalogue with milestone badges | Document tools |
| ✅ | P0 | `view.command-palette` | Command palette (⌘K) over every command | Ctrl+K / Cmd+K |
| ✅ | P0 | `view.keyboard-shortcuts` | Platform keyboard shortcuts for registered commands |  |
| ❌ | P1 | `view.shortcuts-dialog` | Keyboard shortcuts reference dialog |  |
| ❌ | P0 | `view.menus-from-registry` | Menus generated from the command registry |  |
| ✅ | P2 | `view.about-dialog` | About dialog | About / クレジット |
| ❌ | P2 | `view.check-for-updates` | Help ▸ Check for updates |  |
| ❌ | P2 | `view.single-key-accelerators` | Single-key tool accelerators |  |
| ❌ | P2 | `view.customize-quick-tools` | Customise the quick-action toolbar and pinned tools |  |
| 🔶 | P0 | `view.restriction-notice` | Message bar for restricted documents | 権限を文書プロパティに表示 |
| ❌ | P2 | `view.pdfa-view-mode` | Read-only view for PDF/A files with an Enable editing bar |  |
| ❌ | P2 | `view.show-page-boxes` | Show art, trim and bleed boxes |  |
| 🔶 | P0 | `view.web-app` | Run in the browser (WebAssembly) | fepdf-wasm は読み取りとテキスト抽出のみ、描画不可 |
| ❓ | P0 | `view.large-document-performance` | Smooth scrolling on 500-page documents |  |

## C 編集

| 状態 | tier | id | 機能 | 根拠・備考 |
| :-: | :-: | :-- | :-- | :-- |
| ✅ | P0 | `edit.text-edit` | Edit existing text in place | EditRun（ラン単位）、W-E3/W-E4 |
| ⛔ | P0 | `edit.text-reflow` | Reflow edited text inside its paragraph box | ADR-0091: 段落を推測しない、はみ出しは表示する |
| ⛔ | P0 | `edit.paragraph-detection` | Detect paragraphs and text blocks for editing | ADR-0091 |
| ✅ | P0 | `edit.font-reuse-substitution` | Reuse embedded fonts or substitute with a warning | W-E1d: 埋め込み許可のある書体か拒否（ADR-0089/0090） |
| 🔶 | P0 | `edit.text-format-font` | Change font, size and colour | ランの書体を表示、変更は未確認 |
| ❌ | P1 | `edit.text-format-style` | Bold, italic, underline |  |
| ❌ | P2 | `edit.text-super-subscript` | Superscript and subscript |  |
| ❌ | P0 | `edit.paragraph-alignment` | Paragraph alignment (left, centre, right, justify) |  |
| ❌ | P1 | `edit.line-paragraph-spacing` | Line and paragraph spacing |  |
| ❓ | P1 | `edit.char-spacing-scale` | Character spacing and horizontal scaling |  |
| ❌ | P1 | `edit.lists` | Bulleted and numbered lists |  |
| 🔶 | P0 | `edit.add-text` | Add a new text box as page content | タイプライター/テキストボックス注釈、ヘッダー等の装飾 |
| 🔶 | P2 | `edit.find-replace` | Find and replace in edit mode | ランの置換（Replace） |
| 🔶 | P1 | `edit.rtl-cjk-editing` | Edit right-to-left and CJK text | CJKのフォント埋め込みは可（W-E1b2）、縦書き編集は未確認 |
| ❓ | P2 | `edit.rotated-text-editing` | Edit rotated text |  |
| ❓ | P1 | `edit.keep-tags-on-edit` | Keep structure tags when editing |  |
| 🔶 | P0 | `edit.image-add` | Add an image | スタンプ画像（JPEG） |
| ✅ | P0 | `edit.image-replace` | Replace an image (keep position and size) | EditXObject |
| 🔶 | P0 | `edit.image-crop` | Crop an image | ページのトリミングで画像も切られる（W-G1-b） |
| ✅ | P0 | `edit.image-rotate-flip` | Rotate and flip images | EditXObject: 回転 |
| ✅ | P0 | `edit.image-move-resize` | Move and resize images | EditXObject: 移動・拡大縮小 |
| ❌ | P1 | `edit.image-extract` | Save an image to a file |  |
| ❌ | P2 | `edit.image-external-editor` | Edit an image in an external application |  |
| ❌ | P1 | `edit.arrange-objects` | Arrange objects (bring forward, send backward) |  |
| ❌ | P1 | `edit.align-distribute-objects` | Align and distribute objects |  |
| ❌ | P1 | `edit.vector-edit` | Select, move, scale, recolour and delete vector paths |  |
| 🔶 | P1 | `edit.edit-object-tool` | Edit object tool for any page object | ランのドラッグ移動、XObject編集 |
| ❌ | P2 | `edit.object-properties` | Object properties (colour space, font, convert colour) |  |
| ✅ | P1 | `edit.link-create` | Create web and page links | AddAnnotation のリンク（ページ番号またはURL） |
| ❓ | P1 | `edit.link-properties` | Link appearance and properties |  |
| ❌ | P1 | `edit.links-from-urls` | Create links from URLs in the text |  |
| ❌ | P2 | `edit.remove-all-links` | Remove all links |  |
| ✅ | P1 | `edit.action-goto` | Go-to-page-view action (with named destinations) | リンク注釈 |
| ✅ | P1 | `edit.action-uri` | Open-a-web-link action | リンク注釈 |
| ❌ | P2 | `edit.action-launch` | Open-a-file action |  |
| ❌ | P2 | `edit.action-named` | Execute-a-menu-item action (safe list) |  |
| ❓ | P2 | `edit.action-set-ocg-state` | Set-layer-visibility action |  |
| ❓ | P1 | `edit.action-show-hide-field` | Show/hide-a-field action |  |
| 🔶 | P1 | `edit.action-javascript` | Run-a-JavaScript action | 実行は fepdf-script、作成は SetOpenAction のみ |
| ❌ | P3 | `edit.action-thread` | Read-an-article action |  |
| ❌ | P3 | `edit.action-goto-embedded` | Go to a target in an embedded file (GoToE) |  |
| ⛔ | P3 | `edit.action-media` | Play sound / media / rich-media actions | マルチメディアは対象外（13.4 非推奨） |
| ✅ | P0 | `edit.header-footer-add` | Add header and footer | AddPageDecoration |
| ❓ | P0 | `edit.header-footer-update-remove` | Update or remove header and footer |  |
| 🔶 | P0 | `edit.header-footer-tokens` | Page number and date tokens in header and footer | ページ番号の書式 |
| ✅ | P1 | `edit.header-footer-ranges` | Header and footer page ranges (all, range, odd, even) | 開始ページ、選択ページ |
| ❌ | P2 | `edit.header-footer-shrink` | Shrink document to avoid overwriting content |  |
| ✅ | P0 | `edit.watermark-add-text` | Add a text watermark | AddPageDecoration（/Subtype Watermark） |
| ❓ | P1 | `edit.watermark-add-file` | Add a watermark from an image or PDF page |  |
| ❓ | P0 | `edit.watermark-update-remove` | Update or remove watermarks |  |
| ❓ | P1 | `edit.watermark-appearance` | Watermark rotation, opacity, scale, position, print/screen visibility |  |
| 🔶 | P0 | `edit.background-add` | Add a page background (colour or file) | /Type Background の装飾 |
| ❓ | P0 | `edit.background-update-remove` | Update or remove backgrounds |  |
| ❌ | P2 | `edit.decoration-profiles` | Saved header/footer and watermark settings |  |
| ✅ | P1 | `edit.bates-numbering` | Bates numbering | ApplyBatesNumbering、CLI edit bates |
| ❌ | P1 | `edit.bates-multi-file` | Bates numbering across multiple files |  |
| ❓ | P1 | `edit.bates-remove` | Remove Bates numbering |  |
| ❌ | P2 | `edit.spell-check` | Spell check in comments, form fields and edited text |  |
| ❌ | P2 | `edit.spell-dictionaries` | Spelling dictionaries per language and a user dictionary |  |
| ❌ | P0 | `edit.format-panel` | Format panel for edited text |  |
| ☁ | N/A | `edit.adobe-express-edits` | Adobe Express image edits (remove background, generative fill) | Adobeクラウド専用 |

## D ページ整理

| 状態 | tier | id | 機能 | 根拠・備考 |
| :-: | :-: | :-- | :-- | :-- |
| 🔶 | P0 | `organize.organize-grid` | Organize pages thumbnail grid | サムネイルでのページ操作 |
| ✅ | P0 | `organize.multi-select-pages` | Select pages with click, ⌘-click and ⇧-click | 選択ページへの操作 |
| ✅ | P0 | `organize.rotate-pages` | Rotate pages | Rotate、CLI edit rotate |
| ✅ | P1 | `organize.rotate-filters` | Rotate by range with even/odd and orientation filters | 全ページ/偶数/奇数 |
| ✅ | P0 | `organize.delete-pages` | Delete pages | RemovePages |
| ✅ | P0 | `organize.move-pages` | Move pages earlier or later | Reorder |
| ❓ | P0 | `organize.drag-reorder` | Reorder pages by drag and drop |  |
| ❓ | P0 | `organize.insert-blank` | Insert blank pages |  |
| ✅ | P0 | `organize.insert-from-file` | Insert pages from another file | InsertFrom |
| ❌ | P1 | `organize.insert-from-clipboard` | Insert pages from the clipboard |  |
| ❌ | P2 | `organize.insert-from-scanner` | Insert pages from a scanner |  |
| ❌ | P2 | `organize.insert-from-web` | Insert pages from a web page |  |
| ❌ | P1 | `organize.insert-by-drop` | Insert by dropping files onto the page grid |  |
| ✅ | P0 | `organize.extract-pages` | Extract pages to a new document | CLI edit split、GUI「選択ページを抽出」 |
| ❓ | P1 | `organize.extract-separate-files` | Extract pages as separate files |  |
| ❌ | P1 | `organize.extract-and-delete` | Delete pages after extracting |  |
| ✅ | P0 | `organize.replace-pages` | Replace pages with pages from another file | 「ページを置換」 |
| ✅ | P0 | `organize.duplicate-pages` | Duplicate pages | DuplicatePages |
| ❌ | P2 | `organize.reverse-pages` | Reverse page order |  |
| ❌ | P1 | `organize.cut-copy-paste-pages` | Cut, copy and paste pages |  |
| ❌ | P2 | `organize.drag-pages-between-documents` | Drag pages between documents |  |
| ✅ | P1 | `organize.page-range-selection` | Select by range, odd/even, landscape/portrait | pages_named |
| ❌ | P2 | `organize.thumbnail-size` | Thumbnail size slider |  |
| ❓ | P0 | `organize.split-every-n` | Split every N pages | edit split は --pages の範囲指定 |
| 🔶 | P0 | `organize.split-before-pages` | Split before chosen pages | edit split --pages |
| ❌ | P1 | `organize.split-by-size` | Split by file size |  |
| ❌ | P1 | `organize.split-by-bookmarks` | Split by top-level bookmarks |  |
| ❌ | P2 | `organize.split-naming` | Name split parts after bookmarks; never overwrite |  |
| ❌ | P2 | `organize.split-multiple-files` | Split multiple files in one run |  |
| ✅ | P0 | `organize.combine-files` | Combine files into one PDF | CLI edit merge |
| ❓ | P0 | `organize.combine-bookmarks` | Combine adds a bookmark per file with its own bookmarks nested |  |
| ❌ | P1 | `organize.combine-choose-pages` | Choose which pages of each file to combine |  |
| ❌ | P2 | `organize.combine-sort` | Sort and reorder files before combining |  |
| ❌ | P2 | `organize.combine-options` | Combine options (file size, convert to PDF/A, include open files) |  |
| 🔶 | P0 | `organize.resource-dedupe` | Store identical fonts and images once when combining or inserting | フォントは文書内で1回（W-E1b4） |
| ❓ | P0 | `organize.preserve-links-dests` | Rewire links and named destinations to copied pages |  |
| ❓ | P0 | `organize.preserve-fields` | Keep form fields interactive when copying pages |  |
| ❓ | P0 | `organize.preserve-layers` | Keep layers and their default state when copying pages |  |
| ❓ | P0 | `organize.preserve-inherited` | Copy inherited page attributes with pages |  |
| ❓ | P1 | `organize.preserve-attachments` | Keep attachments when combining |  |
| ❓ | P1 | `organize.field-name-conflicts` | Rename or merge colliding field names when combining |  |
| ❓ | P1 | `organize.struct-tree-merge` | Merge structure trees when combining |  |
| ✅ | P1 | `organize.page-labels-edit` | Edit page labels (style, prefix, start) | SetPageLabels、CLI edit page-label |
| 🔶 | P1 | `organize.page-boxes` | Set page boxes (crop, trim, bleed, art) | トリミング |
| ✅ | P1 | `organize.crop-tool` | Crop pages by dragging a rectangle | CropPages、外側を削除（ADR-0088） |
| ❌ | P2 | `organize.remove-white-margins` | Remove white margins |  |
| ✅ | P2 | `organize.change-page-size` | Change page size | ResizePages |
| ❌ | P2 | `organize.page-transitions` | Page transitions |  |
| ✅ | P1 | `organize.page-tab-order` | Page tab order property | SetTabOrder |
| ❌ | P2 | `organize.page-actions` | Page open/close actions |  |
| ❌ | P3 | `organize.page-templates` | Page templates |  |
| ✅ | P0 | `organize.bookmark-add` | Add a bookmark (⌘B) | UpdateOutlines、GUIのしおり編集 |
| ✅ | P0 | `organize.bookmark-rename` | Rename bookmarks |  |
| ✅ | P0 | `organize.bookmark-delete` | Delete bookmarks |  |
| ✅ | P0 | `organize.bookmark-move` | Reorder and nest bookmarks | 上下・親子の移動 |
| ✅ | P0 | `organize.bookmark-set-destination` | Set a bookmark's destination | 行き先のページ |
| ❌ | P2 | `organize.bookmark-appearance` | Bookmark style and colour |  |
| ❓ | P1 | `organize.bookmark-actions` | Bookmark actions |  |
| ❓ | P1 | `organize.bookmark-expand-collapse` | Expand and collapse all bookmarks |  |
| ❌ | P1 | `organize.bookmarks-from-structure` | Generate bookmarks from headings or tags |  |
| ✅ | P2 | `organize.attachments-add` | Add file attachments | AttachAssociatedFile、CLI edit attach |
| ❓ | P2 | `organize.attachments-delete` | Delete attachments |  |
| ❌ | P2 | `organize.attachments-description` | Edit attachment descriptions |  |
| ✅ | P2 | `organize.associated-files` | Associated files with relationships (PDF/A-3, PDF 2.0) | /AF と /AFRelationship |
| ❌ | P2 | `organize.portfolio-view` | View PDF Portfolios as a file list |  |
| ❓ | P2 | `organize.portfolio-extract` | Extract files from a portfolio |  |
| ✅ | P3 | `organize.portfolio-create` | Create PDF Portfolios | CreatePortfolio、CLI edit portfolio |
| ✅ | P0 | `organize.document-properties` | Document properties dialog (description, fonts, advanced) | GUI 文書プロパティ、inspect info |
| ✅ | P0 | `organize.edit-metadata` | Edit title, author, subject and keywords | --title/--author/--lang/--copyright、--strip |
| ✅ | P1 | `organize.fonts-list` | List fonts with type and embedding status | 使用フォント一覧 |
| 🔶 | P1 | `organize.initial-view` | Edit initial view (layout, magnification, panels, window options) | SetOpenAction |
| ✅ | P2 | `organize.custom-properties` | Custom document properties | AddUserProperties |
| ❌ | P2 | `organize.xmp-editor` | Additional metadata (XMP) editor |  |
| ✅ | P1 | `organize.document-language` | Set document language and binding | --lang、構造要素の /Lang |
| ❌ | P2 | `organize.print-presets` | Print dialog presets in document properties |  |
| ❌ | P3 | `organize.base-url` | Base URL for relative links |  |

## E 注釈

| 状態 | tier | id | 機能 | 根拠・備考 |
| :-: | :-: | :-- | :-- | :-- |
| 🔶 | P0 | `comment.list-view` | Comments panel with threaded replies (read-only) | inspect interactive が注釈を列挙 |
| ✅ | P0 | `comment.appearance-generation` | Generate correct appearances for every markup type | W-8: 外観ストリームを書く |
| ✅ | P0 | `comment.sticky-note` | Sticky note comments | Note |
| ✅ | P0 | `comment.highlight` | Highlight text |  |
| ✅ | P0 | `comment.underline` | Underline text |  |
| ✅ | P0 | `comment.strikeout` | Strikethrough text |  |
| ✅ | P2 | `comment.squiggly` | Squiggly underline |  |
| ❌ | P1 | `comment.replace-text` | Replace-text proposal (strikeout + caret) |  |
| ❌ | P1 | `comment.insert-text` | Insert-text proposal (caret) |  |
| ✅ | P0 | `comment.typewriter` | Add text comment (typewriter) |  |
| ✅ | P0 | `comment.text-box` | Text box comment with rich text |  |
| ✅ | P1 | `comment.callout` | Callout comment |  |
| ✅ | P0 | `comment.pencil` | Freehand drawing (ink) | Ink |
| ❌ | P1 | `comment.eraser` | Eraser for ink strokes |  |
| ✅ | P0 | `comment.line` | Line |  |
| ❓ | P0 | `comment.arrow` | Arrow | 直線注釈の線端は未確認 |
| ✅ | P0 | `comment.rectangle` | Rectangle |  |
| ✅ | P0 | `comment.oval` | Oval | Ellipse |
| ❌ | P1 | `comment.polygon` | Polygon |  |
| ❌ | P1 | `comment.polyline` | Connected lines (polyline) |  |
| ❌ | P1 | `comment.cloud` | Cloud shape |  |
| ❌ | P1 | `comment.area-highlight` | Area highlight |  |
| ❌ | P1 | `comment.attach-file-comment` | Attach a file as a comment |  |
| ❌ | P3 | `comment.record-audio` | Sound comments |  |
| ❌ | P2 | `comment.image-comment` | Add an image as a comment |  |
| ❓ | P1 | `comment.line-endings` | All ten line-ending styles and dashed/cloudy borders |  |
| 🔶 | P0 | `comment.properties-dialog` | Comment properties (appearance, author, subject) | 色・太さ |
| ❌ | P1 | `comment.properties-bar` | Comment properties bar |  |
| ❌ | P1 | `comment.lock-comment` | Lock comments |  |
| ❌ | P1 | `comment.default-properties` | Make current properties the tool default |  |
| ❌ | P1 | `comment.author-identity` | Author name from identity preferences |  |
| 🔶 | P0 | `comment.reply` | Reply to comments | /IRT は読む、返信の作成は未確認 |
| ❌ | P0 | `comment.set-status` | Set review status (accepted, rejected, cancelled, completed) |  |
| ❌ | P1 | `comment.checkmark` | Mark comments with a checkmark |  |
| ❓ | P0 | `comment.delete-comment` | Delete comments |  |
| ❓ | P0 | `comment.edit-comment-text` | Edit comment text |  |
| ❓ | P1 | `comment.copy-comment-text` | Copy comment text |  |
| ❌ | P0 | `comment.filter-comments` | Filter comments (type, author, status, colour, checkmark) |  |
| ❌ | P0 | `comment.sort-comments` | Sort comments (page, author, date, type, colour) |  |
| ❌ | P1 | `comment.search-comments` | Search comments |  |
| ❓ | P1 | `comment.show-hide-comments` | Show or hide all comments and pop-ups |  |
| ❓ | P1 | `comment.popups` | Open and reposition comment pop-ups |  |
| ❌ | P2 | `comment.connector-lines` | Connector lines between markups and pop-ups |  |
| ❌ | P2 | `comment.review-history` | Review history of state changes |  |
| 🔶 | P1 | `comment.stamps-standard` | Standard business stamps | スタンプ（画像） |
| ❌ | P1 | `comment.stamps-sign-here` | Sign-here stamps |  |
| ❌ | P1 | `comment.stamps-dynamic` | Dynamic stamps (name and date) |  |
| ✅ | P1 | `comment.stamps-custom` | Custom stamps from images or PDF pages | 画像を選んでスタンプ |
| ❌ | P2 | `comment.stamps-manage` | Manage stamp categories and imports |  |
| ❌ | P2 | `comment.stamps-rotate` | Rotate stamps with a handle |  |
| ❌ | P1 | `comment.import-fdf-xfdf` | Import comments from FDF/XFDF |  |
| ❌ | P1 | `comment.export-fdf-xfdf` | Export all comments to FDF/XFDF |  |
| ❌ | P2 | `comment.export-selected` | Export selected comments |  |
| ❌ | P2 | `comment.import-from-pdf` | Import comments from another PDF |  |
| ❌ | P1 | `comment.summarize` | Summarize comments into a new PDF |  |
| ❓ | P1 | `comment.flatten` | Flatten comments into page content |  |
| ❌ | P3 | `comment.migrate` | Migrate comments to a revised document |  |
| ✅ | P2 | `comment.measure-distance` | Distance measuring tool | W-16 |
| ✅ | P2 | `comment.measure-perimeter` | Perimeter measuring tool | W-16 |
| ✅ | P2 | `comment.measure-area` | Area measuring tool | W-16 |
| ✅ | P2 | `comment.measure-scale` | Scale ratio and calibration per viewport | SetMeasurementScale |
| ❌ | P2 | `comment.measure-snap` | Snap to paths, endpoints, midpoints and intersections |  |
| ❌ | P2 | `comment.measure-info-panel` | Measurement info panel |  |
| ❌ | P3 | `comment.measure-export` | Export measurements as CSV |  |
| ✅ | P2 | `comment.geospatial` | Geospatial location and measuring | SetGeospatialAnchor、CLI edit geo |
| ☁ | N/A | `comment.shared-review` | Send for comments / shared review | Adobeクラウド専用 |
| ☁ | N/A | `comment.mentions-live-review` | @mentions, presence and live co-review | Adobeクラウド専用 |
| ☁ | N/A | `comment.review-tracker` | Review tracker | Adobeクラウド専用 |
| ☁ | N/A | `comment.enable-reader-commenting` | Enable commenting and measuring in Reader (usage rights) | Adobeクラウド専用 |

## F フォーム

| 状態 | tier | id | 機能 | 根拠・備考 |
| :-: | :-: | :-- | :-- | :-- |
| ✅ | P0 | `form.field-list` | Form fields panel listing every field and value | GUI フォームサイドバー、inspect interactive |
| ❓ | P0 | `form.highlight-fields` | Highlight existing fields |  |
| ✅ | P0 | `form.field-appearance-display` | Display checkbox, radio, list and signature widgets as designed |  |
| ✅ | P0 | `form.fill-text` | Fill text fields | SetFormFieldValue（外観を生成） |
| ✅ | P0 | `form.fill-checkbox-radio` | Toggle checkboxes and radio buttons (radios in unison) |  |
| ✅ | P0 | `form.fill-choice` | Choose from combo boxes and list boxes | ADR-0048 |
| ❓ | P0 | `form.push-buttons` | Push buttons run their actions |  |
| ❓ | P0 | `form.field-navigation` | Tab between fields |  |
| ✅ | P0 | `form.appearance-regeneration` | Regenerate field appearances (auto-size, comb, multiline, password) | 値の設定で外観を作る（12.7.4.3） |
| ❌ | P2 | `form.rich-text-fields` | Rich-text field values |  |
| ✅ | P0 | `form.af-number` | Number and currency formatting (AFNumber) | fepdf-script aform.js |
| ✅ | P0 | `form.af-percent` | Percentage formatting | aform.js |
| ✅ | P0 | `form.af-date-time` | Date and time formatting and parsing | aform.js |
| ❓ | P0 | `form.af-special` | Special formats (zip, phone, SSN, arbitrary mask) |  |
| ❓ | P0 | `form.af-range-validate` | Range validation |  |
| ✅ | P0 | `form.af-simple-calculate` | Sum, product, average, min, max calculations | aform.js |
| ❓ | P0 | `form.simplified-notation` | Simplified field notation calculations |  |
| ✅ | P0 | `form.custom-scripts` | Custom format, keystroke, validate and calculate scripts | boa による ECMAScript（ADR-0026） |
| ✅ | P0 | `form.calculation-order` | Calculation order | SetCalculationOrder |
| ❓ | P0 | `form.event-order` | Keystroke, validate, calculate, format event order |  |
| ❌ | P1 | `form.date-picker` | Date picker for date fields |  |
| ❌ | P2 | `form.auto-complete` | Auto-complete from previous entries |  |
| ❌ | P1 | `form.required-fields` | Required-field highlighting |  |
| ❓ | P0 | `form.reset-form` | Clear or reset a form | ResetForm アクションは読む |
| ❓ | P1 | `form.submit-form` | Submit form to a URL or email (with a prompt) | SubmitForm アクションは読む |
| ❌ | P1 | `form.import-data` | Import form data (FDF, XFDF) |  |
| ❌ | P1 | `form.export-data` | Export form data (FDF, XFDF) |  |
| ❌ | P1 | `form.data-xml-csv-txt` | Form data as XML, CSV or tab-delimited text |  |
| ❌ | P1 | `form.merge-to-spreadsheet` | Merge form data files into a spreadsheet |  |
| ❓ | P1 | `form.flatten-fields` | Flatten form fields |  |
| 🔶 | P0 | `form.prepare-form-mode` | Prepare a form mode | フォーム作成は MCP／操作から（GUIのフォーム作成モードは未確認） |
| ✅ | P0 | `form.add-text-field` | Add text fields | AddFormField（9種） |
| ✅ | P0 | `form.add-checkbox` | Add checkboxes |  |
| ✅ | P0 | `form.add-radio` | Add radio buttons |  |
| ✅ | P0 | `form.add-dropdown` | Add drop-down lists | combo_box |
| ✅ | P0 | `form.add-listbox` | Add list boxes |  |
| ✅ | P0 | `form.add-button` | Add push buttons | push_button |
| ❌ | P1 | `form.add-image-field` | Add image fields |  |
| ❓ | P0 | `form.add-date-field` | Add date fields |  |
| ✅ | P0 | `form.add-signature-field` | Add digital signature fields | 署名欄の配置 |
| ❌ | P2 | `form.add-barcode-field` | Barcode fields (PDF417, QR, Data Matrix) |  |
| 🔶 | P0 | `form.props-general` | Field properties: general (name, tooltip, visibility, read-only, required) | AddFormField の引数 |
| ❓ | P0 | `form.props-appearance` | Field properties: appearance (border, fill, font) |  |
| 🔶 | P1 | `form.props-position` | Field properties: position and size | AddFormField の矩形 |
| 🔶 | P0 | `form.props-options` | Field properties: options per field type | AddFormField の引数（種類別） |
| ❓ | P1 | `form.props-actions` | Field properties: actions per trigger |  |
| 🔶 | P0 | `form.props-format` | Field properties: format | AForm 書式スクリプトを実行 |
| ❓ | P0 | `form.props-validate` | Field properties: validate |  |
| 🔶 | P0 | `form.props-calculate` | Field properties: calculate | 計算スクリプトを実行、SetCalculationOrder |
| ❌ | P2 | `form.props-selection-change` | Field properties: selection change |  |
| ❌ | P1 | `form.props-signed` | Field properties: signed (lock fields when signed) |  |
| ❌ | P1 | `form.align-distribute-fields` | Align, centre, distribute and match field sizes |  |
| ❌ | P1 | `form.duplicate-fields` | Duplicate fields across pages |  |
| ❌ | P2 | `form.multiple-copies` | Create multiple copies of a field in a grid |  |
| ❌ | P1 | `form.rename-lock-fields` | Rename and lock fields |  |
| ✅ | P0 | `form.tab-order` | Tab order (row, column, structure, manual) | SetTabOrder（行/列/構造順） |
| ❌ | P2 | `form.show-tab-numbers` | Show tab numbers |  |
| ❌ | P1 | `form.preview-form` | Preview mode for testing a form |  |
| ❌ | P1 | `form.auto-detect-fields` | Automatic form field detection |  |
| ❌ | P1 | `form.auto-name-fields` | Name new fields from nearby labels |  |
| 🔶 | P1 | `form.document-javascripts` | Document-level JavaScripts | 文書スクリプトを実行（ADR-0026）、編集は不可 |
| 🔶 | P1 | `form.document-actions` | Document actions (will close, will save, did save, will print, did print) | SetOpenAction |
| ✅ | P0 | `form.js-host` | JavaScript engine with the Acrobat object model subset | app/this/Field/event/util/color |
| ❌ | P1 | `form.js-console` | JavaScript console |  |
| ❌ | P1 | `form.js-debugger` | JavaScript debugger |  |
| ❌ | P2 | `form.edit-all-javascripts` | Edit all JavaScripts in one editor |  |
| 🔶 | P0 | `form.js-security` | JavaScript enable switch and sandboxing | inspect actions が開くだけで動くコードを報告 |
| ❌ | P3 | `form.folder-level-js` | Folder-level JavaScript |  |
| ⛔ | P1 | `form.xfa-static` | View and fill static XFA forms | XFA は対象外（2.0で非推奨） |
| ⛔ | P1 | `form.xfa-dynamic` | View and fill dynamic XFA forms | 同上 |
| ⛔ | P1 | `form.xfa-formcalc` | FormCalc scripting | 同上 |
| ⛔ | P2 | `form.xfa-data` | XFA XML data import and export | 同上 |
| ⛔ | P2 | `form.xfa-flatten` | Flatten XFA forms | 同上 |
| ❌ | P0 | `form.fill-sign-text` | Fill & Sign: add text on flat forms |  |
| ❌ | P0 | `form.fill-sign-marks` | Fill & Sign: check, cross, dot, line marks |  |
| ❌ | P0 | `form.fill-sign-date` | Fill & Sign: today's date |  |
| ❌ | P0 | `form.fill-sign-signature` | Fill & Sign: typed, drawn or image signature and initials |  |
| ❌ | P0 | `form.fill-sign-flatten` | Fill & Sign: flatten on save |  |
| ☁ | N/A | `form.autofill-profile` | Autofill collection synced to an Adobe account | Adobeクラウド専用 |
| ☁ | N/A | `form.distribute-track` | Distribute and track forms, collect responses | Adobeクラウド専用 |
| ☁ | N/A | `form.web-forms` | Acrobat Sign web forms | Adobeクラウド専用 |

## G 保護

| 状態 | tier | id | 機能 | 根拠・備考 |
| :-: | :-: | :-- | :-- | :-- |
| ✅ | P0 | `protect.security-properties` | Security properties (method, which password opened it, each permission) | inspect encryption、GUIのセキュリティ表示 |
| ✅ | P0 | `protect.password-to-open` | Protect with a password to open | AES-256、--encrypt-password |
| ✅ | P0 | `protect.permissions-password` | Restrict editing and printing with a permissions password | 所有者パスワード |
| ✅ | P0 | `protect.permissions-matrix` | Choose printing, changes, copying and accessibility permissions | 書き出しウィザードの権限 |
| 🔶 | P1 | `protect.encryption-level` | Choose encryption compatibility (RC4 40/128, AES-128, AES-256) | AES-256 R6 のみ（ADR-0015） |
| ❓ | P1 | `protect.encrypt-except-metadata` | Encrypt everything except metadata |  |
| ❌ | P2 | `protect.encrypt-attachments-only` | Encrypt only file attachments |  |
| ❌ | P2 | `protect.password-strength` | Password strength meter |  |
| ✅ | P0 | `protect.remove-security` | Remove security | 保護なしで書き出す |
| ✅ | P1 | `protect.certificate-encryption` | Encrypt with certificates for recipients | --encrypt-to |
| ❓ | P1 | `protect.recipient-permissions` | Per-recipient permissions for certificate security |  |
| ❌ | P3 | `protect.recipient-ldap` | Find recipients in a directory (LDAP) |  |
| ❌ | P2 | `protect.security-policies` | Local security policies (saved presets) |  |
| ☁ | N/A | `protect.rights-management` | Server-based rights management (AEM/LiveCycle) | Adobeクラウド専用 |
| ❌ | P3 | `protect.mip-labels` | Open Microsoft Purview-protected PDFs and apply labels |  |
| ✅ | P0 | `protect.redact-text` | Mark text for redaction | Redact、墨消しスタジオ |
| ✅ | P0 | `protect.redact-area` | Mark areas and images for redaction |  |
| ❓ | P0 | `protect.redact-pages` | Mark whole pages for redaction |  |
| ✅ | P0 | `protect.search-and-redact` | Find text and redact | 検索して墨消し |
| 🔶 | P1 | `protect.redact-word-lists` | Redact lists of words or phrases | 正規表現の選択（|）で代用 |
| ✅ | P0 | `protect.redact-patterns` | Redact patterns (phone, email, card, SSN, dates) | 正規表現 |
| ❌ | P1 | `protect.redact-pattern-locales` | Pattern sets per locale |  |
| ❓ | P2 | `protect.redact-partial-words` | Redact part of a match (e.g. all but the last four digits) |  |
| ❌ | P2 | `protect.redact-folder` | Search and redact across a folder |  |
| 🔶 | P0 | `protect.redaction-properties` | Redaction fill colour and overlay text | 塗りつぶし色 |
| ❌ | P1 | `protect.redaction-codes` | Redaction code sets (e.g. FOIA exemptions) |  |
| ✅ | P0 | `protect.apply-redactions` | Apply redactions (remove underlying content) | ApplyRedactAnnotations |
| ✅ | P0 | `protect.redact-glyph-removal` | Remove redacted text glyphs from content streams |  |
| ✅ | P0 | `protect.redact-image-pixels` | Remove redacted image pixels |  |
| ✅ | P0 | `protect.redact-vectors` | Remove or cut redacted vector paths |  |
| 🔶 | P0 | `protect.redact-annots-fields` | Remove annotations and fields under redactions | redact_annots.rs |
| ❓ | P0 | `protect.redact-struct-tree` | Remove redacted content from tags and alternate text |  |
| ❓ | P0 | `protect.redaction-verification` | Verify no residue remains after redaction |  |
| ❌ | P3 | `protect.redact-filename-suffix` | Add a suffix to redacted file names |  |
| ❌ | P2 | `protect.redact-ai-suggestions` | Suggest sensitive content to redact via an AI provider |  |
| 🔶 | P1 | `protect.remove-hidden-information` | Remove hidden information (selectable categories) | --strip でメタデータ削除 |
| 🔶 | P1 | `protect.sanitize` | Sanitize document (remove all hidden information) | サニタイズ（mcp redact、書き出しウィザード） |
| ❌ | P3 | `protect.remove-hidden-on-close` | Remove hidden information when closing or sending |  |
| ❌ | P1 | `protect.attachment-trust` | Block opening risky attachment types |  |
| 🔶 | P1 | `protect.url-access-control` | Control internet access from PDFs | inspect actions が外部アクセスするアクションを報告 |
| ❌ | P2 | `protect.protected-view` | Open untrusted files in a restricted view |  |
| ❌ | P3 | `protect.security-envelope` | Security envelopes |  |

## H 電子署名

| 状態 | tier | id | 機能 | 根拠・備考 |
| :-: | :-: | :-- | :-- | :-- |
| ❌ | P0 | `sign.pkcs12` | Sign with a PKCS#12 (.p12/.pfx) digital ID | 鍵は DER PKCS#8 のみ |
| ❌ | P0 | `sign.macos-keychain` | Sign with macOS Keychain identities |  |
| ❌ | P0 | `sign.windows-cert-store` | Sign with the Windows certificate store |  |
| ❌ | P1 | `sign.pkcs11` | Sign with PKCS#11 tokens and smart cards |  |
| ❌ | P1 | `sign.self-signed-id` | Create a self-signed digital ID |  |
| ❌ | P2 | `sign.export-certificate` | Export a public certificate |  |
| 🔶 | P0 | `sign.visible-signature` | Visible signature by drawing a rectangle | 署名欄を配置（自分の出力のみ） |
| ⛔ | P0 | `sign.sign-existing-field` | Sign an existing signature field | 他者の文書への署名は ADR-0014 で対象外 |
| ❓ | P1 | `sign.invisible-signature` | Invisible signature |  |
| ❓ | P1 | `sign.signature-appearance` | Configure signature appearance (text, graphic, name) |  |
| ✅ | P1 | `sign.reason-location` | Reason, location and contact info | SignOptions の reason/location/contact_info |
| ❌ | P1 | `sign.lock-after-signing` | Lock document or fields after signing |  |
| ✅ | P0 | `sign.pkcs7-detached` | PKCS#7 detached signatures | ETSI.CAdES.detached |
| ✅ | P0 | `sign.cades-pades-bb` | CAdES / PAdES B-B signatures | PAdES |
| ❓ | P0 | `sign.hash-algorithms` | SHA-256/384/512 hashes; no SHA-1 for new signatures |  |
| ❌ | P1 | `sign.certify` | Certify a document (DocMDP) | DocMDP /Perms は未着手（ROADMAP P3） |
| ❌ | P1 | `sign.certify-permissions` | Allowed changes after certifying |  |
| ❌ | P1 | `sign.field-mdp` | Field locks (FieldMDP) |  |
| ❌ | P1 | `sign.document-timestamp` | Add a document timestamp (RFC 3161) |  |
| ❌ | P1 | `sign.timestamp-servers` | Configure timestamp servers |  |
| ❌ | P1 | `sign.pades-bt` | PAdES B-T (signature timestamp) | タイムスタンプなし |
| 🔶 | P1 | `sign.ltv` | Long-term validation (DSS/VRI) | AddLtvInfo（証明書の格納のみ、OCSP/CRL なし） |
| ❌ | P1 | `sign.pades-blta` | PAdES B-LT and B-LTA |  |
| ❌ | P1 | `sign.ocsp-crl` | Revocation checking (OCSP and CRL) |  |
| ✅ | P0 | `sign.validate-all` | Validate all signatures | verify-signature、GUIの署名一覧 |
| ✅ | P0 | `sign.signatures-panel` | Signatures panel with per-revision tree | GUI Signatures |
| 🔶 | P0 | `sign.status-icons` | Signature status (valid, unknown, invalid) | 「verifies」と範囲の表示 |
| ❌ | P0 | `sign.validation-banner` | Signed/certified document message bar |  |
| 🔶 | P0 | `sign.signature-properties` | Signature properties (validity summary, signer, time) | 署名者・時刻・範囲 |
| ❌ | P1 | `sign.certificate-viewer` | Certificate viewer |  |
| ❌ | P0 | `sign.trust-store` | Trusted certificates store | 信頼判断なし（README） |
| ❌ | P1 | `sign.trust-import` | Import trusted certificates and set trust |  |
| ❌ | P2 | `sign.eutl` | EU Trusted Lists |  |
| ❌ | P1 | `sign.os-trust` | Trust the operating system's root certificates |  |
| ❌ | P1 | `sign.verification-time` | Verification time (signing time, timestamp, now) | 有効期間を確認しない |
| 🔶 | P0 | `sign.changes-after-signing` | List changes made after signing (MDP-aware diff) | 署名がファイル全体を覆うかを報告 |
| ❌ | P1 | `sign.view-signed-version` | View the signed version |  |
| ❌ | P1 | `sign.compare-signed-version` | Compare signed version with the current one |  |
| ❌ | P1 | `sign.verify-on-open` | Verify signatures when a document opens |  |
| ❌ | P2 | `sign.clear-signature` | Clear an unsaved signature; clear all signature fields |  |
| ❌ | P2 | `sign.seed-values` | Signature seed values |  |
| ❌ | P2 | `sign.preview-mode` | Preview document mode before signing |  |
| ❌ | P2 | `sign.qualified-labels` | Show qualified signature/seal labels |  |
| ❌ | P1 | `sign.web-signing` | Sign in the browser with a PKCS#12 file | wasm に署名なし |
| ☁ | N/A | `sign.aatl` | Adobe Approved Trust List | Adobeクラウド専用 |
| ☁ | N/A | `sign.request-esignatures` | Request e-signatures (Acrobat Sign) | Adobeクラウド専用 |
| ☁ | N/A | `sign.cloud-signatures` | Cloud signature (CSC) digital IDs via Adobe | Adobeクラウド専用 |

## I OCR

| 状態 | tier | id | 機能 | 根拠・備考 |
| :-: | :-: | :-- | :-- | :-- |
| ⛔ | P0 | `ocr.recognize-text` | Recognize text (OCR) in this file | OCRエンジンは作らない（ADR-0086） |
| ⛔ | P0 | `ocr.recognize-range` | Recognize text in the current page or a range | ADR-0086 |
| ⛔ | P1 | `ocr.recognize-multiple` | Recognize text in multiple files | ADR-0086 |
| ⛔ | P0 | `ocr.ocr-languages` | OCR document language selection | ADR-0086 |
| 🔶 | P0 | `ocr.searchable-image` | Searchable image output (invisible text layer) | 外部OCRの結果を AddTextLayer で重ねる（W-O1） |
| 🔶 | P1 | `ocr.searchable-image-exact` | Searchable image (exact): leave the image untouched | AddTextLayer（モード3） |
| ❌ | P1 | `ocr.editable-text-output` | Editable text and images output |  |
| ❌ | P2 | `ocr.ocr-downsample` | Downsample images during OCR |  |
| ❌ | P1 | `ocr.skip-renderable-text` | Skip pages that already have text |  |
| ❌ | P1 | `ocr.rotation-detection` | Detect and fix page rotation |  |
| ❌ | P1 | `ocr.deskew` | Deskew |  |
| ❌ | P1 | `ocr.despeckle` | Despeckle |  |
| ❌ | P1 | `ocr.background-removal` | Background removal |  |
| ❌ | P2 | `ocr.descreen` | Descreen |  |
| ❌ | P3 | `ocr.remove-halo` | Remove halos |  |
| ❌ | P2 | `ocr.edge-shadow-removal` | Edge shadow removal |  |
| ❌ | P1 | `ocr.camera-perspective` | Enhance camera images (edges, perspective, contrast) |  |
| ❌ | P2 | `ocr.camera-document-type` | Camera image document types (whiteboard, card, form) |  |
| ❌ | P2 | `ocr.correct-recognized-text` | Correct recognized text (low-confidence suspects) |  |
| ❌ | P2 | `ocr.adaptive-compression` | Adaptive (MRC) compression of scans |  |
| ❌ | P2 | `ocr.scan-pdfa` | Make scans PDF/A compliant |  |
| 🔶 | P1 | `ocr.ocr-engines` | Pluggable OCR engines (ocrs default; tesseract optional) | page_for_ocr で外部エンジンに渡す |
| ❌ | P1 | `ocr.ocr-web` | OCR in the browser |  |
| ❌ | P2 | `ocr.scanner-acquire` | Scan from TWAIN/WIA/ICA/SANE scanners |  |
| ❌ | P2 | `ocr.scan-presets` | Scan presets |  |
| ❌ | P2 | `ocr.scan-append` | Append scans to an existing PDF or split into files |  |
| ☁ | N/A | `ocr.adobe-scan-sync` | Adobe Scan mobile capture sync | Adobeクラウド専用 |

## J 作成と変換

| 状態 | tier | id | 機能 | 根拠・備考 |
| :-: | :-: | :-- | :-- | :-- |
| ❓ | P0 | `create.from-images` | Create PDF from images (JPEG, PNG, TIFF, GIF, BMP, JPEG 2000) |  |
| ❌ | P0 | `create.from-multipage-tiff` | Multi-page TIFF to multi-page PDF |  |
| ❌ | P0 | `create.from-text` | Create PDF from plain text |  |
| ❌ | P2 | `create.from-markdown` | Create PDF from Markdown |  |
| ❌ | P0 | `create.from-clipboard` | Create PDF from the clipboard |  |
| ❌ | P1 | `create.from-screenshot` | Create PDF from a screen capture |  |
| ❓ | P0 | `create.blank` | Create a blank PDF |  |
| ❌ | P2 | `create.from-html` | Create PDF from an HTML file |  |
| ❌ | P2 | `create.from-web-page` | Create PDF from a web page URL |  |
| ⛔ | P2 | `create.from-office` | Create PDF from Office documents (LibreOffice sidecar) | DOCX変換は対象外（ROADMAP Not planned） |
| ❌ | P1 | `create.create-multiple` | Convert many files to PDF in one run |  |
| ❌ | P2 | `create.image-conversion-options` | Image compression options when converting |  |
| ❌ | P3 | `create.pdf-printer` | Print to PDF from other applications |  |
| ❌ | P3 | `create.distiller` | PostScript/EPS to PDF (Distiller) |  |
| ❌ | P3 | `create.job-options` | PDF settings presets (job options) |  |
| ⛔ | P1 | `create.export-docx` | Export to Word (.docx) | 同上 |
| ❌ | P1 | `create.export-rtf` | Export to RTF |  |
| ❌ | P1 | `create.export-xlsx` | Export to Excel (.xlsx) with table detection |  |
| ❌ | P1 | `create.export-pptx` | Export to PowerPoint (.pptx) |  |
| ✅ | P0 | `create.export-png` | Export pages as PNG | publish render、ページを画像で書き出し |
| 🔶 | P0 | `create.export-jpeg-tiff` | Export pages as JPEG or TIFF | JPEG可、TIFFなし |
| ❌ | P2 | `create.export-jpeg2000` | Export pages as JPEG 2000 |  |
| ❌ | P1 | `create.export-all-images` | Export all embedded images |  |
| ❌ | P1 | `create.export-html` | Export to HTML |  |
| ✅ | P0 | `create.export-text` | Export to plain text | inspect text |
| 🔶 | P1 | `create.export-accessible-text` | Export accessible text (from tags) | 構造順の読み上げ順（W-19a） |
| ❌ | P2 | `create.export-xml` | Export to XML |  |
| ❌ | P1 | `create.export-svg` | Export to SVG |  |
| ❌ | P2 | `create.export-ps` | Export to PostScript |  |
| ❌ | P2 | `create.export-eps` | Export to EPS |  |
| ❌ | P1 | `create.export-ocr` | Run OCR automatically when exporting scans |  |
| ❌ | P2 | `create.export-settings` | Per-format export settings |  |
| ✅ | P0 | `create.text-extraction` | Extract text in reading order (columns, RTL, CJK) | 1,632万グリフ中1,137欠落（ROADMAP 9） |
| ❌ | P3 | `create.translate` | Translate documents |  |
| ☁ | N/A | `create.cloud-office-conversion` | Cloud conversion from the macOS Office add-in | Adobeクラウド専用 |

## K 最適化と規格

| 状態 | tier | id | 機能 | 根拠・備考 |
| :-: | :-: | :-- | :-- | :-- |
| 🔶 | P0 | `optimize.reduce-file-size` | Reduce file size (one click) | オブジェクトストリームと Flate 圧縮 |
| ⛔ | P1 | `optimize.compatibility-level` | Make compatible with a chosen PDF version | 出力は常に PDF 2.0（ADR-0014） |
| ❌ | P1 | `optimize.optimizer` | Advanced optimization (PDF Optimizer) |  |
| ❌ | P1 | `optimize.downsample-images` | Downsample colour, grey and mono images |  |
| ❌ | P1 | `optimize.recompress-images` | Recompress images (JPEG, JPEG 2000, Flate) |  |
| ❌ | P1 | `optimize.mono-compression` | Monochrome compression (JBIG2, CCITT G4) |  |
| ❌ | P1 | `optimize.only-if-smaller` | Optimize images only if smaller |  |
| ❌ | P2 | `optimize.unembed-fonts` | Unembed fonts |  |
| ❌ | P1 | `optimize.flatten-transparency` | Flatten transparency |  |
| ❓ | P1 | `optimize.discard-javascript` | Discard JavaScript actions | サニタイズの範囲は未確認 |
| ❌ | P1 | `optimize.discard-thumbnails` | Discard embedded thumbnails |  |
| ❌ | P2 | `optimize.discard-tags` | Discard document tags |  |
| ❌ | P2 | `optimize.discard-bookmarks` | Discard bookmarks |  |
| ❌ | P2 | `optimize.discard-search-index` | Discard embedded search index |  |
| ❌ | P2 | `optimize.discard-print-settings` | Discard embedded print settings |  |
| ❌ | P2 | `optimize.discard-alternate-images` | Discard alternate images |  |
| ❌ | P2 | `optimize.discard-form-actions` | Discard form submit, import and reset actions |  |
| ❌ | P1 | `optimize.flatten-form-fields` | Flatten form fields while optimizing |  |
| ❓ | P1 | `optimize.discard-comments` | Discard comments, forms and multimedia |  |
| ✅ | P1 | `optimize.discard-metadata` | Discard document information and metadata | --strip |
| ❓ | P1 | `optimize.discard-attachments` | Discard file attachments |  |
| ❓ | P2 | `optimize.discard-private-data` | Discard private application data (PieceInfo) |  |
| ❌ | P2 | `optimize.discard-external-refs` | Discard external cross-references |  |
| ❓ | P2 | `optimize.discard-hidden-layers` | Discard hidden layers and flatten visible ones |  |
| ❌ | P3 | `optimize.merge-image-fragments` | Detect and merge image fragments |  |
| ❌ | P3 | `optimize.lines-to-curves` | Convert smooth lines to curves |  |
| ✅ | P1 | `optimize.compress-structure` | Compress document structure (object streams) | オブジェクトストリーム（ADR-0016） |
| ✅ | P1 | `optimize.flate-unencoded` | Flate-encode unencoded streams; replace LZW with Flate | Flate 圧縮オプション |
| ❌ | P1 | `optimize.remove-invalid-links` | Remove invalid links, bookmarks and unreferenced destinations |  |
| ❌ | P2 | `optimize.optimize-content` | Optimize page content streams |  |
| ❌ | P1 | `optimize.audit-space` | Audit space usage |  |
| ❌ | P2 | `optimize.optimizer-presets` | Optimizer presets |  |
| ❌ | P2 | `optimize.batch-optimize` | Optimize multiple files |  |
| ❌ | P1 | `optimize.preflight` | Preflight analysis |  |
| ❌ | P1 | `optimize.preflight-fix` | Preflight analyze and fix |  |
| ❌ | P1 | `optimize.preflight-profiles` | Built-in preflight profiles |  |
| ❌ | P1 | `optimize.preflight-single-checks` | Single checks |  |
| ❌ | P1 | `optimize.preflight-single-fixups` | Single fixups |  |
| ❌ | P2 | `optimize.preflight-custom-profiles` | Create and edit custom profiles |  |
| ❌ | P2 | `optimize.preflight-profile-exchange` | Import, export and lock profiles |  |
| ❌ | P1 | `optimize.preflight-results` | Preflight results tree with object details |  |
| ❌ | P2 | `optimize.preflight-snap-view` | Snap view of a flagged object |  |
| ❌ | P1 | `optimize.preflight-reports` | Preflight reports (PDF, XML, text) |  |
| ❌ | P2 | `optimize.preflight-droplets` | Preflight droplets and batch preflight |  |
| ❌ | P3 | `optimize.preflight-audit-trail` | Embed preflight audit trail |  |
| ❌ | P1 | `optimize.pdfa-validate` | Verify PDF/A compliance (1-4) |  |
| 🔶 | P1 | `optimize.pdfa-convert` | Save as PDF/A (1b, 2b, 3b, 4) | publish upgrade で PDF/A-4 を宣言 |
| ⛔ | P1 | `optimize.pdfa-accessible-levels` | PDF/A conformance levels a and u | PDF/A-4 のみ宣言、a/u 水準は PDF/A-1〜3（1.7以前） |
| ❌ | P1 | `optimize.pdfx-validate` | Verify PDF/X compliance |  |
| ⛔ | P1 | `optimize.pdfx-convert` | Save as PDF/X (1a, 3, 4) | X-6 は拒否（ADR-0100） |
| ✅ | P1 | `optimize.output-intents` | Choose output intents and conditions | SetOutputIntent |
| ❌ | P2 | `optimize.pdfe` | PDF/E |  |
| ❌ | P2 | `optimize.pdfvt` | PDF/VT |  |
| ✅ | P1 | `optimize.pdfua-validate` | Verify PDF/UA-1 and PDF/UA-2 | inspect audit（Matterhorn 137条件中116を判定） |
| 🔶 | P1 | `optimize.standards-panel` | Standards panel showing declared conformance | 「規格を宣言」DeclareConformance |
| ❌ | P2 | `optimize.remove-standards-id` | Remove PDF/A or PDF/X identification |  |
| ❌ | P3 | `optimize.pdfx5-reference-xobjects` | Show PDF/X-5 reference XObjects |  |
| ⛔ | P3 | `optimize.einvoice-pdfa3` | Embed e-invoice XML in PDF/A-3 | A-3 は PDF 1.7 なので対象外 |

## L 印刷

| 状態 | tier | id | 機能 | 根拠・備考 |
| :-: | :-: | :-- | :-- | :-- |
| ✅ | P0 | `print.print-dialog` | Print dialog | W-17: スプーラーへ |
| ✅ | P0 | `print.printer-selection` | Choose printer and properties |  |
| 🔶 | P0 | `print.copies-collate` | Copies and collation | 部数 |
| ✅ | P0 | `print.page-range` | Print all, current page or a range (labels allowed) |  |
| ❌ | P1 | `print.odd-even-reverse` | Odd/even pages and reverse order |  |
| ❓ | P0 | `print.scaling` | Fit, actual size, shrink oversized, custom scale |  |
| ❌ | P2 | `print.paper-source-by-size` | Choose paper source by PDF page size |  |
| ❌ | P1 | `print.duplex` | Print on both sides |  |
| ❌ | P0 | `print.n-up` | Multiple pages per sheet |  |
| ❌ | P0 | `print.booklet` | Booklet printing |  |
| ❌ | P1 | `print.poster` | Poster / tiled printing with overlap and cut marks |  |
| ❓ | P0 | `print.orientation` | Auto portrait/landscape |  |
| ❓ | P0 | `print.comments-and-forms` | Print document, markups, stamps or form fields only |  |
| ❌ | P2 | `print.print-with-comment-summary` | Print with a comments summary |  |
| ❌ | P0 | `print.print-preview` | Print preview |  |
| ❌ | P1 | `print.grayscale` | Print in grayscale |  |
| ❌ | P1 | `print.print-as-image` | Print as image |  |
| ❌ | P1 | `print.page-setup` | Page setup |  |
| ❌ | P1 | `print.print-ready-pdf` | Produce a print-ready imposed PDF |  |
| ❓ | P2 | `print.print-pages-from-thumbnails` | Print selected pages from thumbnails |  |
| ❌ | P2 | `print.postscript-output` | PostScript Level 3 output |  |
| ❌ | P1 | `print.output-preview` | Output preview |  |
| ❌ | P1 | `print.separations` | Separations preview with area coverage |  |
| ❌ | P1 | `print.total-area-coverage` | Total area coverage warning |  |
| ❌ | P1 | `print.color-warnings` | Overprint and rich-black warnings |  |
| ❌ | P1 | `print.object-inspector` | Object inspector |  |
| ❌ | P2 | `print.simulate-paper-ink` | Simulate paper colour and black ink |  |
| ❌ | P1 | `print.overprint-preview` | Overprint preview |  |
| ❌ | P1 | `print.convert-colors` | Convert colours |  |
| ❌ | P1 | `print.ink-manager` | Ink manager |  |
| ❌ | P2 | `print.ink-aliases` | Ink aliases and spot-to-process |  |
| ❌ | P1 | `print.printer-marks` | Add printer marks (crop, bleed, registration, colour bars) |  |
| ❌ | P1 | `print.fix-hairlines` | Fix hairlines |  |
| ❌ | P2 | `print.flattener-preview` | Flattener preview |  |
| ❌ | P1 | `print.color-management` | Colour management settings (working spaces, intents) |  |
| ❌ | P2 | `print.advanced-print-separations` | Print separations and in-RIP separations |  |
| ❌ | P2 | `print.print-marks-bleeds` | Marks and bleeds when printing |  |
| ❌ | P3 | `print.trap-presets` | Trap presets (stored as data) |  |
| ❌ | P3 | `print.jdf` | JDF job definitions (read-only) |  |

## M アクセシビリティ

| 状態 | tier | id | 機能 | 根拠・備考 |
| :-: | :-: | :-- | :-- | :-- |
| ❓ | P0 | `a11y.app-screen-reader` | Screen-reader support for the app (AccessKit) |  |
| ❓ | P0 | `a11y.keyboard-only` | Full keyboard operation of the app |  |
| ✅ | P1 | `a11y.checker` | Accessibility checker (full check) | inspect audit / GUI アクセシビリティ監査 |
| ✅ | P1 | `a11y.checker-report` | Accessibility report | W-21f |
| 🔶 | P1 | `a11y.checker-fix` | Fix, skip, explain and re-check rules | remediate_pdf_ua（MCP） |
| ✅ | P1 | `a11y.rule-permission-flag` | Rule: accessibility permission flag | Matterhorn 監査（inspect audit） |
| ✅ | P1 | `a11y.rule-image-only` | Rule: image-only PDF | Matterhorn 監査（inspect audit） |
| ✅ | P1 | `a11y.rule-tagged-pdf` | Rule: tagged PDF | Matterhorn 監査（inspect audit） |
| 🔶 | P1 | `a11y.rule-logical-order` | Rule: logical reading order (manual) | Matterhorn 監査（inspect audit）。人が判断する13条件を含む |
| ✅ | P1 | `a11y.rule-primary-language` | Rule: primary language | Matterhorn 監査（inspect audit） |
| ✅ | P1 | `a11y.rule-title` | Rule: title shown in the window | Matterhorn 監査（inspect audit） |
| 🔶 | P1 | `a11y.rule-bookmarks` | Rule: bookmarks in long documents | Matterhorn 監査（inspect audit）。人が判断する13条件を含む |
| ❌ | P1 | `a11y.rule-color-contrast` | Rule: colour contrast (manual) |  |
| ✅ | P1 | `a11y.rule-tagged-content` | Rule: all content tagged or artifact | Matterhorn 監査（inspect audit） |
| ✅ | P1 | `a11y.rule-tagged-annotations` | Rule: tagged annotations | Matterhorn 監査（inspect audit） |
| ✅ | P1 | `a11y.rule-tab-order` | Rule: tab order follows structure | Matterhorn 監査（inspect audit） |
| ✅ | P1 | `a11y.rule-character-encoding` | Rule: reliable character encoding | Matterhorn 監査（inspect audit） |
| ✅ | P1 | `a11y.rule-tagged-multimedia` | Rule: tagged multimedia | Matterhorn 監査（inspect audit） |
| 🔶 | P1 | `a11y.rule-screen-flicker` | Rule: screen flicker (manual) | Matterhorn 監査（inspect audit）。人が判断する13条件を含む |
| 🔶 | P1 | `a11y.rule-scripts` | Rule: scripts (manual) | Matterhorn 監査（inspect audit）。人が判断する13条件を含む |
| 🔶 | P1 | `a11y.rule-timed-responses` | Rule: timed responses (manual) | Matterhorn 監査（inspect audit）。人が判断する13条件を含む |
| 🔶 | P1 | `a11y.rule-navigation-links` | Rule: navigation links (manual) | Matterhorn 監査（inspect audit）。人が判断する13条件を含む |
| ✅ | P1 | `a11y.rule-tagged-fields` | Rule: tagged form fields | Matterhorn 監査（inspect audit） |
| ✅ | P1 | `a11y.rule-field-descriptions` | Rule: field descriptions | Matterhorn 監査（inspect audit） |
| ✅ | P1 | `a11y.rule-figure-alt` | Rule: figures have alternate text | Matterhorn 監査（inspect audit） |
| ✅ | P1 | `a11y.rule-nested-alt` | Rule: no nested alternate text | Matterhorn 監査（inspect audit） |
| ✅ | P1 | `a11y.rule-alt-associated` | Rule: alternate text associated with content | Matterhorn 監査（inspect audit） |
| ✅ | P1 | `a11y.rule-alt-hides-annot` | Rule: alternate text does not hide annotations | Matterhorn 監査（inspect audit） |
| ✅ | P1 | `a11y.rule-other-alt` | Rule: other elements have alternate text | Matterhorn 監査（inspect audit） |
| ✅ | P1 | `a11y.rule-table-rows` | Rule: table rows | Matterhorn 監査（inspect audit） |
| ✅ | P1 | `a11y.rule-table-cells` | Rule: TH and TD in rows | Matterhorn 監査（inspect audit） |
| ✅ | P1 | `a11y.rule-table-headers` | Rule: tables have headers | Matterhorn 監査（inspect audit） |
| ✅ | P1 | `a11y.rule-table-regularity` | Rule: table regularity | Matterhorn 監査（inspect audit） |
| ✅ | P1 | `a11y.rule-table-summary` | Rule: table summary | Matterhorn 監査（inspect audit） |
| ✅ | P1 | `a11y.rule-list-items` | Rule: list items | Matterhorn 監査（inspect audit） |
| ✅ | P1 | `a11y.rule-lbl-lbody` | Rule: Lbl and LBody | Matterhorn 監査（inspect audit） |
| ✅ | P1 | `a11y.rule-heading-nesting` | Rule: heading nesting | Matterhorn 監査（inspect audit） |
| ✅ | P1 | `a11y.autotag` | Automatically tag a document | Retag、CLI edit tag |
| ❌ | P1 | `a11y.autotag-fields` | Autotag form fields |  |
| ✅ | P1 | `a11y.reading-order-tool` | Reading order tool | 読み上げ順オーバーレイ |
| ✅ | P1 | `a11y.tags-panel` | Tags panel (new, delete, find, change to artifact) | 構造ツリー |
| ✅ | P1 | `a11y.tag-properties` | Tag properties (type, title, actual text, alternate text, language) | 要素プロパティ、SetStructAttribute |
| ✅ | P2 | `a11y.role-map` | Role map editor | MapStructType |
| ❌ | P3 | `a11y.class-map` | Class map editor |  |
| ❓ | P1 | `a11y.order-panel` | Order panel |  |
| ❌ | P2 | `a11y.content-panel` | Content panel |  |
| ✅ | P2 | `a11y.create-tag-from-selection` | Create a tag from a selection | タグ付けブラシ、意味タグ作成 |
| ✅ | P1 | `a11y.artifact-marking` | Mark content as artifact | MarkArtifact |
| 🔶 | P2 | `a11y.find-untagged` | Find unmarked content, comments and links | Matterhorn チェックポイント01（W-21c） |
| ❌ | P2 | `a11y.table-editor` | Table editor (headers, scope, spans) |  |
| ✅ | P1 | `a11y.alt-text-workflow` | Set alternate text figure by figure (with decorative option) | 代替テキストギャラリー |
| ❌ | P2 | `a11y.alt-text-suggestions` | Suggest alternate text via an AI provider |  |
| ✅ | P1 | `a11y.set-language` | Set document language | W-22a |
| ❓ | P1 | `a11y.set-title-display` | Show the document title in the window |  |
| ❌ | P1 | `a11y.make-accessible-action` | Make Accessible guided action |  |
| ❌ | P2 | `a11y.setup-assistant` | Accessibility setup assistant |  |
| ✅ | P2 | `a11y.read-aloud` | Read out loud (platform TTS) | W-19b: OSの音声合成 |
| ❓ | P2 | `a11y.read-aloud-options` | Read-aloud voice, pitch and speed |  |
| ❌ | P2 | `a11y.replace-page-colors` | Replace page colours (high contrast) |  |
| ❌ | P2 | `a11y.reflow-tagged` | Reflow tagged content |  |
| ❌ | P2 | `a11y.keyboard-selection-cursor` | Always show the keyboard selection cursor |  |

## N その他

| 状態 | tier | id | 機能 | 根拠・備考 |
| :-: | :-: | :-- | :-- | :-- |
| ✅ | P1 | `misc.compare-text` | Compare files: text differences | W-18 |
| ✅ | P1 | `misc.compare-visual` | Compare files: visual differences | W-18（画素） |
| ❓ | P1 | `misc.compare-side-by-side` | Side-by-side compare view with synchronised scrolling |  |
| ❌ | P1 | `misc.compare-report` | Compare summary report as PDF |  |
| ❌ | P2 | `misc.compare-filters` | Filter changes by type (text, images, formatting) |  |
| ❓ | P2 | `misc.compare-page-ranges` | Compare page ranges |  |
| ❌ | P2 | `misc.compare-scanned` | Compare scanned documents via OCR |  |
| ❌ | P1 | `misc.action-wizard-run` | Run guided actions on files |  |
| ❌ | P1 | `misc.action-wizard-create` | Create and edit actions |  |
| ❌ | P2 | `misc.action-wizard-manage` | Import, export, copy and delete actions |  |
| ❌ | P1 | `misc.action-prepare-distribution` | Built-in action: prepare for distribution |  |
| ❌ | P1 | `misc.action-optimize-scans` | Built-in action: optimize scanned documents |  |
| ❌ | P1 | `misc.action-archive` | Built-in action: archive documents |  |
| ❌ | P2 | `misc.action-publish-web` | Built-in action: prepare for web publishing |  |
| ❌ | P1 | `misc.batch-processing` | Batch processing over files and folders |  |
| ❌ | P2 | `misc.action-prompt-steps` | Prompt-user and instruction steps |  |
| ❌ | P2 | `misc.action-javascript-step` | Execute JavaScript step |  |
| ❌ | P1 | `misc.custom-commands` | Custom commands |  |
| ❌ | P2 | `misc.custom-tools` | Custom tool sets |  |
| 🔶 | P0 | `misc.command-registry` | One command registry for menus, shortcuts, palette and automation | コマンドパレット |
| 🔶 | P1 | `misc.disabled-command-reasons` | Disabled commands explain why | コマンドパレットが実行できない項目を灰色で表示 |
| ✅ | P0 | `misc.cli-inspect` | CLI: inspect, render and extract text | fepdf inspect（10サブコマンド） |
| ✅ | P0 | `misc.cli-edit` | CLI: edit, combine, extract, split | fepdf edit（11サブコマンド） |
| 🔶 | P0 | `misc.cli-run` | CLI run: call any automation tool or a JSON script | publish（upgrade/render/sign/verify） |
| 🔶 | P0 | `misc.cli-completeness` | CLI covers every shipped command | CLIは54操作中約9を公開（2026-09-28） |
| ✅ | P0 | `misc.automation-tools` | Headless JSON-Schema tool table | MCP が52操作（2026-09-28） |
| ❌ | P1 | `misc.command-list-tool` | List commands with enablement and their tools |  |
| ✅ | P0 | `misc.mcp-server` | Opt-in MCP server over stdio | fepdf-mcp |
| ❓ | P0 | `misc.automation-root-confinement` | Confine automation file access to a root directory |  |
| 🔶 | P1 | `misc.mcp-resources` | MCP resources for page images and text | 構造ツリーとメタデータ（ページ画像は未確認） |
| ❌ | P0 | `misc.ui-control-channel` | UI control channel (inspect, click, type, keys, commands) |  |
| ❌ | P1 | `misc.ui-screenshots` | Screenshots of the window or a region via the control channel |  |
| ❌ | P2 | `misc.ai-provider` | AI provider interface (local model or own endpoint), off by default |  |
| ❌ | P2 | `misc.ai-summary` | Summarize a document |  |
| ❌ | P2 | `misc.ai-ask` | Ask questions with citations to pages |  |
| ❌ | P3 | `misc.ai-conversational-edit` | Natural-language editing through registry commands |  |
| ❌ | P3 | `misc.ai-presentation` | Generate a presentation from documents |  |
| ❌ | P3 | `misc.ai-podcast` | Generate an audio overview |  |
| ❌ | P3 | `misc.ai-charts` | Charts from document data |  |
| ❌ | P3 | `misc.contract-analysis` | Contract key terms and multi-contract compare |  |
| ☁ | N/A | `misc.adobe-ai-assistant` | Adobe AI Assistant | Adobeクラウド専用 |
| ☁ | N/A | `misc.pdf-spaces` | PDF Spaces | Adobeクラウド専用 |
| ☁ | N/A | `misc.adobe-express` | Adobe Express integration and generated images | Adobeクラウド専用 |
| ☁ | N/A | `misc.document-cloud-storage` | Adobe Document Cloud storage | Adobeクラウド専用 |
| ☁ | N/A | `misc.share-links` | Share links and invitations | Adobeクラウド専用 |
| ☁ | N/A | `misc.connected-storage` | Connected third-party storage accounts in-app | Adobeクラウド専用 |
| ☁ | N/A | `misc.cross-device-sync` | Sync signatures, stamps and preferences across devices | Adobeクラウド専用 |
| ☁ | N/A | `misc.adobe-account` | Adobe account sign-in | Adobeクラウド専用 |
| ⛔ | P3 | `misc.rich-media-preserve` | Preserve 3D, video and sound content | マルチメディアは対象外 |
| ⛔ | P3 | `misc.rich-media-poster` | Show posters for rich media | 同上 |
| ⛔ | P3 | `misc.rich-media-playback` | Play video and sound via the platform | 同上 |
| ⛔ | P3 | `misc.3d-view` | View 3D (U3D/PRC) content | 同上 |
| ⛔ | P3 | `misc.legacy-multimedia` | Legacy multimedia (screen annotations, renditions) | 同上 |
| ☁ | N/A | `misc.flash-swf` | Flash/SWF content | Adobeクラウド専用 |
| ❌ | P1 | `misc.preferences-dialog` | Preferences dialog generated from a schema |  |
| ❌ | P1 | `misc.prefs-general` | Preferences: general |  |
| ❌ | P1 | `misc.prefs-page-display` | Preferences: page display |  |
| ❌ | P1 | `misc.prefs-documents` | Preferences: documents (autosave interval, recent count) |  |
| ❌ | P1 | `misc.prefs-commenting` | Preferences: commenting |  |
| ❌ | P1 | `misc.prefs-forms` | Preferences: forms |  |
| ❌ | P1 | `misc.prefs-identity` | Preferences: identity |  |
| ❌ | P2 | `misc.prefs-full-screen` | Preferences: full screen |  |
| ❌ | P2 | `misc.prefs-search` | Preferences: search |  |
| ❌ | P1 | `misc.prefs-signatures` | Preferences: signatures |  |
| ❌ | P1 | `misc.prefs-security` | Preferences: security and trust |  |
| ❌ | P2 | `misc.prefs-spelling` | Preferences: spelling |  |
| ✅ | P1 | `misc.prefs-language` | Preferences: language | UI言語の設定 |
| ❌ | P2 | `misc.prefs-reading` | Preferences: reading |  |
| ❌ | P1 | `misc.prefs-accessibility` | Preferences: accessibility |  |
| ❌ | P1 | `misc.prefs-color-management` | Preferences: colour management |  |
| ❌ | P1 | `misc.prefs-javascript` | Preferences: JavaScript |  |
| ❌ | P2 | `misc.prefs-measuring` | Preferences: measuring |  |
| ❌ | P2 | `misc.prefs-units-guides` | Preferences: units and guides |  |
| ❌ | P2 | `misc.prefs-content-editing` | Preferences: content editing |  |
| ❌ | P2 | `misc.prefs-convert` | Preferences: convert to and from PDF |  |
| ❌ | P3 | `misc.prefs-catalog` | Preferences: catalog / index |  |
| ❌ | P2 | `misc.admin-preferences` | Admin-deployable preference defaults |  |
| ✅ | P1 | `misc.localization` | Localization (Fluent catalogs) | 日本語・英語UI |
| ❌ | P2 | `misc.rtl-ui` | Right-to-left user interface |  |
| 🔶 | P1 | `misc.installers` | Signed installers for macOS, Windows and Linux | GitHub Releases の配布物（署名の有無は未確認） |
| ⛔ | P1 | `misc.web-deploy` | Hosted web app deployment | fepdf-wasm を同格のフロントエンドにしない（ROADMAP Not planned） |
| ❌ | P0 | `misc.performance-budgets` | Meet performance budgets on Tier-1 platforms |  |
| ❌ | P3 | `misc.send-email` | Send the file by email (system mail client) |  |
| 🔶 | P1 | `misc.user-docs` | User documentation and keyboard reference | README と各コマンドの --help |
