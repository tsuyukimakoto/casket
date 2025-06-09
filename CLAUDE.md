# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## プロジェクト概要

Casket は写真・動画ファイルのカタログインポートツールです。メディアファイルを日付ベースのディレクトリ構造で整理し、2048pxサムネイル生成、メタデータ抽出、SQLiteデータベースへの情報保存を行います。

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

1. 設定ファイル読み込み (macOS: `~/Library/Application Support/casket/catalogs.toml`)
2. ソースディレクトリのファイルスキャン
3. 各ファイルの処理:
   - EXIFメタデータ抽出
   - 年/月/日ディレクトリ構造での保存
   - 2048pxサムネイル生成 (全形式対応)
4. SQLiteデータベースへの情報保存

### 重要な外部依存関係

- `kamadak-exif` (as `exif`): EXIFメタデータ抽出
- `libraw-rs`: RAWファイル処理 (NEF等の現像処理)
- `rusqlite`: SQLiteデータベース操作
- `image`: 一般的な画像フォーマット処理とJPEGエンコード
- `chrono`: 日時処理
- `clap`: CLI引数解析
- `dirs`: 設定ディレクトリ取得

システム依存:
- `sips` (macOS): HEIC/DNG変換処理

### エラーハンドリング方針

- 個別ファイルの処理エラーは警告表示して処理継続
- 設定読み込みやデータベース操作の重要なエラーは即座に終了
- トランザクション使用によるデータ整合性確保

## サムネイル生成仕様

### サイズとクオリティ設定

- **最大サイズ**: 長辺2048px（アスペクト比維持）
- **拡大防止**: 元画像が2048px以下の場合は元サイズを保持
- **JPEGクオリティ**: 10段階設定 (1=10%〜10=100%、デフォルト6=60%)
- **出力形式**: JPEG固定

### 対応ファイル形式

1. **RAWファイル (NEF/CR2/ARW/DNG)**:
   - libraw-rs による8bit/16bit現像処理
   - DNG: sipsコマンドによるフォールバック変換

2. **HEIC/HEIF**:
   - macOS sipsコマンドによるJPEG変換

3. **一般画像 (JPEG/PNG/TIFF/WebP等)**:
   - imageクレートによる直接処理

### サムネイル生成フロー (RAW)

1. libraw 8bit処理
2. libraw 16bit処理 (フォールバック)
3. EXIF埋め込みプレビュー抽出 (フォールバック)
4. sips変換 (DNG用最終手段)

## 設定ファイル形式

カタログ設定は macOS: `~/Library/Application Support/casket/catalogs.toml`

```toml
[catalog_name]
data_path = "/path/to/original/files"
thumbnail_path = "/path/to/thumbnails"
```

## 開発時の注意点

### サムネイル生成関連

- `THUMBNAIL_MAX_SIZE` = 2048px (長辺)
- `THUMBNAIL_QUALITY` = 6 (デフォルトJPEGクオリティ)
- `resize_without_upscaling()`: 拡大防止機能
- `save_jpeg_thumbnail()`: クオリティ指定JPEG保存

### RAW処理のフォールバック戦略

1. **NEF**: librawで直接現像 (成功率高)
2. **DNG**: libraw失敗時はsips変換 (iPhone 16等新形式対応)
3. **CR2/ARW**: librawで処理

### データベース設計

- ファイルパス重複チェック (original_path UNIQUE制約)
- 日付情報: EXIF優先、フォールバックでファイル更新日時
- サムネイルパス: thumbnail_path カラムで管理