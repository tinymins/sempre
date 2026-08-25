use std::{fs, io::Write};

use flate2::{Compression, write::GzEncoder};
use sempre_artifact::{ArchiveFormat, ExtractOptions, extract, find};
use tempfile::tempdir;
use zip::{ZipWriter, write::SimpleFileOptions};

#[test]
fn extracts_and_finds_zip_payload() {
    let root = tempdir().expect("temporary directory");
    let archive = root.path().join("core.zip");
    let file = fs::File::create(&archive).expect("archive");
    let mut writer = ZipWriter::new(file);
    writer
        .start_file(
            "release/bin/Core",
            SimpleFileOptions::default().unix_permissions(0o755),
        )
        .expect("ZIP entry");
    writer.write_all(b"binary").expect("ZIP payload");
    writer.finish().expect("finish ZIP");

    let destination = root.path().join("out");
    extract(
        &archive,
        &destination,
        &ExtractOptions {
            format: ArchiveFormat::Zip,
            single_file_name: None,
        },
    )
    .expect("extract ZIP");
    assert_eq!(
        fs::read(find(&destination, "core").expect("core")).expect("payload"),
        b"binary"
    );
}

#[test]
fn rejects_zip_path_traversal() {
    let root = tempdir().expect("temporary directory");
    let archive = root.path().join("evil.zip");
    let file = fs::File::create(&archive).expect("archive");
    let mut writer = ZipWriter::new(file);
    writer
        .start_file("../evil", SimpleFileOptions::default())
        .expect("ZIP entry");
    writer.write_all(b"evil").expect("ZIP payload");
    writer.finish().expect("finish ZIP");

    assert!(
        extract(
            &archive,
            &root.path().join("out"),
            &ExtractOptions {
                format: ArchiveFormat::Zip,
                single_file_name: None,
            },
        )
        .is_err()
    );
    assert!(!root.path().join("evil").exists());
}

#[test]
fn extracts_tar_gzip_and_single_file_formats() {
    let root = tempdir().expect("temporary directory");
    let tar_gz = root.path().join("core.tar.gz");
    let encoder = GzEncoder::new(
        fs::File::create(&tar_gz).expect("tar.gz"),
        Compression::fast(),
    );
    let mut tar = tar::Builder::new(encoder);
    let mut header = tar::Header::new_gnu();
    header.set_size(3);
    header.set_mode(0o755);
    header.set_cksum();
    tar.append_data(&mut header, "release/core", &b"tar"[..])
        .expect("tar payload");
    tar.into_inner()
        .expect("finish tar")
        .finish()
        .expect("finish gzip");
    let tar_out = root.path().join("tar-out");
    extract(
        &tar_gz,
        &tar_out,
        &ExtractOptions {
            format: ArchiveFormat::TarGz,
            single_file_name: None,
        },
    )
    .expect("extract tar.gz");
    assert_eq!(
        fs::read(find(&tar_out, "core").expect("core")).expect("payload"),
        b"tar"
    );

    let gzip = root.path().join("core.gz");
    let mut encoder = GzEncoder::new(fs::File::create(&gzip).expect("gzip"), Compression::fast());
    encoder.write_all(b"gzip").expect("gzip payload");
    encoder.finish().expect("finish gzip");
    let gzip_out = root.path().join("gzip-out");
    extract(
        &gzip,
        &gzip_out,
        &ExtractOptions {
            format: ArchiveFormat::Gzip,
            single_file_name: Some("core".into()),
        },
    )
    .expect("extract gzip");
    assert_eq!(fs::read(gzip_out.join("core")).expect("payload"), b"gzip");

    let raw = root.path().join("raw-core");
    fs::write(&raw, b"raw").expect("raw payload");
    let raw_out = root.path().join("raw-out");
    extract(
        &raw,
        &raw_out,
        &ExtractOptions {
            format: ArchiveFormat::Raw,
            single_file_name: Some("core".into()),
        },
    )
    .expect("extract raw");
    assert_eq!(fs::read(raw_out.join("core")).expect("payload"), b"raw");
}
