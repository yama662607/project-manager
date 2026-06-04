# Handoff Document — TauriBench Keyboard Fix

## Date
2026-06-04

## Primary Issue
**TauriBench のキーボード操作（Escape, Enter, Ctrl+M）が一切機能しない。**

ユーザはフレームレスウィンドウ（左上の閉じる/最小化/最大化ボタンなし）を希望している。
現在 `decorations: false`, `transparent: true` になっているが、キー入力が効かない。

## What the user wants
1. Escape / Enter / Ctrl+M が動作すること
2. フレームレスウィンドウ（左上3つのボタンなし）
3. 純粋な Rust + Web（トレイアイコン、グローバルショートカット、メニューバーは不要）
4. AppKitBench / IcedBench / WailsBench は残す（削除しないこと）

## Git history
```
36e8bb8 fix: add Escape and Ctrl+M keyboard handlers in web frontend
69707d8 fix: restore other app implementations, make Tauri frameless
f3adb59 refactor: remove all Apple native code and other app implementations
4ae0de6 fix: use explicit Mutex<bool> state for palette visibility
6128f47 fix: improve show_palette toggle logic
4987a8b Initial commit: project launcher benchmark apps (AppKit, Tauri, Iced, Wails)
```

- `4987a8b`: 初期コミット。4アプリすべて含む。トレイアイコン、グローバルショートカットあり。
- `f3adb59`: **他アプリを誤って削除してしまったコミット**（AppKit, Iced, Wails削除）
- `69707d8`: 他アプリ復元 + Tauriをframelessに（`decorations: false`, `transparent: true`）
- `36e8bb8`: フロントエンドにEscape/Ctrl+Mハンドラ追加（最新）

## Key Files

### `mock-app/tauri-bench/src-tauri/tauri.conf.json`
```json
"decorations": false,   // フレームレス
"transparent": true,    // 透明背景
"visible": true,        // 起動時に表示
"macOSPrivateApi": true // transparentに必要
```

### `mock-app/tauri-bench/src-tauri/Cargo.toml`
```toml
tauri = { version = "2.11.2", features = ["macos-private-api"] }
# tray-icon, global-shortcut プラグインは削除済み
```

### `mock-app/tauri-bench/src-tauri/src/lib.rs`
- シンプルなRustバックエンド（371行）
- トレイアイコン、グローバルショートカット、メニューバーは**すべて削除済み**
- 残っているコマンド: `load_projects`, `get_config`, `save_config`, `log_event`, `log_metric`, `open_project`, `open_settings_window`, `close_settings_window`, `browse_folder`
- `setup()` は `window.center()` だけ
- `open_project` は `zed` コマンドでプロジェクトを開く

### `mock-app/tauri-bench/src/main.ts`
- フロントエンド（451行）
- キー入力: `search` 要素は `readOnly = true`、`inputmode="none"`。`beforeinput`/`compositionstart`/`compositionupdate`/`compositionend` を preventDefault。
- `search.addEventListener("keydown", ...)` で Escape/Enter/Ctrl+M/Cmd+,/↑↓/Ctrl+N/Ctrl+P をハンドル
- `document.addEventListener("keydown", ..., true)` で capture フェーズで Escape/Cmd+, をバックアップハンドル
- `getCurrentWindow().hide()` でウィンドウを隠す（`@tauri-apps/api/window` から import）
- Dock アイコンクリックで復帰可能（Info.plist に LSUIElement なし）

### `mock-app/tauri-bench/src/styles.css`
- ダークテーマ
- `html, body { background: transparent; }` — フレームレスウィンドウ用
- `.palette { background: var(--bg-deep); border-radius: 14px; }` — パレット自体が背景を持つ

### `mock-app/tauri-bench/src/settings.ts` + `mock-app/tauri-bench/settings.html`
- ショートカット設定タブは削除済み
- プロジェクトの追加/編集/削除、Browse ボタン（`osascript` でフォルダ選択）

## 考えられる原因

### 1. `search.readOnly = true` がキーイベントを阻害
`readOnly` な `<input>` では、macOS の WebKit が特定のキーイベントを配送しない可能性がある。
→ `readOnly` を外し、代わりに `beforeinput` preventDefault だけに頼る。または `<div>` に変える。

### 2. フレームレス透明ウィンドウで webview がキーイベントを受け取れない
macOS で `decorations: false` + `transparent: true` の組み合わせは、NSWindow の
`canBecomeKeyWindow` や `acceptsFirstResponder` の挙動に影響する可能性がある。
Tauri v2 が内部的に適切に設定していない可能性。
→ `decorations: true` に戻してキー入力が効くか確認（問題の切り分け）

### 3. `getCurrentWindow().hide()` がエラー
Tauri v2 の `Window.hide()` API が `invoke` ベースで、`async` だが await していない。
Promise rejection が握りつぶされている可能性。
→ `.catch(console.error)` を付けてエラーを確認

### 4. `search.focus()` が効いていない
`await loadProjectsWithRetry()` の後に `search.focus()` を呼んでいるが、
DOM が完全にレンダリングされる前に focus している可能性。
→ `requestAnimationFrame` や `setTimeout` で遅延させる

## Build & Install
```bash
cd /Users/daisukeyamashiki/Code/Projects/project-manager/mock-app

# Build Tauri
bun run tools/build_tauri.mjs

# Install
pkill -f "tauri-bench"
rm -rf /Applications/TauriBench.app
cp -R tauri-bench/src-tauri/target/release/bundle/macos/TauriBench.app /Applications/

# Launch
open /Applications/TauriBench.app
```

## デバッグ方法
1. コンソールログを見る: `open /Applications/TauriBench.app` の後、`Console.app` で `tauri-bench` をフィルタ
2. Safari の Web Inspector で webview をデバッグ: `Safari > Develop > シミュレータ > TauriBench`
3. まず `decorations: true`, `transparent: false` に戻してキー入力が効くか確認（問題の切り分け）

## 注意点
- AppKitBench / IcedBench / WailsBench のコードは存在するが、AppKitBench だけが `/Applications` にビルド済み
- ユーザの環境は macOS 26.4.1
- グローバルショートカットは不要（ユーザが明示的に削除を指示）
- Ctrl+M は「ウィンドウがフォーカスされているとき」のみ有効（グローバルではない）
