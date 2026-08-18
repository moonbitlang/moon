use super::*;

use std::path::Path;

const N2_BUILD_RECORD_MARK: u16 = 0x8000;
const N2_COMPACTION_MIN_SIZE: u64 = 2 * 1024 * 1024;

// n2's database model is intentionally private. These helpers append valid
// version-1 records so an end-to-end test can cross the compaction threshold
// without invoking Moon tens of thousands of times.
struct EncodedBuildRecord {
    outputs: Vec<u32>,
}

fn n2_build_records(database: &[u8]) -> (u32, Vec<EncodedBuildRecord>) {
    assert!(database.len() >= 8, "n2 database should contain a header");
    assert_eq!(&database[..8], b"n2db\x01\0\0\0");

    let mut records = Vec::new();
    let mut path_count = 0;
    let mut offset = 8;
    while offset < database.len() {
        let header = u16::from_le_bytes(database[offset..offset + 2].try_into().unwrap());
        offset += 2;
        if header & N2_BUILD_RECORD_MARK == 0 {
            offset += usize::from(header);
            assert!(offset <= database.len(), "truncated n2 path record");
            path_count += 1;
            continue;
        }

        let output_count = usize::from(header & !N2_BUILD_RECORD_MARK);
        let mut outputs = Vec::with_capacity(output_count);
        for _ in 0..output_count {
            outputs.push(u32::from_le_bytes([
                database[offset],
                database[offset + 1],
                database[offset + 2],
                0,
            ]));
            offset += 3;
        }
        let dependency_count = usize::from(u16::from_le_bytes(
            database[offset..offset + 2].try_into().unwrap(),
        ));
        offset += 2 + dependency_count * 3 + 8;
        assert!(offset <= database.len(), "truncated n2 build record");
        records.push(EncodedBuildRecord { outputs });
    }
    (path_count, records)
}

fn append_record_until_compaction_threshold(path: &Path, prefix: &[u8], record: &[u8]) -> u64 {
    let mut size = std::fs::metadata(path).unwrap().len();
    let prefix_len = u64::try_from(prefix.len()).unwrap();
    let record_len = u64::try_from(record.len()).unwrap();
    let mut database =
        std::io::BufWriter::new(std::fs::OpenOptions::new().append(true).open(path).unwrap());
    database.write_all(prefix).unwrap();
    size += prefix_len;
    while size < N2_COMPACTION_MIN_SIZE {
        database.write_all(record).unwrap();
        size += record_len;
    }
    database.flush().unwrap();
    size
}

fn append_superseded_records(path: &Path) -> u64 {
    let database = std::fs::read(path).unwrap();
    let (path_count, _) = n2_build_records(&database);
    assert!(
        path_count <= 0x00ff_ffff,
        "n2 path ID should fit in 24 bits"
    );
    let encoded_id = path_count.to_le_bytes();

    let name = b"__moon_compaction_probe__";
    let mut path_record = Vec::with_capacity(2 + name.len());
    path_record.extend_from_slice(&(name.len() as u16).to_le_bytes());
    path_record.extend_from_slice(name);

    // A few large records keep this e2e test fast while remaining valid n2
    // records. Their dummy output has no producer in Moon's current graph, so
    // neither replay nor the retained final record affects dirty checking.
    let dependency_count = u16::MAX;
    let mut record = Vec::with_capacity(15 + usize::from(dependency_count) * 3);
    record.extend_from_slice(&(N2_BUILD_RECORD_MARK | 1).to_le_bytes());
    record.extend_from_slice(&encoded_id[..3]);
    record.extend_from_slice(&dependency_count.to_le_bytes());
    for _ in 0..dependency_count {
        record.extend_from_slice(&encoded_id[..3]);
    }
    record.extend_from_slice(&0u64.to_le_bytes());
    append_record_until_compaction_threshold(path, &path_record, &record)
}

fn append_incompatible_output_records(path: &Path) -> u64 {
    let database = std::fs::read(path).unwrap();
    let (_, records) = n2_build_records(&database);
    let multi_output = records
        .iter()
        .rev()
        .find(|record| record.outputs.len() >= 2)
        .expect("build should write a multi-output n2 record");
    let unrelated_output = records
        .iter()
        .rev()
        .flat_map(|record| record.outputs.iter())
        .copied()
        .find(|output| !multi_output.outputs.contains(output))
        .expect("build should write another n2 record");

    // Simulate output ownership changing from {a, b} to {a, c}. The current
    // graph still maps a/b and c to different execution actions, so replay
    // ignores this incompatible record before compaction.
    let dependency_count = u16::MAX;
    let mut record = Vec::with_capacity(18 + usize::from(dependency_count) * 3);
    record.extend_from_slice(&(N2_BUILD_RECORD_MARK | 2).to_le_bytes());
    record.extend_from_slice(&multi_output.outputs[0].to_le_bytes()[..3]);
    record.extend_from_slice(&unrelated_output.to_le_bytes()[..3]);
    record.extend_from_slice(&dependency_count.to_le_bytes());
    for _ in 0..dependency_count {
        record.extend_from_slice(&multi_output.outputs[0].to_le_bytes()[..3]);
    }
    record.extend_from_slice(&0u64.to_le_bytes());
    append_record_until_compaction_threshold(path, &[], &record)
}

#[test]
fn preserves_partial_target_history() {
    let dir = TestDir::new("targets/shared_n2_db");

    moon_cmd(&dir)
        .args(["check", "--target", "all"])
        .assert()
        .success();

    let database = dir.join("_build/.moon_db");
    let bloated_size = append_superseded_records(&database);

    moon_cmd(&dir)
        .args(["check", "--target", "js"])
        .assert()
        .success()
        .stdout_eq(snapbox::str![""])
        .stderr_eq(snapbox::str![[r#"
Finished. moon: no work to do

"#]]);

    let compacted_size = std::fs::metadata(&database).unwrap().len();
    assert!(
        compacted_size < bloated_size / 3,
        "n2 database should be compacted: {bloated_size} -> {compacted_size}"
    );

    moon_cmd(&dir)
        .args(["check", "--target", "all"])
        .assert()
        .success()
        .stdout_eq(snapbox::str![""])
        .stderr_eq(snapbox::str![[r#"
Finished. moon: no work to do

"#]]);
}

#[test]
fn accepts_conservative_rebuild_after_output_set_change() {
    let dir = TestDir::new("targets/shared_n2_db");

    moon_cmd(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success();

    let database = dir.join("_build/.moon_db");
    let bloated_size = append_incompatible_output_records(&database);

    // This invocation replays the complete log before compacting it, so the
    // compatible older records still make the graph up to date.
    moon_cmd(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success()
        .stdout_eq(snapbox::str![""])
        .stderr_eq(snapbox::str![[r#"
Finished. moon: no work to do

"#]]);
    let compacted_size = std::fs::metadata(&database).unwrap().len();
    assert!(
        compacted_size < bloated_size / 3,
        "n2 database should be compacted: {bloated_size} -> {compacted_size}"
    );

    // Known and accepted n2 trade-off: compaction treats any shared output as
    // superseding the complete older record. If multi-output ownership changes
    // and later changes back, Moon may conservatively rebuild once. This loses
    // only a cache hit; it cannot reuse stale work or produce an incorrect build.
    // Reconsider this decision if normal Moon target selection starts changing
    // overlapping output sets across invocations.
    moon_cmd(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success()
        .stdout_eq(snapbox::str![""])
        .stderr_eq(snapbox::str![[r#"
Finished. moon: ran 2 tasks, now up to date

"#]]);

    moon_cmd(&dir)
        .args(["build", "--target", "wasm"])
        .assert()
        .success()
        .stdout_eq(snapbox::str![""])
        .stderr_eq(snapbox::str![[r#"
Finished. moon: no work to do

"#]]);
}
