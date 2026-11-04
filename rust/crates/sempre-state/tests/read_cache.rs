use sempre_state::{Layout, Store};

#[test]
fn reads_see_writes_from_another_store_instance() {
    let root = tempfile::tempdir().expect("root");
    let reader = Store::new(Layout::at(root.path()));
    reader.initialize().expect("initialize");
    assert!(!reader.read().expect("read").pending);

    // Another process, such as the CLI, has its own Store.
    let writer = Store::new(Layout::at(root.path()));
    writer
        .update(|document| {
            document.pending = true;
            Ok(())
        })
        .expect("update");

    assert!(reader.read().expect("read").pending);
}

#[test]
fn own_writes_are_visible_immediately() {
    let root = tempfile::tempdir().expect("root");
    let store = Store::new(Layout::at(root.path()));
    store.initialize().expect("initialize");
    for expected in [true, false, true] {
        store
            .update(|document| {
                document.pending = expected;
                Ok(())
            })
            .expect("update");
        assert_eq!(store.read().expect("read").pending, expected);
    }
}
