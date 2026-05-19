use super::*;

#[test]
fn test_bench() {
    let dir = TestDir::new("bench2.in");
    let out = get_stdout(&dir, ["bench"]);
    assert!(out.contains("[username/bench2] bench bench2_test.mbt:23 (#2) ok"));
    assert!(out.contains("[username/bench2] bench bench2.mbt:23 (#0) ok"));

    let out = get_stdout(
        &dir,
        ["bench", "-p", "bench2", "--file", "bench2.mbt", "-i", "0"],
    );
    assert!(!(out.contains("[username/bench2] bench bench2_test.mbt:23 (#2) ok")));
    assert!(out.contains("[username/bench2] bench bench2.mbt:23 (#0) ok"));
}

#[test]
fn test_bench_jsonl() {
    let dir = TestDir::new("bench2.in");
    let out = get_stdout(
        &dir,
        ["bench", "--jsonl", "bench.jsonl", "--target", "wasm-gc"],
    );

    assert_eq!(out, "");
    assert!(!out.contains("[username/bench2] bench"), "stdout: {out}");

    let report_content =
        std::fs::read_to_string(dir.join("bench.jsonl")).expect("bench JSONL file should exist");
    let lines = report_content.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 1, "jsonl: {report_content}");
    let report: serde_json::Value =
        serde_json::from_str(lines[0]).expect("bench --jsonl should write JSON Lines");
    assert_eq!(report["backend"], "wasm-gc");
    assert_eq!(report["total"], 2);
    assert_eq!(report["passed"], 2);
    assert_eq!(report["failed"], 0);

    let results = report["results"]
        .as_array()
        .expect("bench report should contain results");
    assert_eq!(results.len(), 2);
    assert_bench_json_result(results, "bench2.mbt", 0);
    assert_bench_json_result(results, "bench2_test.mbt", 2);
}

#[test]
fn test_bench_jsonl_multi_target() {
    let dir = TestDir::new("bench2.in");
    let out = get_stdout(
        &dir,
        [
            "bench",
            "--jsonl",
            "bench-multi.jsonl",
            "--target",
            "wasm,wasm-gc",
        ],
    );

    assert_eq!(out, "");

    let report_content = std::fs::read_to_string(dir.join("bench-multi.jsonl"))
        .expect("bench JSONL file should exist");
    let reports = report_content
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).expect("valid JSONL report"))
        .collect::<Vec<_>>();
    assert_eq!(reports.len(), 2, "jsonl: {report_content}");
    assert_eq!(reports[0]["backend"], "wasm");
    assert_eq!(reports[1]["backend"], "wasm-gc");
}

fn assert_bench_json_result(results: &[serde_json::Value], filename: &str, index: u64) {
    let result = results
        .iter()
        .find(|result| result["filename"] == filename && result["index"] == index)
        .unwrap_or_else(|| panic!("missing result for {filename} #{index}: {results:#?}"));

    assert_eq!(result["package"], "username/bench2");
    assert_eq!(result["line_number"], 23);
    assert_eq!(result["status"], "ok");
    assert!(result.get("message").is_none(), "result: {result:#?}");

    let summaries = result["summaries"]
        .as_array()
        .expect("bench result should contain summaries");
    assert_eq!(summaries.len(), 1);
    assert!(
        summaries[0]["mean"].as_f64().is_some(),
        "summary: {:#?}",
        summaries[0]
    );
}
