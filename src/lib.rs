/// Library for rsdiff

// ----------
// Public API
// ----------

use std::{
    fs::{self, File},
    io::{BufRead, BufReader},
    convert::{TryInto},
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

    // Check to see if file sizes match; if so, figure out the total number
    // of matching bytes.
    if left_meta.len() == right_meta.len() {
        // Iterate over 256kB chunks to compare bytes. Tested with a MacOS
        // system using an SSD, picked the smallest chunk size that seemed
        // to not reduce performance.
        const KILOBYTE: usize = 1024;
        const CHUNK_SIZE: usize = 256 * KILOBYTE;
        // Track the length of the files with a convenient alias
        let fsize: usize = left_meta.len().try_into().unwrap();
        // File pointers and buffer readers
        let left_file = File::open(left).expect("Uh-oh!");
        let right_file = File::open(right).expect("Uh-oh!");
        let mut total_matches: usize = 0;
        let mut left_reader = BufReader::with_capacity(
            CHUNK_SIZE, left_file
        );
        let mut right_reader = BufReader::with_capacity(
            CHUNK_SIZE, right_file
        );
        // Track total matches
        loop {
            // Ask to read, get a length for how many bytes were read
            let length = {
                let left_buffer = left_reader.fill_buf().expect("Uh-oh 2!");
                let right_buffer = right_reader.fill_buf().expect("Uh-h 3!");
                if left_buffer.len() != 0 {
                    total_matches += diff_buffer(
                        left_buffer,
                        right_buffer);
                }
                left_buffer.len()
            };
            left_reader.consume(length);
            right_reader.consume(length);
            if length == 0 {
                break;
            }
        }
        // See if it's a complete match
        d.matches = total_matches == fsize;
        // Fill in similarity index
        let similarity = total_matches as f32 / fsize as f32;
        d.similarity = similarity;
        // If not a complete match, need to fill in additional info
        if !d.matches {
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
pub fn diff_buffer<'a>(left: &[u8], right: &[u8]) -> usize {
    // Verify arrays match in size
    if !(left.len() == right.len()) {
        panic!("Buffers supplied to rsdiff::diff_buffer must have the \
               same length! Instead, left is size {} and right is size {}",
               left.len(), right.len());
    }
    // Iterate and compare bytes
    let mut matches: usize = 0;
    for it in left.iter().zip(right.iter()) {
        let (a, b) = it;
        matches += (a == b) as usize;
    }
    return matches
}
