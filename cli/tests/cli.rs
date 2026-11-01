use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const SOURCE: &str = "Swf(version: 10, tags: [ShowFrame, End])";

/// Creates an empty directory for a test, containing the source file `in.swfasm`.
fn test_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("cli")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("in.swfasm"), SOURCE).unwrap();
    dir
}

fn swfasm(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_swfasm"))
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn writes_next_to_input_by_default() {
    let dir = test_dir("default_output");
    let output = swfasm(&dir, &["in.swfasm"]);
    assert!(output.status.success(), "{output:?}");
    let swf = std::fs::read(dir.join("in.swf")).unwrap();
    assert_eq!(&swf[..3], b"FWS");
}

#[test]
fn refuses_to_overwrite_input() {
    let dir = test_dir("overwrite_input");
    std::fs::hard_link(dir.join("in.swfasm"), dir.join("hard_link.swfasm")).unwrap();
    std::fs::create_dir(dir.join("sub")).unwrap();

    let absolute = dir.join("in.swfasm");
    let mut outputs = vec![
        "in.swfasm",
        "./in.swfasm",
        "sub/../in.swfasm",
        absolute.to_str().unwrap(),
        "hard_link.swfasm",
    ];
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("in.swfasm", dir.join("symlink.swfasm")).unwrap();
        outputs.push("symlink.swfasm");
    }

    for output_path in outputs {
        let output = swfasm(&dir, &["in.swfasm", "-o", output_path]);
        assert!(!output.status.success(), "{output_path}: {output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("Refusing to overwrite the input file"),
            "{output_path}: {output:?}"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("in.swfasm")).unwrap(),
            SOURCE,
            "{output_path}"
        );
    }
}

#[test]
fn refuses_to_overwrite_swf_input() {
    let dir = test_dir("swf_input");
    std::fs::rename(dir.join("in.swfasm"), dir.join("in.swf")).unwrap();
    let output = swfasm(&dir, &["in.swf"]);
    assert!(!output.status.success(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Refusing to overwrite the input file"),
        "{output:?}"
    );
    assert_eq!(std::fs::read_to_string(dir.join("in.swf")).unwrap(), SOURCE);
}

#[test]
fn reads_files_relative_to_input() {
    let dir = test_dir("relative_files");
    std::fs::create_dir(dir.join("sub")).unwrap();
    std::fs::write(
        dir.join("sub/in.swfasm"),
        r#"Swf(version: 10, tags: [DefineBinaryData(id: 1, data: File("data.bin"))])"#,
    )
    .unwrap();
    std::fs::write(dir.join("sub/data.bin"), b"\x01\x02\x03").unwrap();
    // A file of the same name next to the working directory must not be read.
    std::fs::write(dir.join("data.bin"), b"wrong").unwrap();

    for (cwd, input) in [
        (dir.clone(), "sub/in.swfasm"),
        (dir.join("sub"), "in.swfasm"),
    ] {
        let output = swfasm(&cwd, &[input, "-o", "out.swf"]);
        assert!(output.status.success(), "{input}: {output:?}");
        let swf = std::fs::read(cwd.join("out.swf")).unwrap();
        assert!(swf.ends_with(b"\x01\x02\x03"), "{input}: {swf:?}");
    }
}

#[test]
fn reports_missing_files() {
    let dir = test_dir("missing_file");
    std::fs::write(
        dir.join("in.swfasm"),
        r#"Swf(version: 10, tags: [DefineBinaryData(id: 1, data: File("missing.bin"))])"#,
    )
    .unwrap();
    let output = swfasm(&dir, &["in.swfasm"]);
    assert!(!output.status.success(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(r#"failed to read file "missing.bin""#),
        "{output:?}"
    );
    assert!(!dir.join("in.swf").exists());
}

#[test]
fn rejects_files_outside_input_directory() {
    let dir = test_dir("outside_file");
    std::fs::create_dir(dir.join("sub")).unwrap();
    std::fs::write(
        dir.join("sub/in.swfasm"),
        r#"Swf(version: 10, tags: [DefineBinaryData(id: 1, data: File("../data.bin"))])"#,
    )
    .unwrap();
    std::fs::write(dir.join("data.bin"), b"outside").unwrap();
    let output = swfasm(&dir, &["sub/in.swfasm"]);
    assert!(!output.status.success(), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(r#"failed to read file "../data.bin""#),
        "{output:?}"
    );
    assert!(
        stderr.contains("the path leads outside of the directory"),
        "{output:?}"
    );
    assert!(!dir.join("sub/in.swf").exists());
}
