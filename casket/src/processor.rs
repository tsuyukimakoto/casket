use crate::config::Catalog;
use crate::scanner::FileInfo;
use chrono::{DateTime, Local, NaiveDateTime, TimeZone};
use exif;
use image::{ImageFormat, DynamicImage, codecs::jpeg::JpegEncoder};
use libraw::{Processor};
use std::error::Error;
use std::fs::{self, File};
use std::io::{self, BufReader, Read, Seek};
use std::path::{Path, PathBuf};
use std::process::Command;

// --- エラー型定義 ---
type ProcessorResult<T> = Result<T, Box<dyn Error>>;

// --- 処理結果の情報 ---
#[derive(Debug)]
pub struct ProcessedInfo {
    pub original_path: PathBuf,
    pub data_dest_path: PathBuf,
    pub thumbnail_dest_path: Option<PathBuf>,
    pub metadata: Metadata,
    pub datetime_indexed: String, // YYYYMMDDHH形式の絞り込み用日時
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

    // 日時インデックス生成
    let datetime_indexed = match get_datetime_indexed(&file_info.path, &metadata) {
        Ok(dt_indexed) => dt_indexed,
        Err(e) => {
            eprintln!("Error generating datetime index for {:?}: {}", file_info.path, e);
            // フォールバック: 現在時刻を使用
            let now = Local::now();
            format_datetime_indexed(now)
        }
    };

    println!("Finished processing: {:?} (indexed: {})", file_info.path, datetime_indexed);

    Ok(ProcessedInfo {
        original_path: file_info.path.clone(),
        data_dest_path,
        thumbnail_dest_path,
        metadata,
        datetime_indexed,
    })
}

// --- ヘルパー関数 ---

/// 拡大を防ぐリサイズ関数。最大サイズより小さい場合は元のサイズを保持
fn resize_without_upscaling(img: DynamicImage, max_size: u32) -> DynamicImage {
    let (width, height) = (img.width(), img.height());
    let max_dimension = width.max(height);
    
    if max_dimension <= max_size {
        // 元画像が最大サイズより小さい場合はそのまま返す
        println!("  Image size {}x{} is smaller than max {}, keeping original size", 
                width, height, max_size);
        img
    } else {
        // 長辺を基準にアスペクト比を保ってリサイズ
        let thumbnail = img.thumbnail(max_size, max_size);
        println!("  Resized from {}x{} to {}x{}", 
                width, height, thumbnail.width(), thumbnail.height());
        thumbnail
    }
}

/// 日時をYYYYMMDDHH形式にフォーマットする関数
fn format_datetime_indexed(dt: DateTime<Local>) -> String {
    dt.format("%Y%m%d%H").to_string()
}

/// ファイルから日時を取得し、YYYYMMDDHH形式でフォーマット
/// 撮影日時が取得できない場合はファイル作成日時を使用
fn get_datetime_indexed(file_path: &Path, metadata: &Metadata) -> Result<String, Box<dyn Error>> {
    if let Some(datetime_original) = metadata.datetime_original {
        // EXIFから撮影日時が取得できた場合
        println!("  Using EXIF datetime for indexing: {}", datetime_original);
        Ok(format_datetime_indexed(datetime_original))
    } else {
        // EXIFから取得できない場合はファイル作成日時を使用
        let file_meta = std::fs::metadata(file_path)?;
        let created_time = file_meta.created()
            .or_else(|_| file_meta.modified())?; // 作成日時が取得できない場合は更新日時
        let datetime = DateTime::from(created_time);
        println!("  Using file creation time for indexing: {}", datetime);
        Ok(format_datetime_indexed(datetime))
    }
}

/// クオリティ指定でJPEGサムネイルを保存するヘルパー関数
fn save_jpeg_thumbnail(
    img: &DynamicImage,
    path: &Path,
    quality: u8, // 1-10 scale
) -> Result<(), Box<dyn Error>> {
    // 1-10スケールを0-100スケールに変換 (1=10%, 10=100%)
    let jpeg_quality = (quality * 10).min(100);
    
    let file = File::create(path)?;
    let mut encoder = JpegEncoder::new_with_quality(file, jpeg_quality);
    
    let rgb_image = img.to_rgb8();
    encoder.encode(
        rgb_image.as_raw(),
        img.width(),
        img.height(),
        image::ExtendedColorType::Rgb8,
    )?;
    
    println!("  Saved JPEG thumbnail with quality {} ({}%) to {:?}", 
            quality, jpeg_quality, path);
    Ok(())
}

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
    let exifreader = match exif::Reader::new().read_from_container(&mut bufreader) {
        Ok(r) => r,
        Err(_) => {
            return metadata;
        }
    };

    // 日付 (DateTimeOriginal or DateTime)
    let date_tag = exifreader
        .get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY)
        .or_else(|| exifreader.get_field(exif::Tag::DateTime, exif::In::PRIMARY));
    if let Some(field) = date_tag {
        if let exif::Value::Ascii(ref vec) = field.value {
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
    if let Some(field) = exifreader.get_field(exif::Tag::Make, exif::In::PRIMARY) {
        metadata.camera_make = Some(field.display_value().to_string());
    }

    // モデル (Model)
    if let Some(field) = exifreader.get_field(exif::Tag::Model, exif::In::PRIMARY) {
         metadata.camera_model = Some(field.display_value().to_string());
    }

    // TODO: 他のメタデータも同様に抽出

    metadata
}

/// サムネイル生成
fn generate_thumbnail(
    source_path: &Path,
    dest_path_base: &Path,
) -> ProcessorResult<Option<PathBuf>> {
    const THUMBNAIL_MAX_SIZE: u32 = 2048; // サムネイルの最大長辺サイズ
    const THUMBNAIL_QUALITY: u8 = 6; // デフォルトのJPEGクオリティ (1-10, 10が最高画質)

    // ファイルタイプに応じて処理を分岐
    let ext = source_path.extension().and_then(|s| s.to_str()).unwrap_or("");
    let format = match ImageFormat::from_extension(ext) {
        Some(fmt) => fmt,
        None => {
            // image クレートが拡張子からフォーマットを推測できない場合
            match ext.to_lowercase().as_str() {
                "nef" | "cr2" | "arw" | "dng" => {
                    // RAWファイル処理
                    println!("  Processing RAW file: {}", ext);
                    match generate_raw_thumbnail(source_path, THUMBNAIL_MAX_SIZE) {
                        Ok(Some(thumb)) => {
                            let mut thumbnail_path = dest_path_base.to_path_buf();
                            thumbnail_path.set_extension("jpg");
                            match save_jpeg_thumbnail(&thumb, &thumbnail_path, THUMBNAIL_QUALITY) {
                                Ok(_) => {
                                    return Ok(Some(thumbnail_path));
                                }
                                Err(e) => {
                                    eprintln!("  Error saving RAW thumbnail {:?}: {}", thumbnail_path, e);
                                    return Ok(None);
                                }
                            }
                        }
                        Ok(None) => {
                            println!("  Could not generate thumbnail from RAW file {:?}", source_path);
                            return Ok(None);
                        }
                        Err(e) => {
                            eprintln!("  Error processing RAW file {:?}: {}", source_path, e);
                            return Ok(None);
                        }
                    }
                }
                "heic" | "heif" => {
                    // HEIC/HEIF処理
                    println!("  Processing HEIC/HEIF file: {}", ext);
                    match generate_heic_thumbnail(source_path, THUMBNAIL_MAX_SIZE) {
                        Ok(Some(thumb)) => {
                            let mut thumbnail_path = dest_path_base.to_path_buf();
                            thumbnail_path.set_extension("jpg");
                            match save_jpeg_thumbnail(&thumb, &thumbnail_path, THUMBNAIL_QUALITY) {
                                Ok(_) => {
                                    return Ok(Some(thumbnail_path));
                                }
                                Err(e) => {
                                    eprintln!("  Error saving HEIC thumbnail {:?}: {}", thumbnail_path, e);
                                    return Ok(None);
                                }
                            }
                        }
                        Ok(None) => {
                            println!("  Could not generate thumbnail from HEIC file {:?}", source_path);
                            return Ok(None);
                        }
                        Err(e) => {
                            eprintln!("  Error processing HEIC file {:?}: {}", source_path, e);
                            return Ok(None);
                        }
                    }
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

    // リサイズ (拡大防止機能付き)
    let thumbnail = resize_without_upscaling(img, THUMBNAIL_MAX_SIZE);

    // 保存パス (.jpg)
    let mut thumbnail_path = dest_path_base.to_path_buf();
    thumbnail_path.set_extension("jpg");

    // JPEG形式で保存 (クオリティ指定)
    match save_jpeg_thumbnail(&thumbnail, &thumbnail_path, THUMBNAIL_QUALITY) {
        Ok(_) => {
            Ok(Some(thumbnail_path))
        }
        Err(e) => {
            eprintln!("  Error saving image thumbnail {:?}: {}", thumbnail_path, e);
            Ok(None)
        }
    }
}

/// libraw-rs を使ってRAWファイルのサムネイルを生成するヘルパー関数
fn generate_raw_thumbnail(
    raw_path: &Path,
    target_width: u32,
) -> Result<Option<DynamicImage>, Box<dyn Error>> {
    // ファイルを読み込む (libraw-rs はバイトバッファを受け取る)
    let file_data = std::fs::read(raw_path)?;
    
    // Processorを作成してRAW画像を処理
    let processor = Processor::new();
    
    // RAW画像を8ビットRGBで処理
    println!("  Processing RAW image to RGB...");
    let processed_image = match processor.process_8bit(&file_data) {
        Ok(img) => img,
        Err(e) => {
            eprintln!("  Failed to process RAW file: {}", e);
            println!("  Attempting alternative processing methods...");
            
            // 1. 16ビット処理を試行
            match Processor::new().process_16bit(&file_data) {
                Ok(img16) => {
                    // 16ビットから8ビットに変換
                    let width = img16.width();
                    let height = img16.height();
                    let data16: &[u16] = &img16;
                    let data8: Vec<u8> = data16.iter().map(|&x| (x >> 8) as u8).collect();
                    
                    if let Some(image_buffer) = image::ImageBuffer::from_raw(width, height, data8) {
                        let dynamic_img = DynamicImage::ImageRgb8(image_buffer);
                        let thumbnail = resize_without_upscaling(dynamic_img, target_width);
                        println!("  RAW thumbnail generated via 16-bit fallback: {}x{} -> {}x{}", 
                                width, height, thumbnail.width(), thumbnail.height());
                        return Ok(Some(thumbnail));
                    }
                }
                Err(e2) => {
                    eprintln!("  16-bit processing also failed: {}", e2);
                }
            }
            
            // 2. 埋め込みプレビュー画像の抽出を試行（特にDNGファイル用）
            println!("  Attempting to extract embedded preview image...");
            match extract_dng_preview(raw_path) {
                Ok(Some(preview_img)) => {
                    let (orig_width, orig_height) = (preview_img.width(), preview_img.height());
                    let thumbnail = resize_without_upscaling(preview_img, target_width);
                    println!("  RAW thumbnail generated from embedded preview: {}x{} -> {}x{}", 
                            orig_width, orig_height, thumbnail.width(), thumbnail.height());
                    return Ok(Some(thumbnail));
                }
                Ok(None) => {
                    println!("  No embedded preview found");
                }
                Err(e3) => {
                    eprintln!("  Preview extraction failed: {}", e3);
                }
            }
            
            // 3. 最終手段: sipsコマンドでDNGをJPEGに変換 (macOS)
            if raw_path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase() == "dng" {
                println!("  Attempting DNG conversion using sips...");
                match convert_dng_with_sips(raw_path, target_width) {
                    Ok(Some(thumb)) => {
                        println!("  DNG thumbnail generated via sips conversion: {}x{}", 
                                thumb.width(), thumb.height());
                        return Ok(Some(thumb));
                    }
                    Ok(None) => {
                        println!("  sips conversion failed");
                    }
                    Err(e4) => {
                        eprintln!("  sips conversion error: {}", e4);
                    }
                }
            }
            
            return Ok(None);
        }
    };
    
    let width = processed_image.width();
    let height = processed_image.height();
    let rgb_data: &[u8] = &processed_image;
    
    // RGB8データからDynamicImageを作成
    // libraw-rs のProcessedImageは3チャンネル(RGB)のデータを返す
    // データサイズが期待値と一致するかチェック
    let expected_size = (width * height * 3) as usize; // RGB = 3 bytes per pixel
    if rgb_data.len() == expected_size {
        if let Some(image_buffer) = image::ImageBuffer::from_raw(width, height, rgb_data.to_vec()) {
            let dynamic_img = DynamicImage::ImageRgb8(image_buffer);
            let thumbnail = resize_without_upscaling(dynamic_img, target_width);
            return Ok(Some(thumbnail));
        }
    } else {
        eprintln!("  RGB data size mismatch: expected {}, got {}", expected_size, rgb_data.len());
    }
    
    Ok(None)
}

/// HEIC/HEIFファイルのサムネイルを生成するヘルパー関数
/// macOSのsipsコマンドを使用してHEICをJPEGに変換してからサムネイル生成
fn generate_heic_thumbnail(
    heic_path: &Path,
    target_width: u32,
) -> Result<Option<DynamicImage>, Box<dyn Error>> {
    // 一時的な変換ファイルパス
    let temp_dir = std::env::temp_dir();
    let temp_file = temp_dir.join(format!("casket_temp_{}.jpg", 
        std::process::id()));
    
    println!("  Converting HEIC to JPEG using sips...");
    
    // sipsコマンドでHEICをJPEGに変換
    let output = Command::new("sips")
        .arg("-s")
        .arg("format")
        .arg("jpeg")
        .arg(heic_path)
        .arg("--out")
        .arg(&temp_file)
        .output()?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("  sips command failed: {}", stderr);
        return Ok(None);
    }
    
    // 変換されたJPEGファイルからサムネイルを生成
    let result = if temp_file.exists() {
        match image::open(&temp_file) {
            Ok(img) => {
                let thumbnail = resize_without_upscaling(img, target_width);
                println!("  HEIC thumbnail generated via sips conversion: {}x{}", 
                        thumbnail.width(), thumbnail.height());
                Some(thumbnail)
            }
            Err(e) => {
                eprintln!("  Error opening converted JPEG: {}", e);
                None
            }
        }
    } else {
        eprintln!("  Converted JPEG file not found");
        None
    };
    
    // 一時ファイルを削除
    if temp_file.exists() {
        let _ = std::fs::remove_file(&temp_file);
    }
    
    Ok(result)
}

/// DNG/RAWファイルから埋め込みプレビュー画像を抽出する関数
/// EXIFメタデータを使用してプレビュー画像のオフセットと長さを取得
fn extract_dng_preview(dng_path: &Path) -> Result<Option<DynamicImage>, Box<dyn Error>> {
    let file = File::open(dng_path)?;
    let mut bufreader = BufReader::new(&file);
    
    // EXIFデータからプレビュー情報を取得
    let exif_reader = match exif::Reader::new().read_from_container(&mut bufreader) {
        Ok(reader) => reader,
        Err(_) => return Ok(None),
    };
    
    // プレビュー画像の開始位置とサイズを取得（IFD1とPRIMALYの両方を試行）
    let mut preview_start = None;
    let mut preview_length = None;
    
    // IFD1を試行（一般的にDNGのプレビュー画像が格納される場所）
    for ifd in [exif::In::THUMBNAIL, exif::In::PRIMARY] {
        if preview_start.is_none() {
            preview_start = exif_reader
                .get_field(exif::Tag::JPEGInterchangeFormat, ifd)
                .and_then(|field| field.value.get_uint(0));
        }
        
        if preview_length.is_none() {
            preview_length = exif_reader
                .get_field(exif::Tag::JPEGInterchangeFormatLength, ifd)
                .and_then(|field| field.value.get_uint(0));
        }
        
        if preview_start.is_some() && preview_length.is_some() {
            println!("  Found JPEG preview in {:?} IFD", ifd);
            break;
        }
    }
    
    if let (Some(start), Some(length)) = (preview_start, preview_length) {
        println!("  Found preview image at offset {} with length {}", start, length);
        
        // ファイルから該当部分を読み込み
        let mut file = File::open(dng_path)?;
        let mut buffer = vec![0u8; length as usize];
        
        file.seek(std::io::SeekFrom::Start(start as u64))?;
        file.read_exact(&mut buffer)?;
        
        // 画像データとして読み込み
        match image::load_from_memory(&buffer) {
            Ok(img) => {
                println!("  Successfully loaded embedded preview image: {}x{}", img.width(), img.height());
                return Ok(Some(img));
            }
            Err(e) => {
                eprintln!("  Failed to load preview image data: {}", e);
            }
        }
    } else {
        println!("  No preview image metadata found in EXIF");
    }
    
    Ok(None)
}

/// sipsコマンドを使ってDNGファイルをJPEGに変換してサムネイル生成
fn convert_dng_with_sips(
    dng_path: &Path,
    target_width: u32,
) -> Result<Option<DynamicImage>, Box<dyn Error>> {
    // 一時的な変換ファイルパス
    let temp_dir = std::env::temp_dir();
    let temp_file = temp_dir.join(format!("casket_dng_temp_{}.jpg", 
        std::process::id()));
    
    // sipsコマンドでDNGをJPEGに変換
    let output = Command::new("sips")
        .arg("-s")
        .arg("format")
        .arg("jpeg")
        .arg(dng_path)
        .arg("--out")
        .arg(&temp_file)
        .output()?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("  sips DNG conversion failed: {}", stderr);
        return Ok(None);
    }
    
    // 変換されたJPEGファイルからサムネイルを生成
    let result = if temp_file.exists() {
        match image::open(&temp_file) {
            Ok(img) => {
                let thumbnail = resize_without_upscaling(img, target_width);
                Some(thumbnail)
            }
            Err(e) => {
                eprintln!("  Error opening converted DNG JPEG: {}", e);
                None
            }
        }
    } else {
        eprintln!("  Converted DNG JPEG file not found");
        None
    };
    
    // 一時ファイルを削除
    if temp_file.exists() {
        let _ = std::fs::remove_file(&temp_file);
    }
    
    Ok(result)
}


// Removed the old get_original_datetime function
// TODO: RAWファイル用に libraw-rs を使ってメタデータを取得する処理も extract_exif_metadata に統合検討
// TODO: 動画ファイル用に ffmpeg-next を使ってメタデータを取得する処理も extract_exif_metadata に統合検討
