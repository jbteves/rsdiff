/// Library for rsdiff

// ----------
// Public API
// ----------

use std::{
    fs::{self, File},
    io::{BufRead, BufReader}
};

/// Diff
/// Generalized object for performing abstract diffs.
#[derive(Debug)]
pub struct Diff<'a> {
    /// The left object for the diff.
    pub left: &'a str,
    /// The right object for the diff.
    pub right: &'a str,
    /// Whether the objects match
    pub matches: bool,
    /// Objects which only exist in the left object.
    pub left_only: Vec<&'a str>,
    /// Objects which only exist in the right object.
    pub right_only: Vec<&'a str>,
    /// Objects which are common to the left and right objects.
    pub common: Vec<&'a str>,
    /// Generalized similarity index.
    pub similarity: f32,
    /// Any additional information.
    pub additional_info: String,
    /// Any sub-diffs that may want to be represented, for example, if this
    /// Diff object represents a directory that may contain files that also
    /// have diffs.
    pub sub_diffs: Vec<Box<Diff<'a>>>,
    /// The string report that may be printed.
    pub report: String,
}

impl <'a> Diff<'a> {
    /// Create a default diff to be built on.
    pub fn new<'b>(left: &'b str, right: &'b str) -> Diff<'b> {
        Diff {
            left,
            right,
            matches: false,
            left_only: vec!(),
            right_only: vec!(),
            common: vec!(),
            similarity: -1.0,
            additional_info: String::from(""),
            sub_diffs: vec!(),
            report: String::from(""),
        }
    }
}

/// Calculate an abstract diff between two files.
pub fn differ<'a>(left: &'a str, right: &'a str) -> Diff<'a> {
    diff_bytes(left, right)
}

/// Perform a diff on two files of unknown or binary encoding.
pub fn diff_bytes<'a>(left: &'a str, right: &'a str) -> Diff<'a> {
    // Obtain metadata
    let left_meta = fs::metadata(left).expect("Left file didn't exist");
    let right_meta = fs::metadata(right).expect("Right file didn't exist");

    // Check that both left and right are files
    if !(left_meta.is_file()) {
        if !(right_meta.is_file()) {
            panic!("Left and right are not files!")
        }
        else {
            panic!("Left is not a file!")
        }
    }

    // Initialize the Diff object, since one may be computed
    let mut d = Diff::new(left, right);
    
    if left_meta.len() == right_meta.len() {
        // Iterate over 8kB chunks to compare bytes
        const CHUNK_SIZE: usize = 8192;
        // Track the length of the files with a convenient alias
        let fsize = left_meta.len();
        // File pointers and buffer readers
        let left_file = File::open(left).expect("Uh-oh!");
        let right_file = File::open(right).expect("Uh-oh!");
        let mut left_reader = BufReader::with_capacity(
            CHUNK_SIZE, left_file
        );
        let mut right_reader = BufReader::with_capacity(
            CHUNK_SIZE, right_file
        );
        // Track total matches
        let mut total_matches = 0;
        // Loop until we hit EOF
        loop {
            // Ask to read, get a length for how many bytes were read
            let left_buffer = left_reader.fill_buf().expect("Uh-oh 2!");
            let right_buffer = right_reader.fill_buf().expect("Uh-oh 2!");
            // Left and right buffer should be same; we'll reference left
            if left_buffer.len() != 0 {
                // We have bytes to compare
                total_matches += diff_buffer(left_buffer, right_buffer);
            }
            else {
                // We hit EOF
                break;
            }
        }
        // See if it's a complete match
        d.matches = total_matches == fsize;
        // Fill in similarity index
        let similarity = total_matches as f32 / fsize as f32;
        d.similarity = similarity;
        // If not a complete match, need to fill in additional info
        if d.matches {
            let percentage = similarity * 100.0;
            d.additional_info = String::from(
                format!(
                    "{} of {} bytes match ({2:.1}%)",
                    total_matches,
                    fsize,
                    percentage
                )
            );
        }
    }
    else {
        // File size mismatch
        d.additional_info = String::from(
            format!(
                "file sizes differ: {} vs. {}",
                left_meta.len(),
                right_meta.len()
            )
        );
    }

    if !d.matches {
        // Generate report
        d.report = String::from(
            format!("{} vs {}: {}", d.left, d.right, d.additional_info)
        );
    }
    
    return d;
}


/// Calculate how many bytes match between two buffers. The buffers must be
/// of equal size.
pub fn diff_buffer<'a>(left: &[u8], right: &[u8]) -> u64 {
    // Verify arrays match in size
    if !(left.len() == right.len()) {
        panic!("Buffers supplied to rsdiff::diff_buffer must have the \
               same length! Instead, left is size {} and right is size {}",
               left.len(), right.len());
    }
    // Iterate and compare bytes
    let mut matches: u64 = 0;
    for it in left.iter().zip(right.iter()) {
        let (a, b) = it;
        matches += (a == b) as u64;
    }
    return matches
}


// ------------------------
// Private Helper Functions
// ------------------------
/// PositiveIndex
/// Object for constraining a value to the range [0.0, 1.0].
#[derive(Debug)]
struct PositiveIndex {
    /// The value held by the index
    value: f32
}

impl PositiveIndex {
    /// Create a new PositiveIndex from floating-point
    fn from(value: f32) -> PositiveIndex {
        if value < 0.0  || value > 1.0 {
            panic!(
                "PositiveIndex must be in [0.0, 1.0], but has value {}",
                value
            )
        }
        PositiveIndex { value }
    }
    /// Create a zero-index
    fn zero() -> PositiveIndex { PositiveIndex { value: 0.0 } }
    /// Create a one-index
    fn one() -> PositiveIndex { PositiveIndex { value: 1.0 } }
    /// Get the underlying value
    fn value(&self) -> f32 { return self.value }
}

// -----
// Tests
// -----
/// Tests for the rsdiff library
#[cfg(test)]
mod tests {
    /// Tests for PositiveIndex
    mod positive_index{
        use crate::PositiveIndex;
        /// Make sure that PositiveIndex::from fails for negative float
        #[test]
        #[should_panic(expected = "PositiveIndex must be in [0.0, 1.0], but has value -0.5")]
        fn from_panics_negative() {
            PositiveIndex::from(-0.5);
        }
        /// Make sure that PositiveIndex::from fails for float > 1.0
        #[test]
        #[should_panic(expected = "PositiveIndex must be in [0.0, 1.0], but has value 1.1")]
        fn from_panics_large() {
            PositiveIndex::from(1.1);
        }
        /// Make sure that PositiveIndex::from and PositiveIndex::value
        /// work as intended.
        #[test]
        fn from_value_works() {
            let val = PositiveIndex::from(0.5).value();
            assert!(val == 0.5);
        }
        /// Make sure that PositiveIndex::zero works
        #[test]
        fn zero_works() {
            let pi = PositiveIndex::zero();
            assert!(pi.value == 0.0);
        }
        /// Make sure that PositiveIndex::one works
        #[test]
        fn one_works() {
            let pi = PositiveIndex::one();
            assert!(pi.value == 1.0);
        }
    }
}
