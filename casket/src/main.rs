use clap::Parser;
use std::path::PathBuf;
use std::process; // For exiting the program

mod config; // configモジュールを宣言
mod database; // databaseモジュールを宣言
mod processor; // processorモジュールを宣言
mod scanner; // scannerモジュールを宣言

/// カメラデータをカタログにインポートするアプリケーション
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// インポート元のディレクトリパス
    #[arg(short, long, value_name = "SOURCE_DIR")]
    source: PathBuf,

    /// 使用するカタログ名
    #[arg(short, long, value_name = "CATALOG_NAME")]
    catalog_name: String, // 変数名を変更 catalog -> catalog_name
}

fn main() {
    let cli = Cli::parse();

    println!("Source directory: {:?}", cli.source);
    println!("Catalog name: {}", cli.catalog_name);

    // カタログ設定の読み込み
    let config = match config::load_config() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Error loading configuration: {}", e);
            process::exit(1);
        }
    };

    // 指定されたカタログを取得
    let catalog = match config.catalogs.get(&cli.catalog_name) {
        Some(cat) => cat,
        None => {
            eprintln!("Error: Catalog '{}' not found in configuration.", cli.catalog_name);
            eprintln!("Available catalogs: {:?}", config.catalogs.keys());
            process::exit(1);
        }
    };

    println!("Using catalog '{}':", cli.catalog_name);
    println!("  Data path: {:?}", catalog.data_path);
    println!("  Thumbnail path: {:?}", catalog.thumbnail_path);

    // ソースディレクトリのスキャン
    println!("\nScanning source directory...");
    let files_to_process = match scanner::scan_directory(&cli.source) {
        Ok(files) => {
            println!("Found {} files to process.", files.len());
            files
        }
        Err(e) => {
            eprintln!("Error scanning source directory {:?}: {}", cli.source, e);
            process::exit(1);
        }
    };

    if files_to_process.is_empty() {
        println!("No files found in the source directory. Exiting.");
        process::exit(0);
    }

    // ファイル処理（コピー、サムネイル生成、メタデータ抽出）
    println!("\nProcessing files...");
    let mut processed_results = Vec::new();
    let mut error_count = 0;

    for file_info in files_to_process {
        match processor::process_file(&file_info, catalog) {
            Ok(info) => {
                println!("Successfully processed: {:?}", info.original_path);
                processed_results.push(info);
            }
            Err(e) => {
                eprintln!("Error processing file {:?}: {}", file_info.path, e);
                error_count += 1;
                // エラーが発生しても処理を続けるか、停止するか？ ここでは続ける
            }
        }
    }

    println!(
        "\nProcessing complete. {} files processed successfully, {} errors.",
        processed_results.len(),
        error_count
    );

    if error_count > 0 {
        eprintln!("Please check the errors above.");
        // エラーがあった場合に終了コードを変えることも検討
        // process::exit(1);
    }

    if processed_results.is_empty() && error_count > 0 {
         println!("No files were processed successfully.");
         process::exit(1); // 成功したファイルがなければエラー終了
    }

    // データベースへの保存
    let db_path = catalog.thumbnail_path.join("casket.db");
    match database::open_database(&db_path) {
        Ok(conn) => {
            if let Err(e) = database::create_tables(&conn) {
                eprintln!("Error creating database tables: {}", e);
                // テーブル作成エラーは致命的かもしれないので終了する
                process::exit(1);
            }

            if let Err(e) = database::save_all_processed_info(&conn, &processed_results) {
                 eprintln!("Error saving data to database: {}", e);
                 // 保存エラーは警告に留め、処理は完了とするか？
                 // ここでは警告のみ表示
            }
        }
        Err(e) => {
            eprintln!("Error opening database connection to {:?}: {}", db_path, e);
            // DB接続エラーは致命的かもしれないので終了する
            process::exit(1);
        }
    }


    println!("\nAll tasks finished.");
}
