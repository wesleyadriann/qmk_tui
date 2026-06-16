use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QmkAction {
    Compile,
    Flash,
}

impl QmkAction {
    pub fn command_name(self) -> &'static str {
        match self {
            Self::Compile => "compile",
            Self::Flash => "flash",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum KeyboardSource {
    Userspace,
    QmkRepo,
}

impl KeyboardSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Userspace => "Userspace",
            Self::QmkRepo => "QMK repo",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Keyboard {
    pub name: String,
    pub source: KeyboardSource,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QmkRequest {
    pub qmk_home: Option<PathBuf>,
    pub keyboard: String,
    pub keymap: String,
}

impl QmkRequest {
    pub fn new(
        qmk_home: impl Into<Option<PathBuf>>,
        keyboard: impl Into<String>,
        keymap: impl Into<String>,
    ) -> Self {
        Self {
            qmk_home: qmk_home.into(),
            keyboard: keyboard.into(),
            keymap: keymap.into(),
        }
    }

    pub fn validate(&self) -> Result<(), QmkError> {
        if self.keyboard.trim().is_empty() {
            return Err(QmkError::MissingKeyboard);
        }

        if self.keymap.trim().is_empty() {
            return Err(QmkError::MissingKeymap);
        }

        if let Some(qmk_home) = &self.qmk_home
            && !qmk_home.is_dir()
        {
            return Err(QmkError::InvalidQmkHome(qmk_home.clone()));
        }

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum QmkError {
    #[error("keyboard is required")]
    MissingKeyboard,

    #[error("keymap is required")]
    MissingKeymap,

    #[error("QMK home directory does not exist: {0}")]
    InvalidQmkHome(PathBuf),

    #[error("failed to run qmk: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QmkCommandOutput {
    pub status_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl QmkCommandOutput {
    pub fn succeeded(&self) -> bool {
        matches!(self.status_code, Some(0))
    }
}

#[derive(Clone, Debug)]
pub struct QmkRunner {
    executable: OsString,
}

impl Default for QmkRunner {
    fn default() -> Self {
        Self::new("qmk")
    }
}

impl QmkRunner {
    pub fn new(executable: impl Into<OsString>) -> Self {
        Self {
            executable: executable.into(),
        }
    }

    pub fn run(
        &self,
        action: QmkAction,
        request: &QmkRequest,
    ) -> Result<QmkCommandOutput, QmkError> {
        request.validate()?;

        let mut command = Command::new(&self.executable);
        command.args(build_qmk_args(action, request));

        if let Some(qmk_home) = &request.qmk_home {
            command.current_dir(qmk_home);
        }

        let output = command.output()?;

        Ok(QmkCommandOutput {
            status_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        })
    }
}

pub fn build_qmk_args(action: QmkAction, request: &QmkRequest) -> Vec<OsString> {
    vec![
        OsString::from(action.command_name()),
        OsString::from("-kb"),
        OsString::from(request.keyboard.trim()),
        OsString::from("-km"),
        OsString::from(request.keymap.trim()),
    ]
}

pub fn discover_keyboards(qmk_home: &Path) -> Result<Vec<Keyboard>, QmkError> {
    if !qmk_home.is_dir() {
        return Err(QmkError::InvalidQmkHome(qmk_home.to_path_buf()));
    }

    let mut userspace_keyboards = Vec::new();
    for root in userspace_keyboard_roots(qmk_home) {
        collect_keyboard_dirs(&root, KeyboardSource::Userspace, &mut userspace_keyboards)?;
    }
    sort_and_dedup(&mut userspace_keyboards);

    let mut repo_keyboards = Vec::new();
    collect_keyboard_dirs(
        &qmk_home.join("keyboards"),
        KeyboardSource::QmkRepo,
        &mut repo_keyboards,
    )?;
    sort_and_dedup(&mut repo_keyboards);

    userspace_keyboards.extend(repo_keyboards);
    Ok(userspace_keyboards)
}

pub fn parse_qmk_home(input: &str) -> Option<PathBuf> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    Some(expand_tilde(trimmed))
}

pub fn default_qmk_home() -> Option<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join("qmk_firmware"))
        .filter(|path| path.is_dir())
}

fn userspace_keyboard_roots(qmk_home: &Path) -> Vec<PathBuf> {
    let mut roots = vec![
        qmk_home.join("userspace").join("keyboards"),
        qmk_home.join("user_space").join("keyboards"),
    ];

    let users_dir = qmk_home.join("users");
    if let Ok(entries) = fs::read_dir(users_dir) {
        for entry in entries.flatten() {
            if entry
                .file_type()
                .map(|file_type| file_type.is_dir())
                .unwrap_or(false)
            {
                roots.push(entry.path().join("keyboards"));
            }
        }
    }

    roots
}

fn collect_keyboard_dirs(
    root: &Path,
    source: KeyboardSource,
    keyboards: &mut Vec<Keyboard>,
) -> Result<(), QmkError> {
    if !root.is_dir() {
        return Ok(());
    }

    let mut stack = vec![root.to_path_buf()];

    while let Some(path) = stack.pop() {
        if is_keyboard_dir(&path)
            && let Some(name) = keyboard_name(root, &path)
        {
            keyboards.push(Keyboard { name, source });
        }

        for child in child_dirs(&path)? {
            stack.push(child);
        }
    }

    Ok(())
}

fn child_dirs(path: &Path) -> Result<Vec<PathBuf>, QmkError> {
    let mut dirs = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if file_type.is_dir() && !is_ignored_dir(&entry.file_name().to_string_lossy()) {
            dirs.push(entry.path());
        }
    }

    dirs.sort();
    dirs.reverse();
    Ok(dirs)
}

fn is_keyboard_dir(path: &Path) -> bool {
    path.join("keyboard.json").is_file() || path.join("info.json").is_file()
}

fn keyboard_name(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .filter(|name| !name.is_empty())
}

fn is_ignored_dir(name: &str) -> bool {
    matches!(
        name,
        ".git" | ".build" | "build" | "keymaps" | "node_modules" | "target"
    )
}

fn sort_and_dedup(keyboards: &mut Vec<Keyboard>) {
    keyboards.sort_by(|left, right| left.name.cmp(&right.name));
    keyboards.dedup_by(|left, right| left.name == right.name && left.source == right.source);
}

fn expand_tilde(path: &str) -> PathBuf {
    if path == "~" {
        return env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(path));
    }

    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = env::var_os("HOME")
    {
        return Path::new(&home).join(rest);
    }

    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn builds_compile_args() {
        let request = QmkRequest::new(None, "splitkb/kyria/rev3", "default");

        assert_eq!(
            build_qmk_args(QmkAction::Compile, &request),
            vec!["compile", "-kb", "splitkb/kyria/rev3", "-km", "default"]
        );
    }

    #[test]
    fn builds_flash_args() {
        let request = QmkRequest::new(None, "planck/rev6", "custom");

        assert_eq!(
            build_qmk_args(QmkAction::Flash, &request),
            vec!["flash", "-kb", "planck/rev6", "-km", "custom"]
        );
    }

    #[test]
    fn rejects_missing_keyboard() {
        let request = QmkRequest::new(None, "", "default");

        assert!(matches!(request.validate(), Err(QmkError::MissingKeyboard)));
    }

    #[test]
    fn discovers_userspace_keyboards_before_repo_keyboards() {
        let qmk_home = test_qmk_home();
        fs::create_dir_all(qmk_home.join("users/wes/keyboards/acme/mini")).unwrap();
        fs::create_dir_all(qmk_home.join("keyboards/planck/rev6")).unwrap();
        fs::write(
            qmk_home.join("users/wes/keyboards/acme/mini/keyboard.json"),
            "{}",
        )
        .unwrap();
        fs::write(qmk_home.join("keyboards/planck/rev6/keyboard.json"), "{}").unwrap();

        let keyboards = discover_keyboards(&qmk_home).unwrap();

        assert_eq!(
            keyboards,
            vec![
                Keyboard {
                    name: String::from("acme/mini"),
                    source: KeyboardSource::Userspace,
                },
                Keyboard {
                    name: String::from("planck/rev6"),
                    source: KeyboardSource::QmkRepo,
                },
            ]
        );

        fs::remove_dir_all(qmk_home).unwrap();
    }

    fn test_qmk_home() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        env::temp_dir().join(format!("qmk-tui-test-{nanos}"))
    }
}
