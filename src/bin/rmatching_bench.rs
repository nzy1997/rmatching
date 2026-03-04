#[cfg(feature = "bench")]
mod bench {
    use rstim::sim::bit_table::BitTable;
    use std::path::Path;

    pub fn parse_stim_filename(path: &Path) -> Option<(f64, usize)> {
        let stem = path.file_stem()?.to_str()?;
        // Match e.g. "...p_0.001_d_5" or "...p_0.01_d_17"
        let p_start = stem.find("_p_")? + 3;
        let rest = &stem[p_start..];
        let d_pos = rest.find("_d_")?;
        let p_str = &rest[..d_pos];
        let after_d = &rest[d_pos + 3..];
        // d is the next token (until _ or end)
        let d_end = after_d.find('_').unwrap_or(after_d.len());
        let d_str = &after_d[..d_end];
        let p: f64 = p_str.parse().ok()?;
        let d: usize = d_str.parse().ok()?;
        Some((p, d))
    }

    pub fn detections_to_syndromes(table: &BitTable, num_detectors: usize) -> Vec<Vec<u8>> {
        let n_shots = table.num_major();
        (0..n_shots)
            .map(|shot| {
                (0..num_detectors)
                    .map(|det| if table.get(shot, det) { 1u8 } else { 0u8 })
                    .collect()
            })
            .collect()
    }

    pub fn count_logical_errors(
        predictions: &[Vec<u8>],
        obs_flips: &BitTable,
    ) -> usize {
        let num_obs = obs_flips.num_minor();
        predictions.iter().enumerate().filter(|(shot, pred)| {
            (0..num_obs.min(pred.len())).any(|obs| {
                let actual = obs_flips.get(*shot, obs);
                let predicted = pred[obs] != 0;
                actual != predicted
            })
        }).count()
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::path::Path;

        #[test]
        fn test_parse_stim_filename_basic() {
            let p = Path::new("surface_code_rotated_memory_x_p_0.001_d_5.stim");
            assert_eq!(parse_stim_filename(p), Some((0.001, 5)));
        }

        #[test]
        fn test_parse_stim_filename_larger() {
            let p = Path::new("surface_code_rotated_memory_x_p_0.01_d_17.stim");
            assert_eq!(parse_stim_filename(p), Some((0.01, 17)));
        }

        #[test]
        fn test_parse_stim_filename_no_match() {
            let p = Path::new("not_a_surface_code.stim");
            assert_eq!(parse_stim_filename(p), None);
        }

        #[test]
        fn test_detections_to_syndromes_basic() {
            // 2 shots, 3 detectors
            let mut table = BitTable::new(2, 3);
            table.set(0, 0, true);  // shot 0, det 0
            table.set(1, 2, true);  // shot 1, det 2
            let syndromes = detections_to_syndromes(&table, 3);
            assert_eq!(syndromes.len(), 2);
            assert_eq!(syndromes[0], vec![1, 0, 0]);
            assert_eq!(syndromes[1], vec![0, 0, 1]);
        }

        #[test]
        fn test_count_logical_errors_none() {
            // 2 shots, 1 observable, all predicted correctly
            let mut obs = BitTable::new(2, 1);
            obs.set(0, 0, false);
            obs.set(1, 0, false);
            let preds = vec![vec![0u8], vec![0u8]];
            assert_eq!(count_logical_errors(&preds, &obs), 0);
        }

        #[test]
        fn test_count_logical_errors_one() {
            // 2 shots, 1 observable, shot 1 mispredicted
            let mut obs = BitTable::new(2, 1);
            obs.set(0, 0, false);
            obs.set(1, 0, true);  // actual flip
            let preds = vec![vec![0u8], vec![0u8]];  // predicted no flip for shot 1
            assert_eq!(count_logical_errors(&preds, &obs), 1);
        }

        #[test]
        fn test_count_logical_errors_multi_obs() {
            // 1 shot, 2 observables; shot counted once even if both wrong
            let mut obs = BitTable::new(1, 2);
            obs.set(0, 0, true);
            obs.set(0, 1, true);
            let preds = vec![vec![0u8, 0u8]];  // both wrong
            assert_eq!(count_logical_errors(&preds, &obs), 1);
        }
    }
}

fn main() {}
