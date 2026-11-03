#!/usr/bin/env python3
"""Create synthetic paired inputs in a NEW directory; never touches existing files."""
import csv
import random
import sys
import zlib
from pathlib import Path

out = Path(sys.argv[1] if len(sys.argv) > 1 else "demo-data")
out.mkdir(parents=True, exist_ok=False)
rng = random.Random(2026)
# Fixed size, deliberately simplistic inputs: this is a plumbing demo, not a corpus.
text = (b"Quarterly report: revenue and expenditure.\n" * 800)[:32768].ljust(32768, b" ")
randomized = rng.randbytes(len(text))  # Synthetic random bytes, NOT encryption.
compressed = zlib.compress(text)
compressed = (compressed * (len(text) // len(compressed) + 1))[:len(text)]
inputs = {"pre.bin": text, "randomized.bin": randomized,
          "compressed.bin": compressed, "unchanged.bin": text}
for name, data in inputs.items():
    (out / name).write_bytes(data)
with (out / "manifest.csv").open("w", newline="") as f:
    writer = csv.writer(f)
    writer.writerow(["pre", "post", "label"])
    writer.writerow(["pre.bin", "unchanged.bin", "benign"])
    writer.writerow(["pre.bin", "compressed.bin", "benign"])
    writer.writerow(["pre.bin", "randomized.bin", "suspicious"])
print(f"Wrote synthetic data to {out}; randomized bytes are not actual encryption.")
