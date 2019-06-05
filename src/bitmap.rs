/// A growable BitMap provided as a trait, which is by default implmented for `Vec<u8>`.
pub trait BitMap {
    /// Get the bit at the index `n`.
    fn get_bit(&self, n: usize) -> bool;
    /// Set the bit at the index `n`.
    fn set_bit(&mut self, n: usize);
    /// Check whether all bits of which the indices are *less than* `n` (exclusive) are set.
    fn is_set_up_to(&mut self, n: usize) -> bool;
    /// Truncate the BitMap: shrink the underlying storage as much as possible and make all bits of
    /// which the indices are greater than `n` unset.
    fn truncate_to_bit(&mut self, n: usize);
}

impl BitMap for Vec<u8> {
    fn get_bit(&self, n: usize) -> bool {
        let offset_by_byte = n / 8;
        if offset_by_byte >= self.len() {
            return false;
        }
        let offset_in_byte = n % 8;
        self[offset_by_byte] & (1 << offset_in_byte) > 0
    }

    fn set_bit(&mut self, n: usize) {
        let offset_by_byte = n / 8;
        if offset_by_byte >= self.len() {
            self.resize(offset_by_byte + 1, 0);
        }
        let offset_in_byte = n % 8;
        self[offset_by_byte] = self[offset_by_byte] | (1 << offset_in_byte);
    }

    fn is_set_up_to(&mut self, n: usize) -> bool {
        if n == 0 {
            return false;
        }
        let n = n - 1;
        let offset_by_byte = n / 8;
        if offset_by_byte >= self.len() {
            return false;
        }
        for &byte in self.iter().take(offset_by_byte) {
            if byte != 0xFF {
                return false;
            }
        }
        let offset_in_byte = n % 8;
        let mask = (1 << ((offset_in_byte + 1) % 8)) - 1;
        self[offset_by_byte] & mask == mask
    }

    fn truncate_to_bit(&mut self, n: usize) {
        let offset_by_byte = n / 8;
        self.truncate(offset_by_byte + 1);
        let offset_in_byte = n % 8;
        self[offset_by_byte] &= (1 << (offset_in_byte + 1)) - 1;
    }
}

#[cfg(test)]
mod test {
    use super::BitMap;
    use std::collections::HashSet;

    #[test]
    fn test_get_set() {
        let mut bitmap: Vec<u8> = vec![];
        let idxs: HashSet<usize> = [0, 3, 19, 1023, 1024, 65535, 65536, 65537, 1024768]
            .iter()
            .cloned()
            .collect();
        for &idx in idxs.iter() {
            bitmap.set_bit(idx);
        }
        for idx in 0..(idxs.iter().max().unwrap() + 1024) {
            assert!(bitmap.get_bit(idx) == idxs.contains(&idx));
        }
    }

    #[test]
    fn test_truncate() {
        let mut bitmap: Vec<u8> = vec![];
        bitmap.set_bit(64);
        bitmap.set_bit(65);
        bitmap.set_bit(66);
        assert_eq!(bitmap.len(), 9);
        bitmap.truncate_to_bit(64);
        assert_eq!(bitmap.len(), 9);
        dbg!(bitmap[8]);
        assert_eq!(bitmap[8], 0b1);
    }

    #[test]
    fn test_is_all_set() {
        let mut bitmap: Vec<u8> = vec![];
        for i in 0..=64 {
            bitmap.set_bit(i);
        }
        assert!(bitmap.is_set_up_to(63));
        assert!(bitmap.is_set_up_to(64));
        assert!(bitmap.is_set_up_to(65));
        assert!(!bitmap.is_set_up_to(66));
    }
}
