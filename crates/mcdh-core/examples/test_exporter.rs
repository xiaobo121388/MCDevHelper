//! Standalone, read-only-input test packer for the custom export protocol.

use std::error::Error;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use walkdir::WalkDir;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

type TestResult<T> = Result<T, Box<dyn Error>>;
const OUTPUT_NAME: &str = "mcdh-test.zip";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Success,
    Slow,
    Fail,
    Empty,
    Multiple,
    Zero,
    Logs,
}

impl Mode {
    fn parse(value: &str) -> TestResult<Self> {
        match value {
            "success" => Ok(Self::Success),
            "slow" => Ok(Self::Slow),
            "fail" => Ok(Self::Fail),
            "empty" => Ok(Self::Empty),
            "multiple" => Ok(Self::Multiple),
            "zero" => Ok(Self::Zero),
            "logs" => Ok(Self::Logs),
            _ => Err(format!("Unknown mode: {value}").into()),
        }
    }
}

struct Options {
    input: PathBuf,
    output: PathBuf,
    mode: Mode,
}

fn parse_options(
    arguments: impl IntoIterator<Item = OsString>,
    input: Option<OsString>,
    output: Option<OsString>,
) -> TestResult<Options> {
    let (mut input, mut output, mut mode) = (input, output, Mode::Success);
    let mut arguments = arguments.into_iter();
    while let Some(flag) = arguments.next() {
        let value = arguments
            .next()
            .ok_or("Every option requires a separate value")?;
        match flag.to_str() {
            Some("--input") => input = Some(value),
            Some("--output-dir") => output = Some(value),
            Some("--mode") => mode = Mode::parse(value.to_str().ok_or("Mode must be UTF-8")?)?,
            _ => return Err(format!("Unknown option: {}", flag.to_string_lossy()).into()),
        }
    }
    Ok(Options {
        input: input
            .ok_or("Missing input: run from MCDH or supply --input")?
            .into(),
        output: output
            .ok_or("Missing output: run from MCDH or supply --output-dir")?
            .into(),
        mode,
    })
}

fn linked(path: &Path) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn create_zip(input: &Path, output: &Path) -> TestResult<usize> {
    let file = File::options().write(true).create_new(true).open(output)?;
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut count = 0;
    for entry in WalkDir::new(input)
        .follow_links(false)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|entry| !linked(entry.path()))
    {
        let entry = entry?;
        if entry.path() == input {
            continue;
        }
        let name = entry
            .path()
            .strip_prefix(input)?
            .to_string_lossy()
            .replace('\\', "/");
        if entry.file_type().is_dir() {
            archive.add_directory(format!("{name}/"), options)?;
        } else if entry.file_type().is_file() {
            archive.start_file(&name, options)?;
            io::copy(&mut File::open(entry.path())?, &mut archive)?;
        } else {
            continue;
        }
        count += 1;
        if count <= 10 || count % 100 == 0 {
            println!("[zip] entry {count}: {name}");
            io::stdout().flush()?;
        }
    }
    archive.finish()?.sync_all()?;
    Ok(count)
}

fn run(options: &Options, delay: Duration, steps: usize) -> TestResult<i32> {
    let input = fs::canonicalize(&options.input)?;
    let output = fs::canonicalize(&options.output)?;
    if !input.is_dir() || !output.is_dir() {
        return Err("Input and output must be existing directories".into());
    }
    if output.starts_with(&input) {
        return Err("Output must be outside the input directory".into());
    }
    if fs::read_dir(&output)?.next().is_some() {
        return Err("Output directory must be empty; no existing files will be overwritten".into());
    }
    println!("MCDH test packer / protocol v1");
    println!(
        "Mode: {:?}\nInput: {}\nOutput: {}",
        options.mode,
        input.display(),
        output.display()
    );
    println!("UTF-8: \u{4e2d}\u{6587}\u{65e5}\u{5fd7}\u{6d4b}\u{8bd5}");
    eprintln!("[stderr] Diagnostic stream test (not a failure).");
    for step in 1..=steps {
        println!("[progress] {step}/{steps} - test delay; source files are read-only");
        io::stdout().flush()?;
        std::thread::sleep(delay);
    }
    match options.mode {
        Mode::Fail => {
            eprintln!("[intentional failure] Exit code 23; no artifact created.");
            return Ok(23);
        }
        Mode::Empty => {
            println!("[intentional invalid output] No artifact created.");
            return Ok(0);
        }
        Mode::Zero => {
            File::options()
                .create_new(true)
                .write(true)
                .open(output.join(OUTPUT_NAME))?;
            println!("[intentional invalid output] Empty artifact created.");
            return Ok(0);
        }
        Mode::Logs => {
            let padding = "x".repeat(1000);
            let mut stdout = io::stdout().lock();
            for line in 0..2048 {
                writeln!(stdout, "[log-limit-test {line:04}] {padding}")?;
            }
            stdout.flush()?;
        }
        _ => {}
    }
    let count = create_zip(&input, &output.join(OUTPUT_NAME))?;
    if options.mode == Mode::Multiple {
        let mut extra = File::options()
            .create_new(true)
            .write(true)
            .open(output.join("unexpected-report.txt"))?;
        extra.write_all(b"Intentional second artifact for validation testing.\n")?;
    }
    println!("[done] {OUTPUT_NAME}; {count} entries. Input was not modified.");
    io::stdout().flush()?;
    Ok(0)
}

fn main() {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments
        .iter()
        .any(|value| value == "--help" || value == "-h")
    {
        println!(
            "MCDH-TestPacker.exe [--mode success|slow|fail|empty|multiple|zero|logs]\n  [--input DIRECTORY] [--output-dir EMPTY_DIRECTORY]\n\nPaths default to MCDH_INPUT_DIR and MCDH_OUTPUT_DIR.\nDefault: five 300ms progress steps, then mcdh-test.zip.\nSlow: sixty 1-second steps before packaging. Fail: exit code 23.\nInput files are never modified. Output must be empty and outside input."
        );
        return;
    }
    let result = parse_options(
        arguments,
        std::env::var_os("MCDH_INPUT_DIR"),
        std::env::var_os("MCDH_OUTPUT_DIR"),
    )
    .and_then(|options| {
        let (delay, steps) = if options.mode == Mode::Slow {
            (Duration::from_secs(1), 60)
        } else {
            (Duration::from_millis(300), 5)
        };
        run(&options, delay, steps)
    });
    let code = match result {
        Ok(code) => code,
        Err(error) => {
            eprintln!("[error] {error}");
            2
        }
    };
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_environment_defaults_and_argument_overrides() {
        let options = parse_options(
            ["--input", "project with spaces", "--mode", "multiple"].map(OsString::from),
            Some("old".into()),
            Some("out".into()),
        )
        .unwrap();
        assert_eq!(options.input, PathBuf::from("project with spaces"));
        assert_eq!(options.output, PathBuf::from("out"));
        assert_eq!(options.mode, Mode::Multiple);
        assert!(parse_options(["--unknown", "x"].map(OsString::from), None, None).is_err());
        assert!(parse_options(["--mode"].map(OsString::from), None, None).is_err());
        assert!(Mode::parse("typo").is_err());
    }

    #[test]
    fn packages_full_input_and_exercises_failure_modes_without_modifying_source() {
        let root = tempfile::tempdir().unwrap();
        let input = root.path().join("source");
        fs::create_dir_all(input.join(".empty")).unwrap();
        fs::write(input.join(".hidden"), "original").unwrap();
        fs::write(input.join("main.pyi"), "stub").unwrap();
        for mode in [
            Mode::Success,
            Mode::Fail,
            Mode::Empty,
            Mode::Multiple,
            Mode::Zero,
        ] {
            let output = tempfile::tempdir_in(root.path()).unwrap();
            let options = Options {
                input: input.clone(),
                output: output.path().to_path_buf(),
                mode,
            };
            let code = run(&options, Duration::ZERO, 0).unwrap();
            assert_eq!(code, if mode == Mode::Fail { 23 } else { 0 });
            let expected = match mode {
                Mode::Empty | Mode::Fail => 0,
                Mode::Multiple => 2,
                _ => 1,
            };
            assert_eq!(fs::read_dir(output.path()).unwrap().count(), expected);
            if mode == Mode::Success || mode == Mode::Multiple {
                let mut zip =
                    zip::ZipArchive::new(File::open(output.path().join(OUTPUT_NAME)).unwrap())
                        .unwrap();
                assert!(zip.by_name(".empty/").unwrap().is_dir());
                assert_eq!(zip.by_name(".hidden").unwrap().size(), 8);
                assert!(zip.by_name("main.pyi").is_ok());
            }
            if mode == Mode::Zero {
                assert_eq!(
                    fs::metadata(output.path().join(OUTPUT_NAME)).unwrap().len(),
                    0
                );
            }
            assert_eq!(
                fs::read_to_string(input.join(".hidden")).unwrap(),
                "original"
            );
        }
    }

    #[test]
    fn rejects_overwrites_and_output_inside_source() {
        let root = tempfile::tempdir().unwrap();
        let input = root.path().join("source");
        let output = root.path().join("output");
        fs::create_dir(&input).unwrap();
        fs::create_dir(&output).unwrap();
        fs::write(output.join("keep.txt"), "keep").unwrap();
        assert!(
            run(
                &Options {
                    input: input.clone(),
                    output: output.clone(),
                    mode: Mode::Success
                },
                Duration::ZERO,
                0
            )
            .is_err()
        );
        assert_eq!(fs::read_to_string(output.join("keep.txt")).unwrap(), "keep");
        fs::create_dir(input.join("nested")).unwrap();
        assert!(
            run(
                &Options {
                    input: input.clone(),
                    output: input.join("nested"),
                    mode: Mode::Success
                },
                Duration::ZERO,
                0
            )
            .is_err()
        );
    }
}
