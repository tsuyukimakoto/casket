use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// スキャン結果として返すファイル情報
#[derive(Debug)]
pub struct FileInfo {
    pub path: PathBuf,
    // 必要に応じて他の情報（ファイルサイズ、更新日時など）を追加
}

/// 指定されたディレクトリを再帰的にスキャンし、ファイルリストを取得する
pub fn scan_directory(dir_path: &Path) -> io::Result<Vec<FileInfo>> {
    let mut files = Vec::new();
    println!("Scanning directory: {:?}", dir_path); // デバッグ用

    if !dir_path.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Provided path is not a directory",
        ));
    }

    for entry in fs::read_dir(dir_path)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            // サブディレクトリを再帰的にスキャン
            let mut sub_files = scan_directory(&path)?;
            files.append(&mut sub_files);
        } else if path.is_file() {
            // ファイル情報をリストに追加
            // ここでファイルの種類（画像、動画など）を判定することも可能
            println!("Found file: {:?}", path); // デバッグ用
            files.push(FileInfo { path });
        }
    }

    Ok(files)
}
