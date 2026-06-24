use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use qmk_tui::qmk::{
    Keyboard, KeyboardSource, QmkAction, QmkError, QmkRequest, build_qmk_args,
    discover_keyboards,
};

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
