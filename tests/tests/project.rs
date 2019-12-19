use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

pub fn target_tests() -> PathBuf {
    let mut path = env::current_exe().unwrap();
    path.pop(); // chop off exe name
    path.pop(); // chop off 'deps'
    path.pop(); // chop off 'debug'
    path.push("tests");
    fs::create_dir_all(&path).unwrap();
    path
}

pub fn next_root() -> PathBuf {
    static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
    std::thread_local! {
        static TEST_ID: usize = NEXT_ID.fetch_add(1, SeqCst);
    }
    let id = TEST_ID.with(|n| *n);
    target_tests().join(&format!("t{}", id))
}

pub fn project(name: &str) -> ProjectBuilder {
    ProjectBuilder::new(name, next_root())
}

pub struct Project {
    name: String,
    root: PathBuf,
}

pub struct ProjectBuilder {
    project: Project,
    saw_manifest: bool,
    saw_main_or_lib: bool,
}

impl ProjectBuilder {
    pub fn new(name: &str, root: PathBuf) -> ProjectBuilder {
        println!(" ============ {} =============== ", root.display());
        drop(fs::remove_dir_all(&root));
        fs::create_dir_all(&root).unwrap();
        ProjectBuilder {
            project: Project {
                name: name.to_string(),
                root,
            },
            saw_manifest: false,
            saw_main_or_lib: false,
        }
    }

    pub fn root(&self) -> PathBuf {
        self.project.root()
    }

    pub fn with_fuzz(&mut self) -> &mut Self {
        self.file(
            Path::new("fuzz").join("Cargo.toml"),
            &format!(
                r#"
                    [package]
                    name = "{name}-fuzz"
                    version = "0.0.0"
                    authors = ["Automatically generated"]
                    publish = false
                    edition = "2018"

                    [package.metadata]
                    cargo-fuzz = true

                    [workspace]
                    members = ["."]

                    [dependencies.{name}]
                    path = ".."

                    [dependencies.libfuzzer-sys]
                    git = "https://github.com/rust-fuzz/libfuzzer-sys.git"
                "#,
                name = self.project.name,
            ),
        )
    }

    pub fn fuzz_target(&mut self, name: &str, body: &str) -> &mut Self {
        let path = self.project.fuzz_target_path(name);

        let mut fuzz_cargo_toml = fs::OpenOptions::new()
            .write(true)
            .append(true)
            .open(self.project.fuzz_dir().join("Cargo.toml"))
            .unwrap();
        write!(
            &mut fuzz_cargo_toml,
            r#"
                [[bin]]
                name = "{name}"
                path = "{path}"
            "#,
            name = name,
            path = path.display(),
        )
        .unwrap();

        self.file(path, body)
    }

    pub fn file<B: AsRef<Path>>(&mut self, path: B, body: &str) -> &mut Self {
        self._file(path.as_ref(), body);
        self
    }

    fn _file(&mut self, path: &Path, body: &str) {
        if path == Path::new("Cargo.toml") {
            self.saw_manifest = true;
        }
        if path == Path::new("src").join("lib.rs") || path == Path::new("src").join("main.rs") {
            self.saw_main_or_lib = true;
        }
        let path = self.root().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(self.root().join(path), body).unwrap();
    }

    pub fn default_cargo_toml(&mut self) -> &mut Self {
        self.file(
            "Cargo.toml",
            &format!(
                r#"
                    [package]
                    name = "{name}"
                    version = "1.0.0"
                "#,
                name = self.project.name,
            ),
        )
    }

    pub fn default_src_lib(&mut self) -> &mut Self {
        self.file(
            Path::new("src").join("lib.rs"),
            r#"
                pub fn pass_fuzzing(data: &[u8]) {
                    let _ = data;
                }

                pub fn fail_fuzzing(data: &[u8]) {
                    if data.len() == 7 {
                        panic!("I'm afraid of number 7");
                    }
                }
            "#,
        )
    }

    pub fn build(&mut self) -> Project {
        if !self.saw_manifest {
            self.default_cargo_toml();
        }
        if !self.saw_main_or_lib {
            self.default_src_lib();
        }
        Project {
            name: self.project.name.clone(),
            root: self.project.root.clone(),
        }
    }
}

impl Project {
    pub fn root(&self) -> PathBuf {
        self.root.clone()
    }

    pub fn build_dir(&self) -> PathBuf {
        self.root().join("target")
    }

    pub fn fuzz_dir(&self) -> PathBuf {
        self.root().join("fuzz")
    }

    pub fn fuzz_cargo_toml(&self) -> PathBuf {
        self.root().join("fuzz").join("Cargo.toml")
    }

    pub fn fuzz_targets_dir(&self) -> PathBuf {
        self.root().join("fuzz").join("fuzz_targets")
    }

    pub fn fuzz_target_path(&self, target: &str) -> PathBuf {
        let mut p = self.fuzz_targets_dir().join(target);
        p.set_extension("rs");
        p
    }

    pub fn cargo_fuzz(&self) -> Command {
        let mut cmd = super::cargo_fuzz();
        cmd.current_dir(&self.root)
            // Even though this disables some parallelism, we won't need to
            // download and compile libbfuzzer-sys multiple times.
            .env("CARGO_HOME", target_tests().join("cargo-home"))
            .env("CARGO_TARGET_DIR", target_tests().join("target"));
        cmd
    }
}