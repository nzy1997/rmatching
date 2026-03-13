import unittest

from benchmarks.run_minimal_benchmark import summarize_case


class RunMinimalBenchmarkTest(unittest.TestCase):
    def test_summarize_case_reports_match_rate_and_mismatches(self):
        row, mismatches = summarize_case(
            case_name="square-4",
            num_detectors=4,
            num_edges=9,
            py_predictions=[[1], [0], [1]],
            rm_predictions=[[1], [1], [1]],
            rm_stats={
                "build_us": 10.0,
                "mean_decode_us": 2.0,
                "median_decode_us": 2.0,
                "p95_decode_us": 3.0,
            },
            syndromes=[[1, 0, 0, 1], [1, 1, 0, 0], [0, 0, 0, 0]],
        )
        self.assertEqual(row["prediction_match_rate"], 2 / 3)
        self.assertEqual(row["mismatch_cases"], 1)
        self.assertEqual(mismatches[0]["syndrome"], [1, 1, 0, 0])


if __name__ == "__main__":
    unittest.main()
