mod detours;
mod dll_proxy_builder;

use crate::detours::detour_enumerate_export_callback;
use crate::detours::{DetourEnumerateExports, ModuleExportResult};
use crate::dll_proxy_builder::DllProxyBuilder;

use clap::Parser;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{self, Write};
use std::os::raw::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use windows_sys::{Win32::Foundation::HMODULE, Win32::System::LibraryLoader::LoadLibraryW};

fn load_library(path: &str) -> Result<HMODULE, io::Error> {
    let wide: Vec<u16> = OsStr::new(path)
        .encode_wide()
        .chain(std::iter::once(0)) // null terminator
        .collect();

    let handle: HMODULE = unsafe { LoadLibraryW(wide.as_ptr()) };
    Ok(handle)
}

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    #[arg(short = 'i', help = "The path to the input binary.")]
    input: String,

    #[arg(short = 'o', help = "The path to the output directory.")]
    output: Option<String>,

    #[arg(
        long = "pack",
        default_value_t = false,
        help = "Pack the original binary into the source code."
    )]
    pack: bool,

    #[arg(
        long = "dll-main",
        default_value_t = false,
        help = "Generates dll_main.cc stub."
    )]
    dll_main: bool,
}

fn write_file(path: &Path, contents: &str) -> io::Result<()> {
    let mut f = File::create(path)?;
    f.write_all(contents.as_bytes())?;
    Ok(())
}

fn create_dll_proxy(
    input: &Path,
    output_dir: &Path,
    dll_main: bool,
    pack: bool,
    exports: &Vec<ModuleExportResult>,
) -> io::Result<()> {
    let binary_name = input.file_stem().unwrap().to_string_lossy().to_string();
    let mut dll_proxy_builder = DllProxyBuilder::new(binary_name, exports.to_vec(), pack.clone());

    if pack {
        assert!(
            write_file(
                &output_dir.join(format!("{}_binary.h", dll_proxy_builder.binary_name())),
                &dll_proxy_builder.generate_cc_binary_header(&input.to_path_buf()),
            )
            .is_ok()
        );
    }

    if dll_main {
        assert!(
            write_file(
                &output_dir.join("dll_main.cc"),
                &dll_proxy_builder.generate_cc_dll_main(),
            )
            .is_ok()
        );
    }

    assert!(
        write_file(
            &output_dir.join(format!("{}.h", &dll_proxy_builder.binary_name())),
            &dll_proxy_builder.generate_cc_header(),
        )
        .is_ok()
    );
    assert!(
        write_file(
            &output_dir.join(format!("{}.cc", &dll_proxy_builder.binary_name())),
            &dll_proxy_builder.generate_cc_source(),
        )
        .is_ok()
    );
    assert!(
        write_file(
            &output_dir.join(format!("{}_exports.asm", &dll_proxy_builder.binary_name())),
            &dll_proxy_builder.generate_cc_assembler(),
        )
        .is_ok()
    );
    assert!(
        write_file(
            &output_dir.join(format!("{}.def", &dll_proxy_builder.binary_name())),
            &dll_proxy_builder.generate_cc_definitions(),
        )
        .is_ok()
    );

    Ok(())
}

fn main() -> io::Result<()> {
    let args = Args::parse();
    let input = PathBuf::from(&args.input);

    if !input.exists() {
        eprintln!("error: input file does not exist.");
        std::process::exit(1);
    }

    let output_dir = match args.output {
        Some(dir) => PathBuf::from(dir),
        None => {
            let mut dir = std::env::current_dir()?;
            dir.push(input.file_stem().unwrap());
            dir
        }
    };

    if !output_dir.exists() {
        fs::create_dir_all(&output_dir)?;
    }

    let module: HMODULE = load_library(&args.input)?;

    if module == std::ptr::null_mut() {
        panic!("LoadLibraryW returned with 0");
    }

    let mut binary_exports: Vec<ModuleExportResult> = Vec::new();
    let ctx = &mut binary_exports as *mut _ as *mut c_void;

    unsafe {
        DetourEnumerateExports(module, ctx, Some(detour_enumerate_export_callback));
    }

    for export in &binary_exports {
        println!(
            "Ordinal={} Address={:p} Name={:?}",
            export.ordinal, export.code, export.name
        );
    }

    create_dll_proxy(
        &input,
        &output_dir,
        args.dll_main,
        args.pack,
        &binary_exports,
    )?;

    Ok(())
}
