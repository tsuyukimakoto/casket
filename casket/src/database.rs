use crate::processor::ProcessedInfo;
use chrono::SecondsFormat; // For ISO 8601 formatting
use rusqlite::{params, Connection, Result, Transaction}; // Added params and Transaction
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
            datetime_indexed TEXT NOT NULL,    -- 絞り込み用日時 (YYYYMMDDHH形式)
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

/// 処理結果をデータベースに保存する (トランザクション内で使用される想定)
fn save_processed_info_txn(
    tx: &Transaction,
    processed_info: &ProcessedInfo,
) -> Result<usize> { // Returns number of affected rows (0 if ignored)
    // println!("Saving info for {:?} to database...", processed_info.original_path); // Logged in save_all

    // Convert Option<DateTime> to Option<String> (ISO 8601)
    let datetime_str = processed_info
        .metadata
        .datetime_original
        .map(|dt| dt.to_rfc3339_opts(SecondsFormat::Secs, true)); // Use RFC3339 (ISO 8601 compatible)

    // Convert PathBufs to Strings (handle potential non-UTF8 paths?)
    let original_path_str = processed_info.original_path.to_string_lossy().to_string();
    let data_path_str = processed_info.data_dest_path.to_string_lossy().to_string();
    let thumbnail_path_str = processed_info
        .thumbnail_dest_path
        .as_ref()
        .map(|p| p.to_string_lossy().to_string());

    // INSERT OR IGNORE: 重複する original_path があれば挿入をスキップする
    tx.execute(
        "INSERT OR IGNORE INTO media_items (
            original_path, data_path, thumbnail_path,
            datetime_original, datetime_indexed, camera_make, camera_model
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            original_path_str,
            data_path_str,
            thumbnail_path_str,
            datetime_str,
            processed_info.datetime_indexed,
            processed_info.metadata.camera_make,
            processed_info.metadata.camera_model,
        ],
    )
}

/// 複数の処理結果をまとめてデータベースに保存する (トランザクション使用)
pub fn save_all_processed_info(
    conn: &mut Connection, // Needs mutable connection for transaction
    results: &[ProcessedInfo],
) -> Result<()> {
    println!("\nSaving all processed info to database...");
    let tx = conn.transaction()?; // Start transaction

    let mut saved_count = 0;
    let mut ignored_count = 0;
    let mut error_count = 0;

    for info in results {
        match save_processed_info_txn(&tx, info) {
            Ok(affected_rows) => {
                if affected_rows > 0 {
                    saved_count += 1;
                    println!("  Saved info for {:?}", info.original_path);
                } else {
                    ignored_count += 1;
                     println!("  Ignored duplicate entry for {:?}", info.original_path);
                }
            }
            Err(e) => {
                eprintln!("  Error saving info for {:?}: {}", info.original_path, e);
                error_count += 1;
                // トランザクション内のエラーは通常ロールバックすべきだが、
                // ここでは個別の挿入エラーとして扱い、処理を続行する方針
                // (ロールバックする場合は早期リターン `Err(e)?` または `tx.rollback()?` を使う)
            }
        }
    }

    if error_count == 0 {
        tx.commit()?; // Commit transaction if no errors occurred during iteration
        println!(
            "Database save complete. {} new records saved, {} duplicates ignored.",
            saved_count, ignored_count
        );
    } else {
        // エラーがあった場合、コミットせずにロールバックすることも検討できるが、
        // ここでは個別のエラーとして扱い、成功分はコミットする方針
        // (ただし、上記ループ内でエラー時に早期リターンしていないため、
        //  エラーがあっても tx.commit() が呼ばれる。より厳密なエラー処理が必要なら要修正)
         tx.commit()?; // ここではエラーがあってもコミットする
         eprintln!(
             "Database save finished with errors. {} new records saved, {} duplicates ignored, {} errors.",
             saved_count, ignored_count, error_count
         );
         eprintln!("Please check database save errors above.");
         // エラーがあったことを示すためにエラーを返すことも検討
         // return Err(rusqlite::Error::ExecuteReturnedMoreThanOneRow); // ダミーのエラー型
    }

    Ok(())
}
