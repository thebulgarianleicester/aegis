import csv
import gzip
import importlib.util
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location("make_corpus", Path(__file__).resolve().parents[1] / "scripts/make_corpus.py")
corpus = importlib.util.module_from_spec(spec)
spec.loader.exec_module(corpus)


class CorpusTest(unittest.TestCase):
    def test_generation_and_intermediate_labels(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "new"
            info = corpus.generate(root, 1, 5)
            self.assertEqual(info["pairs"], 7)
            with (root / "manifest.csv").open(newline="") as f:
                rows = {r["workload"]: r for r in csv.DictReader(f)}
            row = rows["aes_intermittent_text"]
            pre = (root / row["pre"]).read_bytes()
            post = (root / row["post"]).read_bytes()
            for index in range(16):
                segment = slice(index * 4096, (index + 1) * 4096)
                self.assertEqual(pre[segment] != post[segment], index in {0, 4, 8, 12})
            self.assertEqual(row["positive_blocks"], "0;4;8;12")
            archive = rows["benign_gzip_replacement"]
            self.assertEqual(len(gzip.decompress((root / archive["post"]).read_bytes())), 65536)
            with self.assertRaises(FileExistsError):
                corpus.generate(root, 1, 5)

    def test_transforms_are_stream_ciphers_and_deterministic(self):
        data = b"sample data " * 400
        for algorithm in ("aes-256-ctr", "chacha20"):
            ct = corpus.transform(data, algorithm, 99)
            self.assertNotEqual(ct, data)
            self.assertEqual(corpus.transform(ct, algorithm, 99), data)
            self.assertEqual(corpus.transform(data, algorithm, 99), ct)


if __name__ == "__main__":
    unittest.main()
