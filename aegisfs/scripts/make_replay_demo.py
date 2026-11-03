#!/usr/bin/env python3
"""Create offline replay instructions from the synthetic corpus; never modifies its inputs."""
import argparse
import csv
import json
import os
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("corpus", type=Path, help="previously generated synthetic corpus directory")
    parser.add_argument("output", type=Path, help="new directory that must not exist")
    args=parser.parse_args()
    corpus=args.corpus.resolve(strict=True)
    with (corpus / "manifest.csv").open(newline="") as f:
        pairs={}
        for row in csv.DictReader(f):
            if row["workload"] in ("benign_gzip_replacement", "aes_intermittent_text", "aes_full_text"):
                pairs.setdefault(row["workload"],row)
    if len(pairs)!=3:
        parser.error("corpus is missing expected synthetic workloads")
    args.output.mkdir(exist_ok=False)
    out=args.output.resolve()
    # Deliberately illustrative values, NOT estimated from training data.
    likelihood={"benign":0.4,"suspicious":0.8}
    config={"uniform":likelihood,"header_change":{"benign":0.01,"suspicious":0.5},
            "unlink_or_rename":likelihood,"directory_jump":likelihood,"canary":likelihood,
            "decay_per_second":0.5,"threshold":6.0,"max_groups":8}
    with (out / "config.json").open("x") as f:
        json.dump(config,f,indent=2);f.write("\n")
    names=["benign_gzip_replacement","aes_intermittent_text","aes_full_text"]
    with (out / "events.jsonl").open("x") as f:
        # Interleave groups. These are synthetic ordered file-snapshot chunks,
        # NOT observed writes; unmodified intermittent blocks are included.
        for i in range(16):
            for group in names:
                pair=pairs[group]
                event={"group":group,"timestamp_ns":i*100_000_000,
                    "pre":os.path.relpath(corpus/pair["pre"],out),
                    "post":os.path.relpath(corpus/pair["post"],out),
                    "offset":i*4096,"length":4096}
                f.write(json.dumps(event)+"\n")
    print(f"Wrote synthetic replay inputs to {out}")


if __name__=="__main__":
    main()
