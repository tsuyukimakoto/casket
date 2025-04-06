use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Deserialize, Debug, Clone)]
pub struct Catalog {
    /// オリジナルファイル保存先パス
    pub data_path: PathBuf,
    /// サムネイル保存先パス (データベースファイルもここに配置)
    pub thumbnail_path: PathBuf,
}

#[derive(Deserialize, Debug, Default)]
pub struct Config {
    #[serde(flatten)]
    pub catalogs: HashMap<String, Catalog>,
}

/// 設定ファイルのデフォルトパスを取得
fn default_config_path() -> Result<PathBuf, io::Error> {
    // macOSの標準的な設定ディレクトリ (~/Library/Application Support) を使うことも検討
    // ここでは ~/.config/casket/catalogs.toml を仮のデフォルトとする
    dirs::config_dir()
        .map(|p| p.join("casket").join("catalogs.toml"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Config directory not found"))
}

/// 設定ファイルを読み込む
pub fn load_config() -> Result<Config, Box<dyn std::error::Error>> {
    let config_path = default_config_path()?;
    load_config_from_path(&config_path)
}

/// 指定されたパスから設定ファイルを読み込む
pub fn load_config_from_path(path: &Path) -> Result<Config, Box<dyn std::error::Error>> {
    println!("Loading config from: {:?}", path); // デバッグ用
    if !path.exists() {
        // 設定ファイルが存在しない場合は空の設定を返すか、エラーとするか？
        // ここでは空の設定を返す（カタログ未定義状態）
        println!("Config file not found, returning default empty config.");
        return Ok(Config::default());
    }

    let content = fs::read_to_string(path)?;
    let config: Config = toml::from_str(&content)?;
    Ok(config)
}

// 設定ファイルが存在しない場合にデフォルト設定で作成する関数なども検討可能
// pub fn ensure_config_file_exists() -> Result<PathBuf, io::Error> { ... }
