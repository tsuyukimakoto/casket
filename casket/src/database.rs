use crate::processor::ProcessedInfo;
use rusqlite::{Connection, Result};
use std::path::Path;

/// データベース接続を開く (ファイルが存在しなければ作成される)
pub fn open_database(db_path: &Path) -> Result<Connection> {
    println!("Opening database connection to: {:?}", db_path);
    Connection::open(db_path)
}

/// 必要なテーブルを作成する (存在しない場合のみ)
pub fn create_tables(conn: &Connection) -> Result<()> {
    println!("Creating database tables if they don't exist...");
    conn.execute(
        "CREATE TABLE IF NOT EXISTS media_items (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            original_path TEXT NOT NULL UNIQUE, -- 元ファイルのフルパス (重複インポート防止用)
            data_path TEXT NOT NULL,           -- データ保存先パス
            thumbnail_path TEXT,               -- サムネイル保存先パス (Nullable)
            datetime_original TEXT,            -- 撮影日時 (ISO 8601形式)
            camera_make TEXT,                  -- カメラメーカー
            camera_model TEXT,                 -- カメラモデル
            imported_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP -- インポート日時
            -- TODO: 他のメタデータカラムを追加 (lens, iso, aperture, shutter_speedなど)
        )",
        [], // no parameters
    )?;
    println!("Table 'media_items' checked/created.");
    Ok(())
}

/// 処理結果をデータベースに保存する (仮実装: まだ何もしない)
pub fn save_processed_info(
    conn: &Connection,
    processed_info: &ProcessedInfo,
) -> Result<()> {
    println!("Saving info for {:?} to database...", processed_info.original_path);
    // TODO: processed_infoの内容を media_items テーブルにINSERTするSQLを実行する
    // 注意: datetime_original は Option<DateTime<Local>> なので、TEXTに変換する必要がある
    // 注意: thumbnail_path も Option<PathBuf> なので、TEXTまたはNULLに変換する必要がある
    // 注意: original_path が UNIQUE なので、重複挿入はエラーになる (これをハンドリングするか、事前にチェックするか)
    Ok(())
}

/// 複数の処理結果をまとめてデータベースに保存する (仮実装: 各要素をsave_processed_infoで処理)
pub fn save_all_processed_info(
    conn: &Connection,
    results: &[ProcessedInfo],
) -> Result<()> {
    // TODO: トランザクションを使って効率化・原子性を担保する
    println!("\nSaving all processed info to database...");
    let mut saved_count = 0;
    let mut error_count = 0;
    for info in results {
        match save_processed_info(conn, info) {
            Ok(_) => saved_count += 1,
            Err(e) => {
                eprintln!("  Error saving info for {:?}: {}", info.original_path, e);
                error_count += 1;
                // エラーがあっても続行する (個別エラーとして扱う)
            }
        }
    }
    println!("Database save complete. {} records saved, {} errors.", saved_count, error_count);
    if error_count > 0 {
         eprintln!("Please check database save errors above.");
         // 必要であればここでエラーを返すことも検討
         // return Err(...)
    }
    Ok(())
}
