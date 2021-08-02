/// Library for rsdiff

// ----------
// Public API
// ----------

use std::{
    fs::{self, File},
    io::{self, BufRead, BufReader},
    convert::TryInto,
    path::Path,
    time,
};

use nifti::{NiftiObject, ReaderOptions, IntoNdArray};

/// Diff
/// Generalized object for performing abstract diffs.
#[derive(Debug)]
pub struct Diff {
    /// The left object for the diff.
    pub left: String,
    /// The right object for the diff.
    pub right: String,
    /// Whether the objects match
    pub matches: bool,
    /// Objects which only exist in the left object.
    pub left_only: Vec<String>,
    /// Objects which only exist in the right object.
    pub right_only: Vec<String>,
    /// Objects which are common to the left and right objects.
    pub common: Vec<String>,
    /// Generalized similarity index.
    pub similarity: f32,
    /// Any additional information.
    pub additional_info: String,
    /// Any sub-diffs that may want to be represented, for example, if this
    /// Diff object represents a directory that may contain files that also
    /// have diffs.
    pub sub_diffs: Vec<Box<Diff>>,
    /// The string report that may be printed.
    pub report: String,
}

impl Diff {
    /// Create a default diff to be built on.
    pub fn new(left: &str, right: &str) -> Diff {
        Diff {
            left: String::from(left),
            right: String::from(right),
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
pub fn differ(left: &str, right: &str) -> Diff {
    let left_meta = fs::metadata(left).expect("Left doesn't exist");
    let _right_meta = fs::metadata(right).expect("Right doesn't exist");

    if left_meta.is_dir() {
        return diff_directory(left, right);
    }
    else {
        // Check for specializations
        if left.ends_with(".nii.gz") || left.ends_with(".nii") {
            return diff_nii(left, right);
        }
        return diff_bytes(left, right);
    }
}


// TODO: clean this mess up
/// Calculate an abstract diff between two directories
pub fn diff_directory(left: &str, right: &str) -> Diff {
    // Obtain metadata
    let left_meta = fs::metadata(left).expect("Left dir didn't exist");
    let right_meta = fs::metadata(right).expect("Right dir didn't exist");

    // Check that both left and right are files
    if !(left_meta.is_dir()) {
        if !(right_meta.is_dir()) {
            panic!("Left and right are not dirs!")
        }
        else {
            panic!("Left is not a dir!")
        }
    }

    // Initialize the Diff object, since one may be computed
    let mut d = Diff::new(left, right);

    // Get PathBuf objects for the contents of left and right
    let left_contents = fs::read_dir(left).expect("Boo")
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, io::Error>>().expect("Boo");
    let right_contents = fs::read_dir(right).expect("Boo")
        .map(|res| res.map(|e| e.path()))
        .collect::<Result<Vec<_>, io::Error>>().expect("Boo");

    // Get the object names only to compare
    let left_onames: Vec<String> = left_contents.iter()
        .map(|o| String::from(o.file_name().unwrap().to_str().unwrap()))
        .collect();
    let right_onames: Vec<String> = right_contents.iter()
        .map(|o| String::from(o.file_name().unwrap().to_str().unwrap()))
        .collect();

    // This is inefficient, but we don't expect to deal with more than a
    // few hundred files per directory in this case
    // TODO: come up with a more efficient algorithm
    for x in left_onames.into_iter() {
        if right_onames.contains(&x) {
            d.common.push(String::from(x));
        }
        else {
            d.left_only.push(String::from(x));
        }
    }
    for x in right_onames.into_iter() {
        if !d.common.contains(&x) {
            d.right_only.push(String::from(x));
        }
    }

    // Iterate only over common files to perform diffs
    let mut diffs: Vec<Box<Diff>> = Vec::with_capacity(d.common.len());
    for f in d.common.iter() {
        diffs.push(Box::new(
            differ(
                Path::new(left).join(f).to_str().unwrap(),
                Path::new(right).join(f).to_str().unwrap()
            )
        ));
    }
    d.sub_diffs = diffs;

    // Determine if there is a match
    if d.left_only.len() == 0 && d.right_only.len() == 0 && 
        d.sub_diffs.iter().all(|a| a.matches) {
            // Match
            d.matches = true;
        }    
    else {
        // No match, build report
        let mut report = format!("{} vs. {}\n", left, right);
        if d.left_only.len() != 0 {
            report.push_str(&format!(
                    "Only in {}: {:#?}\n", left, d.left_only
            ));
        }
        if d.right_only.len() != 0 {
            report.push_str(&format!(
                "Only in {}: {:#?}\n", right, d.right_only
            ));
        }
        for subdiff in d.sub_diffs.iter() {
            if !subdiff.matches {
                report.push_str(&format!("{}\n", subdiff.report));
            }
        }
        d.report = report
    }


    return d;
}


/// Perform a diff on two files of unknown or binary encoding.
pub fn diff_bytes(left: &str, right: &str) -> Diff {
    // Obtain metadata
    let left_meta = fs::metadata(left).expect("Left file didn't exist");
    let right_meta = fs::metadata(right).expect("Right file didn't exist");

    // Check that both left and right are files
    if !(left_meta.is_file()) {
        if !(right_meta.is_file()) {
            panic!("{} and {} are not files!", left, right)
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

/// Diff two niftis
pub fn diff_nii(left: &str, right: &str) -> Diff {
    const TOLERANCE: f32 = 1e-16;
    // Load headers
    let left_reader = ReaderOptions::new().read_file(left)
        .expect("Cannot read left file as nifti!");
    let right_reader = ReaderOptions::new().read_file(right)
        .expect("Cannot read right file as nifti!");

    // Since both files exist, make a new Diff object
    let mut d = Diff::new(left, right);
    // Check to see if shapes match
    let shapes_match = 
        left_reader.header().dim == right_reader.header().dim;
    if shapes_match {
        let t0 = time::SystemTime::now();
        // Read in entire files and see if they match
        let left_data = left_reader.into_volume().into_ndarray::<f32>()
            .expect("Cannot read left file as ndarray!");
        let t1 = time::SystemTime::now();
        let right_data = right_reader.into_volume().into_ndarray::<f32>()
            .expect("Cannot read right file as ndarray!");
        let t2 = time::SystemTime::now();
        // Iterate over all of left and right, seeing which voxels are
        // below the tolerance
        let total_matches = left_data.iter().zip(right_data.iter())
            .map(|(a, b)| ((a - b).abs() < TOLERANCE) as usize)
            .sum::<usize>();
        let t3 = time::SystemTime::now();
        // FIXME: delete
        println!(
            "Time to read left: {}\n\
            Time to read right: {}\n\
            Time to perform diff: {}\n",
            t1.duration_since(t0).expect("Time error").as_millis(),
            t2.duration_since(t1).expect("Time error").as_millis(),
            t3.duration_since(t2).expect("Time error").as_millis(),
        );
        let total_voxels: usize = left_data.shape().iter().product();
        // See if we match
        if total_voxels == total_matches {
            // Complete match
            d.matches = true
        }
        else {
            // We can build a report
            let percentage_match =
                total_matches as f32 / total_voxels as f32 * 100.0;
            d.additional_info = format!(
                "Voxels diverge: {} of {} match ({:04.2}%)",
                total_matches,
                total_voxels,
                percentage_match
            );
        }
    }
    else {
        // We can build a report for shape mismatch
        d.additional_info = format!(
            "Shapes diverge: {:#?} vs. {:#?}",
            left_reader.header().dim().expect("Bad dimensions"),
            right_reader.header().dim().expect("Bad dimensions"),
        );
    }

    // Build report
    if !d.matches {
        d.report = format!(
            "{} vs. {}: {}", left, right, d.additional_info
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
