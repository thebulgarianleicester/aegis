#!/usr/bin/env python3
"""Print per-workload block-level counts and rates from an Aegis benchmark report."""
import argparse
import json
from pathlib import Path


def ratio(n, d):
    return f"{100*n/d:6.1f}%" if d else "     —"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path)
    args = parser.parse_args()
    report = json.loads(args.report.read_text())
    if "workloads" not in report:
        parser.error("report has no workload summaries; rerun current aegis-bench")
    print("BLOCK-LEVEL ONLY — synthetic dataset; no confidence or independent workload-level estimates")
    print(f"{'workload':27} {'method':25} {'TP':>5} {'FP':>5} {'TN':>5} {'FN':>5} {'TPR':>7} {'FPR':>7}")
    for workload, methods in report["workloads"].items():
        for method, c in methods.items():
            tp, fp, tn, fn = (c[x] for x in ("true_positive", "false_positive", "true_negative", "false_negative"))
            print(f"{workload:27} {method:25} {tp:5} {fp:5} {tn:5} {fn:5} {ratio(tp,tp+fn):>7} {ratio(fp,fp+tn):>7}")


if __name__ == "__main__":
    main()
