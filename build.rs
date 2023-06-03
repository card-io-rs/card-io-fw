use libflate::gzip;
use std::{
    borrow::Cow,
    fs::{self, File},
    io::{BufReader, Read, Write},
    path::Path,
};

fn compress_files_individually(source: impl AsRef<Path>, dst: impl AsRef<Path>) {
    // Extract file names from ".compress" file
    let compressed_list_file = source.as_ref().join(".compress");
    let Ok(compressed_files) = std::fs::read_to_string(&compressed_list_file)
    else {
        return;
    };
    println!("cargo:rerun-if-changed={}", compressed_list_file.display());

    let file_names = compressed_files
        .lines()
        .map(|line| line.trim())
        .collect::<Vec<_>>();

    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let file_name = path.file_name().unwrap();

        let dst = dst.as_ref().join(file_name);
        if path.is_dir() {
            compress_files_individually(&path, dst);
        } else if file_names.contains(&file_name.to_str().unwrap()) {
            let new_extension = if let Some(extension) = dst.extension() {
                Cow::Owned(format!("{}.gz", extension.to_str().unwrap()))
            } else {
                Cow::Borrowed("gz")
            };
            compress_file(&path, dst.with_extension(&*new_extension));
        }
    }
}

fn compress_file(source: impl AsRef<Path>, dst: impl AsRef<Path>) {
    println!("cargo:rerun-if-changed={}", source.as_ref().display());

    let file = File::open(&source).unwrap();
    let mut reader = BufReader::new(file);

    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer).unwrap();

    let mut encoder = gzip::Encoder::new(Vec::new()).unwrap();
    encoder.write_all(&buffer).unwrap();
    let compressed_bytes = encoder.finish().into_result().unwrap();

    fs::create_dir_all(dst.as_ref().parent().unwrap()).unwrap();
    fs::write(dst, compressed_bytes).unwrap()
}

fn main() {
    // Ensure that only a single MCU is specified.
    let mcu_features = [cfg!(feature = "esp32s2"), cfg!(feature = "esp32s3")];

    match mcu_features.iter().filter(|&&f| f).count() {
        1 => {}
        n => panic!("Exactly 1 MCU must be selected via its Cargo feature, {n} provided"),
    }

    let out = std::env::var("OUT_DIR").unwrap();
    let out_dir = Path::new(&out);

    println!("cargo:rustc-env=COMPRESS_OUT_DIR={out}");

    compress_files_individually("src/static", out_dir.join("static/"));
}
