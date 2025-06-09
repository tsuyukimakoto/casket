# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## プロジェクト概要

Casket は写真・動画ファイルのカタログインポートツールです。メディアファイルを日付ベースのディレクトリ構造で整理し、サムネイル生成、メタデータ抽出、SQLiteデータベースへの情報保存を行います。

## ビルド・実行コマンド

```bash
# プロジェクトディレクトリでビルド
cd casket
cargo build

# リリースビルド
cargo build --release

# 実行 (ソースディレクトリとカタログ名を指定)
cargo run -- --source /path/to/source --catalog-name default

# テスト実行
cargo test
```

## アーキテクチャ

### モジュール構成

- `main.rs`: CLI引数解析、全体の処理フロー制御
- `config.rs`: 設定ファイル管理 (TOML形式、カタログ設定)
- `scanner.rs`: ディレクトリの再帰的スキャン、ファイル一覧取得
- `processor.rs`: ファイル処理 (コピー、メタデータ抽出、サムネイル生成)
- `database.rs`: SQLiteデータベース操作 (テーブル作成、データ保存)

### データフロー

1. 設定ファイル読み込み (`~/.config/casket/catalogs.toml`)
2. ソースディレクトリのファイルスキャン
3. 各ファイルの処理:
   - EXIFメタデータ抽出
   - 年/月/日ディレクトリ構造での保存
   - サムネイル生成 (RAW/動画対応)
4. SQLiteデータベースへの情報保存

### 重要な外部依存関係

- `kamadak-exif` (as `exif`): EXIFメタデータ抽出
- `ffmpeg-next`: 動画ファイル処理 (ビルド時にffmpeg開発ライブラリが必要、静的リンク)
- `rusqlite`: SQLiteデータベース操作
- `image`: 一般的な画像フォーマット処理
- `chrono`: 日時処理
- `clap`: CLI引数解析
- `dirs`: 設定ディレクトリ取得

注意: RAWファイル処理 (`libraw-rs`) は現在無効化されています。

### エラーハンドリング方針

- 個別ファイルの処理エラーは警告表示して処理継続
- 設定読み込みやデータベース操作の重要なエラーは即座に終了
- トランザクション使用によるデータ整合性確保

## 設定ファイル形式

カタログ設定は `~/.config/casket/catalogs.toml` に以下の形式で記述:

```toml
[catalog_name]
data_path = "/path/to/original/files"
thumbnail_path = "/path/to/thumbnails"
```

## 開発時の注意点

- RAWファイル処理は埋め込みサムネイル優先、失敗時はRAW現像
- ファイルパスの重複チェック機能 (original_path UNIQUE制約)
- 日付情報はEXIF優先、なければファイル更新日時を使用
- サムネイルは常にJPEG形式で保存