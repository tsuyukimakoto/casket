use crate::config::Catalog;
use crate::scanner::FileInfo;
use chrono::{DateTime, Local, NaiveDateTime, TimeZone};
use exif::{Reader, Tag, In, Value};
use image::ImageFormat; // Added image import
use std::error::Error;
use std::fs::{self, File};
use std::io::{self, BufReader};
use std::path::{Path, PathBuf};

// --- エラー型定義 ---
type ProcessorResult<T> = Result<T, Box<dyn Error>>;

// --- 処理結果の情報 ---
#[derive(Debug)]
pub struct ProcessedInfo {
    pub original_path: PathBuf,
    pub data_dest_path: PathBuf,
    pub thumbnail_dest_path: Option<PathBuf>,
    pub metadata: Metadata,
}

// --- メタデータ構造体 ---
#[derive(Debug, Default)]
pub struct Metadata {
    pub datetime_original: Option<DateTime<Local>>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    // TODO: 他のメタデータフィールドを追加
}

/// 単一ファイルを処理する（コピー、メタデータ抽出、サムネイル生成）
pub fn process_file(
    file_info: &FileInfo,
    catalog: &Catalog,
) -> ProcessorResult<ProcessedInfo> {
    println!("Processing file: {:?}", file_info.path);

    // 1. メタデータ抽出
    let metadata = extract_exif_metadata(&file_info.path);
    println!("  Extracted Metadata: {:?}", metadata);

    // 2. 日付の特定 (メタデータ優先、なければファイル更新日時)
    let datetime_for_path = match metadata.datetime_original {
        Some(dt) => dt,
        None => {
            println!("  Original datetime not found in metadata, using file modification time.");
            let file_meta = fs::metadata(&file_info.path)?;
            let modified_time = file_meta.modified()?;
            DateTime::from(modified_time)
        }
    };

    let year = datetime_for_path.format("%Y").to_string();
    let month = datetime_for_path.format("%m").to_string();
    let day = datetime_for_path.format("%d").to_string();

    // 3. コピー先パス、サムネイル保存先パスの決定
    let data_dest_dir = catalog.data_path.join(&year).join(&month).join(&day);
    let thumbnail_dest_dir = catalog.thumbnail_path.join(&year).join(&month).join(&day);

    // 4. 保存先ディレクトリの作成 (存在しない場合)
    fs::create_dir_all(&data_dest_dir)?;
    fs::create_dir_all(&thumbnail_dest_dir)?;

    // 5. ファイル名の決定 (元のファイル名を使用)
    let file_name = file_info
        .path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid file path"))?;

    let data_dest_path = data_dest_dir.join(file_name);
    let thumbnail_dest_path_base = thumbnail_dest_dir.join(file_name);

    // 6. ファイルコピー
    println!("Copying {:?} to {:?}", file_info.path, data_dest_path);
    fs::copy(&file_info.path, &data_dest_path)?;

    // 7. サムネイル生成
    println!("Generating thumbnail for {:?}...", file_info.path);
    let thumbnail_dest_path = generate_thumbnail(&file_info.path, &thumbnail_dest_path_base)?;

    println!("Finished processing: {:?}", file_info.path);

    Ok(ProcessedInfo {
        original_path: file_info.path.clone(),
        data_dest_path,
        thumbnail_dest_path,
        metadata,
    })
}

// --- ヘルパー関数 ---

/// EXIF情報からメタデータ (日付, メーカー, モデル) を抽出する
fn extract_exif_metadata(file_path: &Path) -> Metadata {
    let mut metadata = Metadata::default();

    let file = match File::open(file_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("  Error opening file for EXIF reading {:?}: {}", file_path, e);
            return metadata;
        }
    };
    let mut bufreader = BufReader::new(&file);
    let exifreader = match Reader::new().read_from_container(&mut bufreader) {
        Ok(r) => r,
        Err(_) => {
            return metadata;
        }
    };

    // 日付 (DateTimeOriginal or DateTime)
    let date_tag = exifreader
        .get_field(Tag::DateTimeOriginal, In::PRIMARY)
        .or_else(|| exifreader.get_field(Tag::DateTime, In::PRIMARY));
    if let Some(field) = date_tag {
        if let Value::Ascii(ref vec) = field.value {
            if let Some(first_vec) = vec.get(0) {
                 if let Ok(datetime_str) = std::str::from_utf8(first_vec) {
                    if let Ok(naive_dt) =
                        NaiveDateTime::parse_from_str(datetime_str.trim(), "%Y:%m:%d %H:%M:%S")
                    {
                        match Local.from_local_datetime(&naive_dt) {
                            chrono::LocalResult::Single(local_dt) => metadata.datetime_original = Some(local_dt),
                            chrono::LocalResult::Ambiguous(dt1, _) => metadata.datetime_original = Some(dt1),
                            _ => eprintln!("  Could not convert NaiveDateTime to Local DateTime: {}", naive_dt),
                        }
                    } else {
                         eprintln!("  Failed to parse EXIF datetime string: '{}'", datetime_str);
                    }
                }
            }
        }
    }

    // メーカー (Make)
    if let Some(field) = exifreader.get_field(Tag::Make, In::PRIMARY) {
        metadata.camera_make = field.display_value().to_string().into();
    }

    // モデル (Model)
    if let Some(field) = exifreader.get_field(Tag::Model, In::PRIMARY) {
         metadata.camera_model = field.display_value().to_string().into();
    }

    // TODO: 他のメタデータも同様に抽出

    metadata
}

/// サムネイル生成
fn generate_thumbnail(
    source_path: &Path,
    dest_path_base: &Path,
) -> ProcessorResult<Option<PathBuf>> {
    const THUMBNAIL_WIDTH: u32 = 256; // サムネイルの幅

    // ファイルタイプに応じて処理を分岐
    let ext = source_path.extension().and_then(|s| s.to_str()).unwrap_or("");
    let format = match ImageFormat::from_extension(ext) {
        Some(fmt) => fmt,
        None => {
            // image クレートが拡張子からフォーマットを推測できない場合
            match ext.to_lowercase().as_str() {
                "nef" | "cr2" | "arw" | "dng" => {
                    // libraw-rs クレートで処理 (TODO)
                    println!("  (RAW thumbnail generation needed for {})", ext);
                    return Ok(None); // 仮実装: スキップ
                }
                "mov" | "mp4" | "avi" | "mts" => {
                    // ffmpeg-next クレートで処理 (TODO)
                    println!("  (Video thumbnail generation needed for {})", ext);
                    return Ok(None); // 仮実装: スキップ
                }
                _ => {
                    println!("  (Skipping thumbnail for unknown type: {})", ext);
                    return Ok(None); // サポート外の形式はスキップ
                }
            }
        }
    };

    // image クレートで処理可能なフォーマットの場合
    println!("  Generating image thumbnail for {:?} ({:?})", source_path, format);
    let img = match image::open(source_path) {
        Ok(img) => img,
        Err(e) => {
            // エラーの場合はサムネイル生成をスキップ (エラーログは出す)
            eprintln!("  Error opening image {:?}: {}", source_path, e);
            return Ok(None);
        }
    };

    // リサイズ (幅を基準にアスペクト比維持)
    let thumbnail = img.thumbnail(THUMBNAIL_WIDTH, THUMBNAIL_WIDTH);

    // 保存パス (.jpg)
    let mut thumbnail_path = dest_path_base.to_path_buf();
    thumbnail_path.set_extension("jpg");

    // JPEG形式で保存
    match thumbnail.save_with_format(&thumbnail_path, ImageFormat::Jpeg) {
        Ok(_) => {
            println!("  Thumbnail saved to {:?}", thumbnail_path);
            Ok(Some(thumbnail_path))
        }
        Err(e) => {
            // 保存エラーの場合もスキップ (エラーログは出す)
            eprintln!("  Error saving thumbnail {:?}: {}", thumbnail_path, e);
            Ok(None)
        }
    }
}

// Removed the old get_original_datetime function
// TODO: RAWファイル用に libraw-rs を使ってメタデータを取得する処理も extract_exif_metadata に統合検討
// TODO: 動画ファイル用に ffmpeg-next を使ってメタデータを取得する処理も extract_exif_metadata に統合検討
