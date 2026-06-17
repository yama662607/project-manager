# pm

TUIプロジェクトマネージャー - GUI設定画面を使わずに素早くプロジェクトを追加・管理

## 特徴

- **Quick Add**: 現在のディレクトリを最小の手順でプロジェクトとして登録
- **キーボード操作**: Emacs風のナビゲーション（Ctrl+N/Ctrl+P）と矢印キー
- **軽量**: 589KBのシングルバイナリ

## インストール

```bash
cd /Users/daisukeyamashiki/Code/Projects/project-manager/tui-bench
cargo build --release
```

PATHに追加:
```bash
export PATH="$PATH:/Users/daisukeyamashiki/Code/Projects/project-manager/tui-bench/target/release"
```

## 使用方法

### Quick Add（現在のディレクトリを登録）

```bash
cd /path/to/your-project
pm
# Enterを押すだけで登録完了
```

### プロジェクト一覧

```bash
pm
# 既に登録済みのディレクトリでは一覧画面が開きます
```

### キー操作

#### プロジェクト一覧画面
- `Ctrl+N`/`↓`: 次の項目へ
- `Ctrl+P`/`↑`: 前の項目へ
- `n`: 新規追加（New）
- `e`: 選択項目を編集（Edit）
- `d`: 選択項目を削除（Delete）
- `q`/`Escape`: 終了

#### 追加・編集フォーム
- `Enter`: 次のフィールドへ（最後で保存）
- `Escape`: キャンセル
- `Ctrl+N`/`↓`: 次のフィールドへ
- `Ctrl+P`/`↑`: 前のフィールドへ

## 設定ファイル

プロジェクト設定は `~/.project-manager.json` に保存されます。

TauriBenchやAppKitBenchと同じ設定ファイルを使用するため、互換性があります。
