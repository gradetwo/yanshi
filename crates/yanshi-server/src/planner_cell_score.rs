//! Residual cell scoring for the painting planner (peak-aware, round 4 phase 4).
//!
//! The round-3 oil planner ranked residual cells by their **mean** per-pixel
//! absolute channel difference, took the top `N` cells, and stopped at a
//! cutoff. A thin, high-contrast structure inside a cell is diluted by the
//! cell mean: the church-spire apex cell is the standing example. Measured
//! 2026-10-29 on the reconstructed round-3 pre-b12 render (blur 12, cell
//! size 48, `N = 2500`, grid 80 x 63 = 5040 cells), the apex cell's mean
//! residual was 17.77, rank 2809 (below the median 19.17, **not selected**),
//! while the same cell unblurred scored 34.11, rank 2204 (selected). The
//! tower-body cell behaved the same way (blurred mean 12.28, rank 3980).
//! The coarse b30 pass had already painted the apex with a fat 60-pixel
//! brush, so after b12 missed it, no later pass revisited it:
//! "one fat smear, never refined".
//!
//! The fix is a peak-aware cell score: the **95th percentile** of the
//! per-pixel residuals inside the cell (nearest-rank definition, see
//! [`cell_scores_p95`]), instead of the mean. On the same b12 device the
//! apex cell scores 33.67, rank 1760, and the tower cell 38.67, rank 1137
//! (both selected), while 2043 of the old top 2500 mean cells (about 82%)
//! stay selected and every newly admitted cell still has a mean residual
//! just below the old cutoff (median 16.81, max 19.29) -- the change
//! re-admits near-miss fine-structure cells, it does not promote speckle.
//! Only tuning the blur would have masked this one example without fixing
//! the "mean cell + N-cap drops fine structure" defect, so the blur
//! parameters are deliberately left untouched.
//!
//! Grid semantics mirror the round-3 planner exactly:
//!
//! * `nx = width / size`, `ny = height / size` (integer division); only
//!   the leading `nx * size` by `ny * size` pixel prefix is gridded, any
//!   right/bottom remainder strip is ignored.
//! * Cells are row-major: cell `(ix, iy)` has index `iy * nx + ix`.
//! * The caller supplies the per-pixel residual (`diff`), i.e. the mean
//!   absolute channel difference between reference and render, already
//!   blurred with the segment's blur. This module never blurs anything.
//! * Selection mirrors the planner loop: rank cells by score descending,
//!   take at most `N`, and stop at the cutoff (the planner's 2.0
//!   truncation, in score units). Because ranking is descending, "stop at
//!   the first cell below the cutoff" and "keep cells at or above the
//!   cutoff" are the same rule.
//!
//! This module is the canonical scoring rule for round-4 painting drivers:
//! a driver must rank its residual cells with [`cell_scores_p95`] (or an
//! implementation proven to agree with it on the b12 device), not with
//! [`cell_scores_mean`], which is kept only for comparison and tests.
//! There is currently no server-side caller; the rule is consumed by
//! painting drivers and guarded by the unit tests below plus the
//! package's real-input check (reconstructed pre-b12 render vs reference).

/// Number of cells along each axis for a `size` grid over a
/// `width` by `height` image, following the planner's integer division.
/// Returns `(0, 0)` for a zero cell size or a zero-sized image.
pub fn grid_shape(width: usize, height: usize, size: usize) -> (usize, usize) {
    if size == 0 || width == 0 || height == 0 {
        return (0, 0);
    }
    (width / size, height / size)
}

/// Row-major index of cell `(ix, iy)` in a grid with `nx` columns.
pub fn cell_index(ix: usize, iy: usize, nx: usize) -> usize {
    iy * nx + ix
}

/// Collect the per-pixel residuals of one cell, row by row, from a
/// row-major `diff` image of `width` pixels per row.
fn cell_pixels(diff: &[f32], width: usize, ix: usize, iy: usize, size: usize) -> Vec<f32> {
    let mut pixels = Vec::with_capacity(size * size);
    for dy in 0..size {
        let row = (iy * size + dy) * width + ix * size;
        pixels.extend_from_slice(&diff[row..row + size]);
    }
    pixels
}

/// Mean cell score: the round-3 planner's score, kept for comparison.
/// Cells are row-major over the gridded prefix; returns an empty vector
/// when the grid is degenerate or `diff` is shorter than `width * height`.
pub fn cell_scores_mean(diff: &[f32], width: usize, height: usize, size: usize) -> Vec<f32> {
    let (nx, ny) = grid_shape(width, height, size);
    if nx == 0 || ny == 0 || diff.len() < width * height {
        return Vec::new();
    }
    let mut scores = Vec::with_capacity(nx * ny);
    for iy in 0..ny {
        for ix in 0..nx {
            let pixels = cell_pixels(diff, width, ix, iy, size);
            let sum: f32 = pixels.iter().sum();
            scores.push(sum / pixels.len() as f32);
        }
    }
    scores
}

/// Peak-aware cell score: the 95th percentile (nearest-rank) of the
/// per-pixel residuals inside each cell.
///
/// Nearest-rank definition: sort the cell's `n = size * size` residuals
/// ascending and take element `ceil(0.95 * n) - 1` (0-based). A thin
/// structure covering at least 5% of a cell therefore registers, while a
/// lone hot pixel (below 5% of the cell) cannot dominate the score the
/// way a bare maximum would. Returns an empty vector on degenerate input,
/// exactly like [`cell_scores_mean`].
pub fn cell_scores_p95(diff: &[f32], width: usize, height: usize, size: usize) -> Vec<f32> {
    let (nx, ny) = grid_shape(width, height, size);
    if nx == 0 || ny == 0 || diff.len() < width * height {
        return Vec::new();
    }
    let n = size * size;
    // Nearest-rank index for p = 0.95: ceil(0.95 * n) - 1, 0-based.
    let rank = (95 * n).div_ceil(100) - 1;
    let mut scores = Vec::with_capacity(nx * ny);
    for iy in 0..ny {
        for ix in 0..nx {
            let mut pixels = cell_pixels(diff, width, ix, iy, size);
            pixels.sort_by(|a, b| a.total_cmp(b));
            scores.push(pixels[rank]);
        }
    }
    scores
}

/// Cell indices ranked by score descending; ties break by ascending cell
/// index, so the ranking is deterministic for equal scores.
pub fn ranked_cells(scores: &[f32]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..scores.len()).collect();
    order.sort_by(|&a, &b| scores[b].total_cmp(&scores[a]).then(a.cmp(&b)));
    order
}

/// Planner selection: the ranked cells, at most `n`, stopping at the
/// `cutoff` (cells scoring below it are excluded, as are all cells after
/// them in the descending ranking).
pub fn select_cells(scores: &[f32], n: usize, cutoff: f32) -> Vec<usize> {
    ranked_cells(scores)
        .into_iter()
        .take(n)
        .take_while(|&idx| scores[idx] >= cutoff)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a row-major diff image from per-cell uniform values.
    /// `cells` is row-major over `nx` columns; each cell is `size` square.
    fn diff_from_cells(cells: &[f32], nx: usize, size: usize) -> (Vec<f32>, usize, usize) {
        let ny = cells.len() / nx;
        let width = nx * size;
        let height = ny * size;
        let mut diff = vec![0.0; width * height];
        for (idx, &value) in cells.iter().enumerate() {
            let (ix, iy) = (idx % nx, idx / nx);
            for dy in 0..size {
                for dx in 0..size {
                    diff[(iy * size + dy) * width + ix * size + dx] = value;
                }
            }
        }
        (diff, width, height)
    }

    /// The b12 apex pattern, in miniature: a thin-structure cell whose
    /// mean sits below the background cells' mean, but whose p95 towers
    /// over them, plus one broad high-mean cell that must stay on top
    /// under both scores. Under the mean score the thin cell is ranked
    /// last and missed by the N-cap; under p95 it is ranked first.
    #[test]
    fn thin_structure_cell_selected_by_p95_not_by_mean() {
        // Grid: 4 columns x 2 rows, cell size 4 (16 pixels per cell).
        // Cell 0: thin structure -- 2 pixels at 95, 14 pixels at 5.
        //   mean = (2*95 + 14*5) / 16 = 16.25 (below background 20).
        //   p95  = 95 (nearest-rank index 15 of 16 sorted pixels).
        // Cells 1..6: uniform background, value 20 (mean = p95 = 20).
        // Cell 7: broad block, uniform 60 (mean = p95 = 60).
        let size = 4;
        let nx = 4;
        let mut diff = vec![20.0; nx * size * 2 * size];
        // Cell 0 pixels: first row of the cell gets the two hot pixels.
        diff[0] = 95.0;
        diff[1] = 95.0;
        for dy in 0..size {
            for dx in 0..size {
                if !(dy == 0 && dx < 2) {
                    diff[dy * nx * size + dx] = 5.0;
                }
            }
        }
        // Cell 7 (bottom-right): uniform 60.
        for dy in 0..size {
            for dx in 0..size {
                diff[(size + dy) * nx * size + 3 * size + dx] = 60.0;
            }
        }
        let width = nx * size;
        let height = 2 * size;
        let mean = cell_scores_mean(&diff, width, height, size);
        let p95 = cell_scores_p95(&diff, width, height, size);
        assert_eq!(mean.len(), 8);
        assert_eq!(p95.len(), 8);
        assert!((mean[0] - 16.25).abs() < 1e-4, "thin cell mean");
        assert!((p95[0] - 95.0).abs() < f32::EPSILON, "thin cell p95");
        assert!((mean[7] - 60.0).abs() < 1e-4, "broad cell mean");
        assert!((p95[7] - 60.0).abs() < f32::EPSILON, "broad cell p95");
        // N-cap 2: mean selection misses the thin cell, p95 picks it.
        assert_eq!(select_cells(&mean, 2, 2.0), vec![7, 1]);
        assert_eq!(select_cells(&p95, 2, 2.0), vec![0, 7]);
        // Full ranking: mean puts the thin cell last, p95 first.
        assert_eq!(ranked_cells(&mean)[7], 0);
        assert_eq!(ranked_cells(&p95)[0], 0);
    }

    /// The planner grids only the leading prefix: a huge-valued right
    /// strip and bottom strip must not move any cell score.
    #[test]
    fn grid_drops_partial_edge_strip() {
        // 10 x 6 image, size 4 -> 2 x 1 cells over the leading 8 x 4 prefix.
        let (mut diff, width, height) = diff_from_cells(&[10.0, 30.0], 2, 4);
        assert_eq!((width, height), (8, 4));
        // Widen to 10 x 6 with 999 in the remainder strip.
        let mut wide = vec![999.0; 10 * 6];
        for y in 0..4 {
            for x in 0..8 {
                wide[y * 10 + x] = diff[y * 8 + x];
            }
        }
        std::mem::swap(&mut diff, &mut wide);
        let mean = cell_scores_mean(&diff, 10, 6, 4);
        let p95 = cell_scores_p95(&diff, 10, 6, 4);
        assert_eq!(mean, vec![10.0, 30.0]);
        assert_eq!(p95, vec![10.0, 30.0]);
    }

    /// Selection obeys both the N-cap and the cutoff, in ranked order.
    #[test]
    fn selection_stops_at_cutoff_and_cap() {
        let scores = vec![50.0, 30.0, 1.5, 40.0];
        assert_eq!(ranked_cells(&scores), vec![0, 3, 1, 2]);
        assert_eq!(select_cells(&scores, 10, 2.0), vec![0, 3, 1]);
        assert_eq!(select_cells(&scores, 2, 2.0), vec![0, 3]);
        assert!(select_cells(&scores, 10, 100.0).is_empty());
    }

    /// Equal scores rank by ascending cell index, deterministically.
    #[test]
    fn ranked_cells_tie_break_is_deterministic() {
        assert_eq!(ranked_cells(&[10.0, 10.0, 5.0]), vec![0, 1, 2]);
        assert_eq!(ranked_cells(&[5.0, 10.0, 10.0]), vec![1, 2, 0]);
    }

    /// A uniform cell scores identically under both rules, so the new
    /// rule cannot drift on flat regions.
    #[test]
    fn mean_and_p95_agree_on_uniform_cells() {
        let (diff, width, height) = diff_from_cells(&[7.0, 42.0, 0.0], 3, 5);
        assert_eq!(
            cell_scores_mean(&diff, width, height, 5),
            vec![7.0, 42.0, 0.0]
        );
        assert_eq!(
            cell_scores_p95(&diff, width, height, 5),
            vec![7.0, 42.0, 0.0]
        );
    }

    /// Degenerate inputs yield no cells instead of panicking.
    #[test]
    fn degenerate_inputs_yield_no_cells() {
        assert!(cell_scores_mean(&[], 0, 0, 0).is_empty());
        assert!(cell_scores_p95(&[1.0, 2.0], 4, 4, 2).is_empty());
        assert_eq!(grid_shape(3840, 3041, 0), (0, 0));
        assert_eq!(cell_index(43, 39, 80), 39 * 80 + 43);
    }
}
