//! Prefix sums over block heights, with updates.
//!
//! The plan needs three things at interactive speed on a document with
//! hundreds of thousands of blocks: the total height, the top edge of any
//! block, and the block at a given scroll position. A plain prefix-sum array
//! answers the last two in constant time but costs a full rebuild whenever one
//! height changes — and heights change constantly, because every block that
//! scrolls into view replaces its estimate.
//!
//! A Fenwick tree makes all three logarithmic, which is what lets a
//! measurement be folded in during scrolling without a pass over the document.

pub struct Offsets {
    /// One-indexed; `tree[i]` covers a range ending at `i`.
    tree: Vec<f64>,
    len: usize,
    total: f64,
}

impl Offsets {
    pub fn new(heights: impl Iterator<Item = f64>) -> Self {
        let values: Vec<f64> = heights.collect();
        let len = values.len();
        let mut tree = vec![0.0; len + 1];
        let mut total = 0.0;
        // Linear build: add each value to its own node, then fold that node
        // into its parent.
        for (index, value) in values.iter().enumerate() {
            let node = index + 1;
            tree[node] += value;
            total += value;
            let parent = node + node.isolate_lowest_one();
            if parent <= len {
                let carry = tree[node];
                tree[parent] += carry;
            }
        }
        Offsets { tree, len, total }
    }

    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn total(&self) -> f64 {
        self.total
    }

    /// Sum of the first `index` heights, so the top edge of block `index`.
    pub fn prefix(&self, index: usize) -> f64 {
        let mut node = index.min(self.len);
        let mut sum = 0.0;
        while node > 0 {
            sum += self.tree[node];
            node -= node.isolate_lowest_one();
        }
        sum
    }

    pub fn add(&mut self, index: usize, delta: f64) {
        debug_assert!(index < self.len);
        self.total += delta;
        let mut node = index + 1;
        while node <= self.len {
            self.tree[node] += delta;
            node += node.isolate_lowest_one();
        }
    }

    /// The index whose block contains `y`, by binary lifting over the tree.
    /// Heights are never negative, so the prefix sums are monotonic and this is
    /// well defined.
    pub fn search(&self, y: f64) -> usize {
        if self.len == 0 {
            return 0;
        }
        let mut position = 0usize;
        let mut remaining = y;
        let mut step = 1usize << (usize::BITS - 1 - self.len.leading_zeros());
        while step > 0 {
            let candidate = position + step;
            if candidate <= self.len && self.tree[candidate] <= remaining {
                position = candidate;
                remaining -= self.tree[candidate];
            }
            step >>= 1;
        }
        position.min(self.len - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::Offsets;

    fn naive(heights: &[f64], index: usize) -> f64 {
        heights[..index].iter().sum()
    }

    #[test]
    fn prefix_matches_a_plain_sum() {
        let heights: Vec<f64> = (0..100).map(|i| 10.0 + (i % 7) as f64).collect();
        let offsets = Offsets::new(heights.iter().copied());
        for index in 0..=heights.len() {
            assert!((offsets.prefix(index) - naive(&heights, index)).abs() < 1e-9);
        }
        assert!((offsets.total() - naive(&heights, heights.len())).abs() < 1e-9);
    }

    #[test]
    fn an_update_moves_every_later_offset_and_no_earlier_one() {
        let heights: Vec<f64> = vec![20.0; 50];
        let mut offsets = Offsets::new(heights.iter().copied());
        let before: Vec<f64> = (0..=50).map(|i| offsets.prefix(i)).collect();
        offsets.add(20, 5.0);
        for (index, was) in before.iter().enumerate().take(21) {
            assert_eq!(offsets.prefix(index), *was, "offset {index} moved");
        }
        for (index, was) in before.iter().enumerate().skip(21) {
            assert!((offsets.prefix(index) - (was + 5.0)).abs() < 1e-9);
        }
    }

    #[test]
    fn search_finds_the_block_covering_a_position() {
        let heights: Vec<f64> = vec![10.0, 20.0, 5.0, 30.0];
        let offsets = Offsets::new(heights.iter().copied());
        // Boundaries belong to the block that starts there.
        assert_eq!(offsets.search(0.0), 0);
        assert_eq!(offsets.search(9.9), 0);
        assert_eq!(offsets.search(10.0), 1);
        assert_eq!(offsets.search(29.9), 1);
        assert_eq!(offsets.search(30.0), 2);
        assert_eq!(offsets.search(35.0), 3);
        // Past the end clamps to the last block.
        assert_eq!(offsets.search(1000.0), 3);
    }

    #[test]
    fn search_agrees_with_prefix_after_updates() {
        let mut offsets = Offsets::new((0..64).map(|i| 8.0 + (i % 5) as f64));
        for index in (0..64).step_by(3) {
            offsets.add(index, 7.0);
        }
        for index in 0..64 {
            let top = offsets.prefix(index);
            assert_eq!(offsets.search(top), index, "at top of block {index}");
            assert_eq!(offsets.search(top + 0.5), index, "inside block {index}");
        }
    }

    #[test]
    fn an_empty_document_has_no_offsets() {
        let offsets = Offsets::new(std::iter::empty());
        assert_eq!(offsets.len(), 0);
        assert_eq!(offsets.total(), 0.0);
        assert_eq!(offsets.search(10.0), 0);
    }
}
