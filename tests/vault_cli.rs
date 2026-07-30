//! CLI smoke for vault membership (`tes vault` / `tes link`) — THI-217.

use std::path::Path;
use std::process::Command;

use tempfile::tempdir;
use tessera_doc::catalog::{DocumentCatalog, LinkEntry, LinkKind, TesWriterSession, TextHeader};
use tessera_doc::layout::DocKind;
use uuid::Uuid;

fn tes() -> Command {
    Command::new(env!("CARGO_BIN_EXE_tes"))
}

fn write_note(dir: &Path, name: &str, title: &str, id: Uuid, link_to: Option<Uuid>) {
    let path = dir.join(name);
    let mut session = TesWriterSession::create(&path, DocKind::Note);
    session
        .set_catalog(DocumentCatalog::new(
            id.to_string(),
            title,
            "2026-07-28T00:00:00Z",
            "2026-07-28T00:00:00Z",
            DocKind::Note,
        ))
        .unwrap();
    session
        .add_text_chunk(&TextHeader::paragraph(), title)
        .unwrap();
    if let Some(target) = link_to {
        session
            .add_link(LinkEntry::new(
                1,
                0,
                title.len() as u32,
                target,
                1,
                LinkKind::Wiki,
            ))
            .unwrap();
    }
    session.commit().unwrap();
}

#[test]
fn vault_add_list_members_link_check_and_remove() {
    let vault = tempdir().unwrap();
    let outside = tempdir().unwrap();
    let in_id = Uuid::new_v4();
    let out_id = Uuid::new_v4();
    write_note(vault.path(), "in.tes", "Inside", in_id, Some(out_id));
    write_note(outside.path(), "out.tes", "Outside", out_id, None);
    let external = outside.path().join("out.tes");

    // Before registration the out-of-tree target is missing.
    let before = tes()
        .args(["link", "--vault"])
        .arg(vault.path())
        .arg("check")
        .output()
        .unwrap();
    assert_eq!(before.status.code(), Some(1));
    let before_err = String::from_utf8_lossy(&before.stdout);
    assert!(before_err.contains("status=failed"), "{before_err}");

    let add = tes()
        .args(["vault", "--vault"])
        .arg(vault.path())
        .arg("add")
        .arg(&external)
        .output()
        .unwrap();
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    let add_out = String::from_utf8_lossy(&add.stdout);
    assert!(add_out.contains("added\tfile\t"), "{add_out}");

    let members = tes()
        .args(["vault", "--vault"])
        .arg(vault.path())
        .arg("members")
        .output()
        .unwrap();
    assert!(members.status.success());
    let members_out = String::from_utf8_lossy(&members.stdout);
    assert!(members_out.contains("file\t"), "{members_out}");
    assert!(members_out.contains("members=1"), "{members_out}");

    let list = tes()
        .args(["vault", "--vault"])
        .arg(vault.path())
        .arg("list")
        .output()
        .unwrap();
    assert!(list.status.success());
    let list_out = String::from_utf8_lossy(&list.stdout);
    assert!(list_out.contains("Outside"), "{list_out}");
    assert!(list_out.contains("documents=2"), "{list_out}");

    let check = tes()
        .args(["link", "--vault"])
        .arg(vault.path())
        .arg("check")
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
    let check_out = String::from_utf8_lossy(&check.stdout);
    assert!(check_out.contains("status=ok"), "{check_out}");
    assert!(check_out.contains("documents=2"), "{check_out}");

    let remove = tes()
        .args(["vault", "--vault"])
        .arg(vault.path())
        .arg("remove")
        .arg(&external)
        .output()
        .unwrap();
    assert!(remove.status.success());

    let after_list = tes()
        .args(["vault", "--vault"])
        .arg(vault.path())
        .arg("list")
        .output()
        .unwrap();
    assert!(after_list.status.success());
    let after_out = String::from_utf8_lossy(&after_list.stdout);
    assert!(!after_out.contains("Outside"), "{after_out}");
    assert!(after_out.contains("documents=1"), "{after_out}");
}

#[test]
fn vault_add_extra_root_lists_nested() {
    let vault = tempdir().unwrap();
    let extra = tempdir().unwrap();
    write_note(vault.path(), "in.tes", "Inside", Uuid::new_v4(), None);
    write_note(extra.path(), "nested.tes", "Nested", Uuid::new_v4(), None);
    std::fs::create_dir(extra.path().join("sub")).unwrap();
    write_note(
        &extra.path().join("sub"),
        "deep.tes",
        "Deep",
        Uuid::new_v4(),
        None,
    );

    let add = tes()
        .args(["vault", "--vault"])
        .arg(vault.path())
        .arg("add")
        .arg(extra.path())
        .output()
        .unwrap();
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    assert!(
        String::from_utf8_lossy(&add.stdout).contains("added\troot\t"),
        "{}",
        String::from_utf8_lossy(&add.stdout)
    );

    let list = tes()
        .args(["vault", "--vault"])
        .arg(vault.path())
        .arg("list")
        .output()
        .unwrap();
    assert!(list.status.success());
    let out = String::from_utf8_lossy(&list.stdout);
    assert!(out.contains("Nested"), "{out}");
    assert!(out.contains("Deep"), "{out}");
    assert!(out.contains("documents=3"), "{out}");
}

#[test]
fn vault_search_scan_and_index() {
    let vault = tempdir().unwrap();
    write_note(
        vault.path(),
        "a.tes",
        "Alpha note with xylophone marker",
        Uuid::new_v4(),
        None,
    );
    write_note(vault.path(), "b.tes", "Beta ordinary", Uuid::new_v4(), None);

    let scan = tes()
        .args(["vault", "--vault"])
        .arg(vault.path())
        .arg("search")
        .arg("xylophone")
        .output()
        .unwrap();
    assert!(
        scan.status.success(),
        "{}",
        String::from_utf8_lossy(&scan.stderr)
    );
    let scan_err = String::from_utf8_lossy(&scan.stderr);
    assert!(scan_err.contains("source=scan"), "{scan_err}");
    let scan_out = String::from_utf8_lossy(&scan.stdout);
    assert!(scan_out.contains("Alpha"), "{scan_out}");
    assert!(scan_out.contains("hits=1"), "{scan_out}");
    assert!(!vault.path().join(".tessera").exists());

    let indexed = tes()
        .args(["vault", "--vault"])
        .arg(vault.path())
        .arg("search")
        .arg("xylophone")
        .arg("--index")
        .output()
        .unwrap();
    assert!(
        indexed.status.success(),
        "{}",
        String::from_utf8_lossy(&indexed.stderr)
    );
    let idx_err = String::from_utf8_lossy(&indexed.stderr);
    assert!(
        idx_err.contains(".tessera") || idx_err.contains("rebuilt"),
        "{idx_err}"
    );
    let idx_out = String::from_utf8_lossy(&indexed.stdout);
    assert!(idx_out.contains("Alpha"), "{idx_out}");
    assert!(vault.path().join(".tessera/fts").is_dir());
}
