#!/usr/bin/env python3
"""Benchmark pre-generated 10000x20 fixtures on macOS, using a release example.

Usage: python3 scripts/benchmark.py dist/performance [--runs 3]
Build: cargo build --release --target aarch64-apple-darwin --example benchmark
Fixture generation is deliberately excluded from all timings. No network needed.
"""
import argparse
import collections
import hashlib
import json
import pathlib
import platform
import re
import statistics
import subprocess
import xml.etree.ElementTree as ET
import zipfile

NS = {"x": "http://schemas.openxmlformats.org/spreadsheetml/2006/main"}


def cells(path):
    """Independent OOXML reader: compare types, raw values and formulas."""
    with zipfile.ZipFile(path) as archive:
        shared = []
        if "xl/sharedStrings.xml" in archive.namelist():
            shared = ["".join(s.itertext()) for s in ET.fromstring(archive.read("xl/sharedStrings.xml"))]
        sheet = ET.fromstring(archive.read("xl/worksheets/sheet1.xml"))
        result = {}
        for cell in sheet.findall(".//x:sheetData/x:row/x:c", NS):
            kind = cell.get("t", "n")
            value = cell.findtext("x:v", "", NS)
            formula = cell.findtext("x:f", "", NS)
            if kind == "s":
                value = shared[int(value)] if value else ""
            if kind == "inlineStr":
                value = "".join(cell.find("x:is", NS).itertext())
            if kind in {"inlineStr", "str"}:
                kind = "s"
            if value or formula:
                result[cell.get("r")] = (kind, value, formula)
        return result


def expected_changes(old, new):
    changes = {}
    for coordinate in old.keys() | new.keys():
        if old.get(coordinate) != new.get(coordinate):
            changes[coordinate] = "新增" if coordinate not in old else "删除" if coordinate not in new else "修改"
    return changes


def verify_report(run, expected, new_cells, comments):
    log = pathlib.Path(run["log"]).read_text()
    actual = dict((coordinate, kind) for kind, coordinate in re.findall(r"^\[(新增|删除|修改)\] .*? ([A-Z]+\d+) \|", log, re.M))
    assert actual == expected, "Log coordinates/types differ from independent OOXML comparison"
    assert int(run["total"]) == len(expected)
    assert int(run["preview"]) == min(5000, len(expected))
    counts = collections.Counter(expected.values())
    for field, label in [("added", "新增"), ("removed", "删除"), ("modified", "修改")]:
        assert int(run[field]) == counts[label]
    # Numeric serialization may normalize 3.0 -> 3; compare numerically here.
    actual_cells = cells(run["excel"])
    assert actual_cells.keys() == new_cells.keys()
    for coordinate, expected_cell in new_cells.items():
        actual_cell = actual_cells[coordinate]
        assert actual_cell[0] == expected_cell[0] and actual_cell[2] == expected_cell[2], coordinate
        if actual_cell[0] == "n" and actual_cell[1] and expected_cell[1]:
            assert abs(float(actual_cell[1]) - float(expected_cell[1])) < 1e-7, coordinate
        else:
            assert actual_cell[1] == expected_cell[1], coordinate
    with zipfile.ZipFile(run["excel"]) as archive:
        sheet = ET.fromstring(archive.read("xl/worksheets/sheet1.xml"))
        styles = ET.fromstring(archive.read("xl/styles.xml"))
        xfs = styles.find("x:cellXfs", NS)
        fills = styles.find("x:fills", NS)
        styled = {cell.get("r"): int(cell.get("s", "0")) for cell in sheet.findall(".//x:sheetData/x:row/x:c", NS)}
        for coordinate in expected:
            fill = fills[int(xfs[styled[coordinate]].get("fillId", "0"))]
            color = fill.find("x:patternFill/x:fgColor", NS)
            assert color is not None and color.get("rgb") == "FFFFF0BD", coordinate
        notes = []
        for name in archive.namelist():
            if re.search(r"(?:^|/)comments\d*\.xml$", name):
                notes.extend(ET.fromstring(archive.read(name)).findall("x:commentList/x:comment", NS))
        assert {note.get("ref") for note in notes} == (set(expected) if comments else set())


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("fixtures", type=pathlib.Path)
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--binary", type=pathlib.Path, default=pathlib.Path("target/aarch64-apple-darwin/release/examples/benchmark"))
    args = parser.parse_args()
    root = args.fixtures.resolve()
    paths = {name: root / f"{name}-10000x20.xlsx" for name in ["old", "changed", "many"]}
    before = {name: hashlib.sha256(path.read_bytes()).hexdigest() for name, path in paths.items()}
    data = {name: cells(path) for name, path in paths.items()}
    expectations = {name: expected_changes(data["old"], data[name]) for name in paths}
    assert len(data["old"]) == 190001
    assert sum(bool(cell[2]) for cell in data["old"].values()) == 9999
    assert collections.Counter(expectations["changed"].values()) == {"修改": 1000, "新增": 100, "删除": 100}
    assert collections.Counter(expectations["many"].values()) == {"修改": 9999}
    results = []
    for name, comments in [("old", True), ("changed", True), ("many", True), ("many", False)]:
        for repeat in range(args.runs):
            cmd = ["/usr/bin/time", "-l", str(args.binary.resolve()), str(paths["old"]), str(paths[name]), str(root / "reports")]
            if not comments:
                cmd.append("--no-comments")
            completed = subprocess.run(cmd, capture_output=True, text=True, check=True, timeout=300)
            run = dict(line.split("\t", 1) for line in completed.stdout.splitlines() if "\t" in line)
            memory = re.search(r"(\d+)\s+maximum resident set size", completed.stderr)
            assert memory, completed.stderr
            run["peak_rss_mib"] = int(memory[1]) / 1024**2
            wall = re.search(r"([\d.]+)\s+real\s+([\d.]+)\s+user\s+([\d.]+)\s+sys", completed.stderr)
            assert wall, completed.stderr
            run["process_wall_s"] = float(wall[1])
            run.update(case=name, comments=comments, repeat=repeat + 1)
            # Verify each case once, outside its timing; runs stay sequential.
            if repeat == 0:
                verify_report(run, expectations[name], data[name], comments)
            results.append(run)
            print(json.dumps(run, ensure_ascii=False), flush=True)
    assert before == {name: hashlib.sha256(path.read_bytes()).hexdigest() for name, path in paths.items()}
    summary = []
    for name, comments in [("old", True), ("changed", True), ("many", True), ("many", False)]:
        group = [r for r in results if r["case"] == name and r["comments"] == comments]
        summary.append(dict(case=name, comments=comments, differences=len(expectations[name]),
                            peak_rss_mib_max=max(r["peak_rss_mib"] for r in group),
                            **{key: statistics.median(float(r[key]) for r in group) for key in ["old_read_s", "new_read_s", "compare_export_s", "export_s", "total_s", "process_wall_s", "peak_rss_mib"]}))
    report = dict(platform=platform.platform(), machine=platform.machine(), runs=args.runs,
                  fixture_bytes={name:path.stat().st_size for name,path in paths.items()},
                  sha256=before, summary=summary, measurements=results)
    (root / "results.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print("SUMMARY", json.dumps(summary, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
