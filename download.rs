use std::{env, io};
use std::error::Error;
use std::fs::File;
use std::io::Cursor;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::tempdir;
use ureq::{Agent, Proxy};
use ureq::tls::{TlsConfig, TlsProvider};
use zip_extract::extract;

fn download_file_curl<T: AsRef<Path>>(url: &str, target_dir: T) -> Result<(), Box<dyn Error>> {
    // Not using `CARGO_CFG_TARGET_OS` because of the possibility of
    // cross-compilation. When targeting `x86_64-pc-windows-gnu` on Linux for
    // example, `cfg!()` in the build script still reports `target_os =
    // "linux"`, which is desirable.
    let curl_bin_name = if cfg!(target_os = "windows") {
        // powershell aliases `curl` to `Invoke-WebRequest`
        "curl.exe"
    } else {
        "curl"
    };

    let mut args = Vec::with_capacity(6);
    args.extend([
        "-sSL",
        "-o",
        target_dir
            .as_ref()
            .as_os_str()
            .to_str()
            .expect("target dir should be valid utf-8"),
        url,
    ]);
    let cacert = env::var("CARGO_HTTP_CAINFO").unwrap_or_default();
    if !cacert.is_empty() {
        args.extend(["--cacert", &cacert]);
    }

    let download = std::process::Command::new(curl_bin_name)
        .args(args)
        .spawn()
        .and_then(|mut child| child.wait());

    Ok(download
        .and_then(|status| {
            if status.success() {
                Ok(())
            } else {
                Err(std::io::Error::new(
                    io::ErrorKind::Other,
                    format!("curl download file exited with error status: {status}"),
                ))
            }
        })
        .map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                io::Error::new(error.kind(), format!("`{curl_bin_name}` command not found"))
            } else {
                error
            }
        })
        .map_err(Box::new)?)
}

pub fn download_and_extract_zip(url: &str, extract_path: &Path) -> Result<(), Box<dyn Error>> {
    // Download the ZIP file
    println!("cargo:warning=Downloading from {}", url);
    let zip_path = extract_path.join("libscip.zip");
    download_file_curl(url, zip_path.clone())?;
    let target_dir = PathBuf::from(extract_path);

    println!("cargo:warning=Downloaded to {:?}", zip_path);
    println!("cargo:warning=Extracting to {:?}", target_dir);
    extract(
        Cursor::new(std::fs::read(zip_path).unwrap()),
        &target_dir,
        false,
    )?;

    // Check if the extracted content is another zip file
    let extracted_files: Vec<_> = std::fs::read_dir(&target_dir)?.collect();
    if extracted_files.len() == 1 {
        let first_file = extracted_files[0].as_ref().unwrap();
        if first_file
            .path()
            .extension()
            .map_or(false, |ext| ext == "zip")
        {
            println!("cargo:warning=Found nested zip file, extracting again");
            let nested_zip_path = first_file.path();
            extract(
                Cursor::new(std::fs::read(&nested_zip_path).unwrap()),
                &(target_dir.join("scip_install")),
                true,
            )?;
            std::fs::remove_file(nested_zip_path)?;
        }
    }
    Ok(())
}
