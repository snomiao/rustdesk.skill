//! `Array.prototype.sort(comparator)` as bun's JavaScriptCore does it
//! (Source/JavaScriptCore/runtime/StableSort.h, `arrayStableSort` with
//! galloping merges: binary insertion sort below 8 elements, otherwise
//! Powersort over runs, short runs extended by insertion sort to 64).
//!
//! For a comparator that orders the elements consistently every stable sort
//! gives the same result. `(a, b) => a.y - b.y` does not when a `y` is NaN
//! or two are infinite (NaN is "not less" either way), and then only the
//! same steps give the same order: these are those steps, with the
//! comparator's result taken as JavaScriptCore takes it (`less(a, b)` is
//! `comparator(a, b) < 0`). rs/src/compare.rs holds it against bun.

/// Sorts `items` as `items.sort(comparator)` would, `less(a, b)` being
/// `comparator(a, b) < 0`.
pub fn sort<T: Copy>(items: &mut [T], less: impl Fn(&T, &T) -> bool) {
    const EXTEND_RUN_CUTOFF: usize = 8;
    const FORCE_RUN_LENGTH: usize = 64;
    let n = items.len();
    if n == 0 {
        return;
    }
    if n < EXTEND_RUN_CUTOFF {
        insertion_sort(items, &less, 0);
        return;
    }
    let mut work: Vec<T> = items.to_vec();
    let mut min_gallop = MIN_GALLOP_THRESHOLD;
    // A run is [begin, end], end included.
    let mut stack: Vec<((usize, usize), u32)> = Vec::new();
    let next_run = |items: &mut [T], begin: usize| -> (usize, usize) {
        let mut end = extend_and_normalize_run(items, begin, &less);
        if end - begin < EXTEND_RUN_CUTOFF {
            let size = FORCE_RUN_LENGTH.min(n - begin);
            insertion_sort(&mut items[begin..begin + size], &less, end - begin);
            end = begin + size - 1;
        }
        while end + 1 < n {
            if less(&items[end + 1], &items[end]) {
                break;
            }
            end += 1;
        }
        (begin, end)
    };
    let mut run1 = next_run(items, 0);
    while run1.1 + 1 < n {
        let run2 = next_run(items, run1.1 + 1);
        let p = power(run1.0, run2.0, run2.1, n);
        while stack.last().is_some_and(|(_, power)| *power > p) {
            let (merge, _) = stack.pop().unwrap();
            merge_runs(&mut work, items, merge.0, merge.1 + 1, run1.0, run1.1 + 1, &less, &mut min_gallop);
            items[merge.0..run1.1 + 1].copy_from_slice(&work[merge.0..run1.1 + 1]);
            run1.0 = merge.0;
        }
        stack.push((run1, p));
        run1 = run2;
    }
    while let Some((merge, _)) = stack.pop() {
        merge_runs(&mut work, items, merge.0, merge.1 + 1, run1.0, run1.1 + 1, &less, &mut min_gallop);
        items[merge.0..run1.1 + 1].copy_from_slice(&work[merge.0..run1.1 + 1]);
        run1.0 = merge.0;
    }
}

const MIN_GALLOP_THRESHOLD: usize = 7;

/// `arrayInsertionSort`: binary insertion, the first `sorted` elements taken
/// as sorted already.
fn insertion_sort<T: Copy>(items: &mut [T], less: &impl Fn(&T, &T) -> bool, sorted: usize) {
    for i in sorted + 1..items.len() {
        let value = items[i];
        let (mut left, mut right) = (0, i);
        while left < right {
            let m = left + (right - left) / 2;
            if less(&value, &items[m]) {
                right = m;
            } else {
                left = m + 1;
            }
        }
        items.copy_within(left..i, left + 1);
        items[left] = value;
    }
}

/// `extendAndNormalizeRun`: the end (included) of the run at `begin`, a
/// strictly descending one reversed.
fn extend_and_normalize_run<T: Copy>(items: &mut [T], begin: usize, less: &impl Fn(&T, &T) -> bool) -> usize {
    let n = items.len();
    let mut end = begin;
    if end + 1 >= n {
        return end;
    }
    if less(&items[end + 1], &items[end]) {
        end += 1;
        while end + 1 < n && less(&items[end + 1], &items[end]) {
            end += 1;
        }
        items[begin..end + 1].reverse();
    } else {
        end += 1;
        while end + 1 < n && !less(&items[end + 1], &items[end]) {
            end += 1;
        }
    }
    end
}

/// The node power of two neighbouring runs, [left, middle) and [middle, right].
fn power(left: usize, middle: usize, right: usize, n: usize) -> u32 {
    let n1 = (middle - left) as u128;
    let n2 = (right - middle + 1) as u128;
    let a = ((left as u128) * 2 + n1) << 62;
    let b = ((middle as u128) * 2 + n2) << 62;
    let n = n as u128;
    (((a / n) ^ (b / n)) as u64).leading_zeros()
}

/// `gallopLeft`: k with base[k-1] < key <= base[k].
fn gallop_left<T>(key: &T, base: &[T], hint: usize, less: &impl Fn(&T, &T) -> bool) -> usize {
    let length = base.len();
    let (mut last, mut offset) = (0usize, 1usize);
    if less(&base[hint], key) {
        let max = length - hint;
        while offset < max {
            if !less(&base[hint + offset], key) {
                break;
            }
            last = offset;
            let next = (offset << 1) + 1;
            offset = if next > offset { next.min(max) } else { max };
        }
        last += hint;
        offset += hint;
    } else {
        let max = hint + 1;
        while offset < max {
            if less(&base[hint - offset], key) {
                break;
            }
            last = offset;
            let next = (offset << 1) + 1;
            offset = if next > offset { next.min(max) } else { max };
        }
        let tmp = last;
        last = hint.wrapping_sub(offset);
        offset = hint - tmp;
    }
    last = last.wrapping_add(1);
    while last < offset {
        let m = last + ((offset - last) >> 1);
        if less(&base[m], key) {
            last = m + 1;
        } else {
            offset = m;
        }
    }
    offset
}

/// `gallopRight`: k with base[k-1] <= key < base[k].
fn gallop_right<T>(key: &T, base: &[T], hint: usize, less: &impl Fn(&T, &T) -> bool) -> usize {
    let length = base.len();
    let (mut last, mut offset) = (0usize, 1usize);
    if less(key, &base[hint]) {
        let max = hint + 1;
        while offset < max {
            if !less(key, &base[hint - offset]) {
                break;
            }
            last = offset;
            let next = (offset << 1) + 1;
            offset = if next > offset { next.min(max) } else { max };
        }
        let tmp = last;
        last = hint.wrapping_sub(offset);
        offset = hint - tmp;
    } else {
        let max = length - hint;
        while offset < max {
            if less(key, &base[hint + offset]) {
                break;
            }
            last = offset;
            let next = (offset << 1) + 1;
            offset = if next > offset { next.min(max) } else { max };
        }
        last += hint;
        offset += hint;
    }
    last = last.wrapping_add(1);
    while last < offset {
        let m = last + ((offset - last) >> 1);
        if less(key, &base[m]) {
            offset = m;
        } else {
            last = m + 1;
        }
    }
    offset
}

/// `mergePowersortRuns`: src[i1..e1) and src[i2..e2) merged into dst.
#[allow(clippy::too_many_arguments)]
fn merge_runs<T: Copy>(dst: &mut [T], src: &[T], i1: usize, e1: usize, i2: usize, e2: usize, less: &impl Fn(&T, &T) -> bool, min_gallop: &mut usize) {
    let mut left_length = e1 - i1;
    let mut right_length = e2 - i2;
    if left_length == 0 || right_length == 0 {
        dst[i1..e2].copy_from_slice(&src[i1..e2]);
        return;
    }
    let skip_left = gallop_right(&src[i2], &src[i1..e1], 0, less);
    dst[i1..i1 + skip_left].copy_from_slice(&src[i1..i1 + skip_left]);
    let mut left = i1 + skip_left;
    left_length -= skip_left;
    if left_length == 0 {
        dst[i2..i2 + right_length].copy_from_slice(&src[i2..i2 + right_length]);
        return;
    }
    let skip_right = right_length - gallop_left(&src[e1 - 1], &src[i2..e2], right_length - 1, less);
    dst[e2 - skip_right..e2].copy_from_slice(&src[e2 - skip_right..e2]);
    let right_end = e2 - skip_right;
    right_length -= skip_right;
    if right_length == 0 {
        dst[left..left + left_length].copy_from_slice(&src[left..left + left_length]);
        return;
    }
    let left_end = e1;
    let mut right = i2;
    let mut at = left;
    'merge: loop {
        let (mut left_wins, mut right_wins) = (0usize, 0usize);
        while left_wins < *min_gallop && right_wins < *min_gallop {
            if right < right_end && left < left_end {
                if less(&src[right], &src[left]) {
                    dst[at] = src[right];
                    at += 1;
                    right += 1;
                    right_wins += 1;
                    left_wins = 0;
                } else {
                    dst[at] = src[left];
                    at += 1;
                    left += 1;
                    left_wins += 1;
                    right_wins = 0;
                }
            } else {
                break 'merge;
            }
        }
        *min_gallop += 1;
        loop {
            if *min_gallop > 1 {
                *min_gallop -= 1;
            }
            if left >= left_end || right >= right_end {
                break 'merge;
            }
            let k = gallop_right(&src[right], &src[left..left_end], 0, less);
            left_wins = k;
            dst[at..at + k].copy_from_slice(&src[left..left + k]);
            at += k;
            left += k;
            dst[at] = src[right];
            at += 1;
            right += 1;
            if left >= left_end || right >= right_end {
                break 'merge;
            }
            let k = gallop_left(&src[left], &src[right..right_end], 0, less);
            right_wins = k;
            dst[at..at + k].copy_from_slice(&src[right..right + k]);
            at += k;
            right += k;
            dst[at] = src[left];
            at += 1;
            left += 1;
            if left >= left_end || right >= right_end {
                break 'merge;
            }
            if !(left_wins >= MIN_GALLOP_THRESHOLD || right_wins >= MIN_GALLOP_THRESHOLD) {
                break;
            }
        }
        *min_gallop += 1;
    }
    while left < left_end {
        dst[at] = src[left];
        at += 1;
        left += 1;
    }
    while right < right_end {
        dst[at] = src[right];
        at += 1;
        right += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted(ys: &[f64]) -> Vec<usize> {
        let mut items: Vec<(usize, f64)> = ys.iter().copied().enumerate().collect();
        sort(&mut items, |a, b| a.1 - b.1 < 0.0);
        items.into_iter().map(|(i, _)| i).collect()
    }

    #[test]
    fn as_bun_sorted_them() {
        // Orders bun 1.4.0 gave for (a, b) => a.y - b.y.
        let nan = f64::NAN;
        let inf = f64::INFINITY;
        assert_eq!(sorted(&[3.0, nan, inf, 0.0, 0.0]), [0, 1, 3, 4, 2]);
        assert_eq!(sorted(&[nan, nan, 0.0, 1.0, nan, 1.0, 2.0, nan, 2.0, 2.0, 0.0]), [0, 1, 2, 10, 3, 4, 5, 6, 7, 8, 9]);
        assert_eq!(sorted(&[0.0, -0.0]), [0, 1]);
        assert_eq!(sorted(&[-0.0, 0.0]), [0, 1]);
        let many: Vec<f64> = (0..300).map(|i| ((i * 7919) % 300) as f64).collect();
        let order = sorted(&many);
        assert!(order.windows(2).all(|w| many[w[0]] <= many[w[1]]));
    }
}
