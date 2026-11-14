#!/usr/bin/env python3
"""Compare the committed MIMIC fixture with PhysioNet's MIMIC-IV demo in the OMOP CDM (v0.9), cell by cell.

The fixture `tests/oracle/generated/eunomia/mimic/cdm.sqlite` came from the R package Eunomia. PhysioNet publishes
the same demo as CSV files. This script reads both and classifies every cell as identical, null, or one of the
known representation changes. Anything else is reported as UNEXPLAINED, with samples.

Get the CSV files (open access, ODbL v1.0; see docs/test-data.md and THIRD_PARTY_NOTICES.md):

    mkdir physionet-csv && cd physionet-csv
    for f in $(curl -sL https://physionet.org/files/mimic-iv-demo-omop/0.9/1_omop_data_csv/ \\
               | grep -o 'href="[^"]*\\.csv"' | cut -d'"' -f2); do
      curl -sLO "https://physionet.org/files/mimic-iv-demo-omop/0.9/1_omop_data_csv/$f"
    done

Run (standard library only; `-I` keeps the interpreter from importing anything from the data directory):

    python3 -I scripts/compare-mimic-with-physionet.py physionet-csv tests/oracle/generated/eunomia/mimic/cdm.sqlite
"""
import collections
import csv
import datetime
import math
import os
import re
import sqlite3
import sys

DATE = re.compile(r"^\d{4}-\d{2}-\d{2}$")
DATETIME = re.compile(r"^\d{4}-\d{2}-\d{2}[ T]\d{2}:\d{2}:\d{2}(\.\d+)?$")
INT = re.compile(r"^-?\d+$")
NUM = re.compile(r"^-?\d+(\.\d+)?([eE][-+]?\d+)?$")
EPOCH = datetime.datetime(1970, 1, 1)

# What each class means. Only NULL-MISMATCH and UNEXPLAINED mean a value differs; the rest change how it is stored.
MEANING = {
    "exact": "identical (a decimal or integer equal to the CSV's value counts as identical)",
    "null": "empty in both",
    "int-rounded-through-double": "64-bit id stored as the nearest double",
    "int-within-2-ulp-of-double": "64-bit id stored as a double within 2 units in the last place of the nearest one",
    "date-or-datetime-stored-as-epoch-seconds": "date or datetime stored as seconds since 1970",
    "numeric-equal": "whole number in the CSV, stored as REAL (300 as 300.0)",
    "numeric-text-reformatted": "number kept as text, with '.0' added",
    "whitespace-trimmed": "text with trailing whitespace trimmed",
    "float-rounded": "decimal differing only in binary-float noise (13.991999999999999 in the CSV, 13.992 in SQLite)",
    "NULL-MISMATCH": "value in the CSV, NULL in SQLite",
    "UNEXPLAINED": "none of the above",
}


def to_epoch(text):
    text = text.replace("T", " ")
    fmt = "%Y-%m-%d" if DATE.match(text) else ("%Y-%m-%d %H:%M:%S.%f" if "." in text else "%Y-%m-%d %H:%M:%S")
    return (datetime.datetime.strptime(text, fmt) - EPOCH).total_seconds()


def classify(s, v):
    """s is the CSV text ('' means null) and v the SQLite value."""
    if s == "" and v is None:
        return "null"
    if s == "" or v is None:
        return "NULL-MISMATCH"
    if isinstance(v, str):
        if v == s:
            return "exact"
        if NUM.match(s) and NUM.match(v) and float(v) == float(s):
            return "numeric-text-reformatted"
        if v == s.strip():
            return "whitespace-trimmed"
        return "UNEXPLAINED"
    if INT.match(s):
        if isinstance(v, int) and int(s) == v:
            return "exact"
        if isinstance(v, int) and int(float(s)) == v:
            return "int-rounded-through-double"
        if float(s) == float(v):
            return "numeric-equal"
        if isinstance(v, int) and abs(int(s) - v) <= 2 * math.ulp(float(s)):
            return "int-within-2-ulp-of-double"
        return "UNEXPLAINED"
    if NUM.match(s):
        if float(s) == float(v):
            return "exact"
        close = abs(float(s) - float(v)) <= 1e-9 * max(1.0, abs(float(s)))
        return "float-rounded" if close else "UNEXPLAINED"
    if DATE.match(s) or DATETIME.match(s):
        try:
            return "date-or-datetime-stored-as-epoch-seconds" if float(v) == to_epoch(s) else "UNEXPLAINED"
        except ValueError:
            return "UNEXPLAINED"
    return "UNEXPLAINED"


def main(csv_dir, db_path):
    csv.field_size_limit(10**9)
    db = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
    tables = {r[0] for r in db.execute("select name from sqlite_master where type='table'")}
    total = collections.Counter()
    per_column = collections.defaultdict(collections.Counter)
    samples = collections.defaultdict(list)
    compared, problems = 0, 0
    for fname in sorted(f for f in os.listdir(csv_dir) if f.endswith(".csv")):
        table = fname[:-4].removeprefix("2b_")  # PhysioNet's vocabulary files are 2b_concept.csv and so on
        if table not in tables:
            print(f"{fname}: no table '{table}' in the SQLite file")
            problems += 1
            continue
        with open(os.path.join(csv_dir, fname), newline="", encoding="utf-8") as handle:
            reader = csv.reader(handle)
            header = next(reader)
            rows = list(reader)
        cursor = db.execute(f"select * from {table}")
        columns = [d[0].lower() for d in cursor.description]
        stored = cursor.fetchall()
        if sorted(h.lower() for h in header) != sorted(columns) or len(rows) != len(stored):
            print(f"{fname}: columns or row count differ (CSV {len(rows)} rows, SQLite {len(stored)})")
            problems += 1
            continue
        order = [columns.index(h.lower()) for h in header]  # the CSV and the SQLite file order columns differently
        compared += 1
        for i, (csv_row, db_row) in enumerate(zip(rows, stored)):
            for position, text in enumerate(csv_row):
                value = db_row[order[position]]
                kind = classify(text, value)
                total[kind] += 1
                if kind not in ("exact", "null"):
                    per_column[f"{table}.{header[position]}"][kind] += 1
                    if kind in ("UNEXPLAINED", "NULL-MISMATCH", "float-rounded") and len(samples[kind]) < 12:
                        samples[kind].append((table, header[position], i, text[:60], repr(value)[:60]))
    print(f"\n{compared} CSV files compared with the SQLite tables, same row count in each; {sum(total.values()):,} cells.\n")
    for kind, count in sorted(total.items(), key=lambda item: -item[1]):
        print(f"  {kind:42s}{count:>11,}   {MEANING[kind]}")
    for kind in ("float-rounded", "NULL-MISMATCH", "UNEXPLAINED"):
        for table, column, row, text, value in samples[kind]:
            print(f"    {kind}: {table}.{column} row {row}: CSV {text!r}, SQLite {value}")
    changed = [c for c, k in per_column.items() if set(k) & {"NULL-MISMATCH", "UNEXPLAINED"}]
    print("\ncolumns where a value, not only its representation, differs:", ", ".join(sorted(changed)) or "none")
    return 1 if problems or total["UNEXPLAINED"] else 0


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    sys.exit(main(sys.argv[1], sys.argv[2]))
