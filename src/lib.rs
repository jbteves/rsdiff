/// Library for rsdiff

// ----------
// Public API
// ----------

use std::{
    fs::{self, File},
    io::{self, BufRead, BufReader, Cursor, prelude::*},
    convert::TryInto,
    path::Path,
    time,
};

use nifti::{NiftiObject, ReaderOptions};
use byteorder::{LittleEndian, ReadBytesExt};
use flate2::read::GzDecoder;
use colored::*;

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
            report.push_str(&format!("{}",
                    format!("Only in {}: {}\n", left, d.left_only.join(", ")
            ).bright_red()));
        }
        if d.right_only.len() != 0 {
            report.push_str(&format!("{}",
                format!("Only in {}: {}\n", right, d.right_only.join(", "))
            .bright_green()));
        }
        // Band cyan and magenta for easy reading
        let mut counts = 0;
        for subdiff in d.sub_diffs.iter() {
            if !subdiff.matches {
                counts += 1;
                if counts % 2 == 0 {
                    // Bright cyan
                    report.push_str(&format!("{}",
                    format!("{}\n", subdiff.report).bright_cyan()));
                }
                else {
                    // Not-bright cyan
                    report.push_str(&format!("{}",
                    format!("{}\n", subdiff.report).bright_magenta()));
                }
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

pub fn diff_transmute_buffers_f32(left: &[u8], right: &[u8], tolerance: f32 ) -> usize {
    // Verify arrays match in size
    if !(left.len() == right.len()) {
        panic!("Buffers supplied to rsdiff::diff_buffer must have the \
               same length! Instead, left is size {} and right is size {}",
               left.len(), right.len());
    }
    // Iterate and compare bytes
    let mut matches: usize = 0;
    let mut left_rdr = Cursor::new(left);
    let mut right_rdr = Cursor::new(right);
    while let Ok(a) = left_rdr.read_f32::<LittleEndian>() {
        if let Ok(b) = right_rdr.read_f32::<LittleEndian>() {
            matches += ((a - b).abs() < tolerance) as usize;
        }
        else {
            panic!("Catastrophic buffer mismatch failure");
        }
    }
    return matches
}

pub fn diff_transmute_buffers_f64(left: &[u8], right: &[u8], tolerance: f64 ) -> usize {
    // Verify arrays match in size
    if !(left.len() == right.len()) {
        panic!("Buffers supplied to rsdiff::diff_buffer must have the \
               same length! Instead, left is size {} and right is size {}",
               left.len(), right.len());
    }
    // Iterate and compare bytes
    let mut matches: usize = 0;
    let mut left_rdr = Cursor::new(left);
    let mut right_rdr = Cursor::new(right);
    while let Ok(a) = left_rdr.read_f64::<LittleEndian>() {
        if let Ok(b) = right_rdr.read_f64::<LittleEndian>() {
            matches += ((a - b).abs() < tolerance) as usize;
        }
        else {
            panic!("Catastrophic buffer mismatch failure");
        }
    }
    return matches
}

pub fn diff_transmute_buffers_u16(left: &[u8], right: &[u8]) -> usize {
    // Verify arrays match in size
    if !(left.len() == right.len()) {
        panic!("Buffers supplied to rsdiff::diff_buffer must have the \
               same length! Instead, left is size {} and right is size {}",
               left.len(), right.len());
    }
    // Iterate and compare bytes
    let mut matches: usize = 0;
    let mut left_rdr = Cursor::new(left);
    let mut right_rdr = Cursor::new(right);
    while let Ok(a) = left_rdr.read_u16::<LittleEndian>() {
        if let Ok(b) = right_rdr.read_u16::<LittleEndian>() {
            matches += (a == b) as usize;
        }
        else {
            panic!("Catastrophic buffer mismatch failure");
        }
    }
    return matches
}

pub fn diff_transmute_buffers_u32(left: &[u8], right: &[u8]) -> usize {
    // Verify arrays match in size
    if !(left.len() == right.len()) {
        panic!("Buffers supplied to rsdiff::diff_buffer must have the \
               same length! Instead, left is size {} and right is size {}",
               left.len(), right.len());
    }
    // Iterate and compare bytes
    let mut matches: usize = 0;
    let mut left_rdr = Cursor::new(left);
    let mut right_rdr = Cursor::new(right);
    while let Ok(a) = left_rdr.read_u32::<LittleEndian>() {
        if let Ok(b) = right_rdr.read_u32::<LittleEndian>() {
            matches += (a == b) as usize;
        }
        else {
            panic!("Catastrophic buffer mismatch failure");
        }
    }
    return matches
}

pub fn diff_transmute_buffers_i16(left: &[u8], right: &[u8]) -> usize {
    // Verify arrays match in size
    if !(left.len() == right.len()) {
        panic!("Buffers supplied to rsdiff::diff_buffer must have the \
               same length! Instead, left is size {} and right is size {}",
               left.len(), right.len());
    }
    // Iterate and compare bytes
    let mut matches: usize = 0;
    let mut left_rdr = Cursor::new(left);
    let mut right_rdr = Cursor::new(right);
    while let Ok(a) = left_rdr.read_i16::<LittleEndian>() {
        if let Ok(b) = right_rdr.read_i16::<LittleEndian>() {
            matches += (a == b) as usize;
        }
        else {
            panic!("Catastrophic buffer mismatch failure");
        }
    }
    return matches
}

pub fn diff_transmute_buffers_i32(left: &[u8], right: &[u8]) -> usize {
    // Verify arrays match in size
    if !(left.len() == right.len()) {
        panic!("Buffers supplied to rsdiff::diff_buffer must have the \
               same length! Instead, left is size {} and right is size {}",
               left.len(), right.len());
    }
    // Iterate and compare bytes
    let mut matches: usize = 0;
    let mut left_rdr = Cursor::new(left);
    let mut right_rdr = Cursor::new(right);
    while let Ok(a) = left_rdr.read_i32::<LittleEndian>() {
        if let Ok(b) = right_rdr.read_i32::<LittleEndian>() {
            matches += (a == b) as usize;
        }
        else {
            panic!("Catastrophic buffer mismatch failure");
        }
    }
    return matches
}

pub fn diff_transmute_buffers_i64(left: &[u8], right: &[u8]) -> usize {
    // Verify arrays match in size
    if !(left.len() == right.len()) {
        panic!("Buffers supplied to rsdiff::diff_buffer must have the \
               same length! Instead, left is size {} and right is size {}",
               left.len(), right.len());
    }
    // Iterate and compare bytes
    let mut matches: usize = 0;
    let mut left_rdr = Cursor::new(left);
    let mut right_rdr = Cursor::new(right);
    while let Ok(a) = left_rdr.read_i64::<LittleEndian>() {
        if let Ok(b) = right_rdr.read_i64::<LittleEndian>() {
            matches += (a == b) as usize;
        }
        else {
            panic!("Catastrophic buffer mismatch failure");
        }
    }
    return matches
}

pub fn diff_transmute_buffers_u64(left: &[u8], right: &[u8]) -> usize {
    // Verify arrays match in size
    if !(left.len() == right.len()) {
        panic!("Buffers supplied to rsdiff::diff_buffer must have the \
               same length! Instead, left is size {} and right is size {}",
               left.len(), right.len());
    }
    // Iterate and compare bytes
    let mut matches: usize = 0;
    let mut left_rdr = Cursor::new(left);
    let mut right_rdr = Cursor::new(right);
    while let Ok(a) = left_rdr.read_u64::<LittleEndian>() {
        if let Ok(b) = right_rdr.read_u64::<LittleEndian>() {
            matches += (a == b) as usize;
        }
        else {
            panic!("Catastrophic buffer mismatch failure");
        }
    }
    return matches
}

fn diff_voxels_nii_gz(left: &str, right: &str, vox_offset: usize, buffer_differ: fn(&[u8], &[u8]) -> usize) -> usize {
    const KILOBYTE: usize = 1024;
    const CHUNK_SIZE: usize = 256 * KILOBYTE;
    const TOLERANCE: f32 = 1e-16;
    let left_file = File::open(left).expect("Uh-oh!");
    let right_file = File::open(right).expect("Uh-oh!");
    let mut left_buffer = [0u8; CHUNK_SIZE];
    let mut right_buffer = [0u8; CHUNK_SIZE];
    let mut place_holder_buffer = Vec::with_capacity(vox_offset);
    let mut left_gz = GzDecoder::new(left_file);
    let mut right_gz = GzDecoder::new(right_file);
    // Clear out offsets
    let _offset_left = left_gz.read_exact(&mut place_holder_buffer)
        .expect("I can't read the GZ file!");
    let _offset_right = right_gz.read_exact(&mut place_holder_buffer)
        .expect("I can't read the right GZ file!");
    let mut total_matches = 0;
    // Loop and compare
    loop {
        if let Ok(nl) = left_gz.read(&mut left_buffer) {
            if let Ok(nr) = right_gz.read(&mut right_buffer) {
                if nl != nr {
                    panic!("Unexpected file size difference! \
                    {} reads {}, {} reads {}!",
                    left, nl,
                    right, nr
                    );
                }
                if nl == 0 {
                    break;
                }
                total_matches += buffer_differ(&left_buffer[..nl], &right_buffer[..nl]);
            }
            else {
                panic!("Can't read from right buffer!");
            }
        }
        else {
            panic!("Can't read from left buffer!");
        }
    }
    total_matches
}

fn diff_voxels_nii(left: &str, right: &str, vox_offset: usize, buffer_differ: fn(&[u8], &[u8]) -> usize) -> usize {
    const KILOBYTE: usize = 1024;
    const CHUNK_SIZE: usize = 256 * KILOBYTE;
    const TOLERANCE: f32 = 1e-16;
    let left_file = File::open(left).expect("Uh-oh!");
    let right_file = File::open(right).expect("Uh-oh!");

    let mut left_rdr = BufReader::with_capacity(
        CHUNK_SIZE, left_file
    );
    let mut right_rdr = BufReader::with_capacity(
        CHUNK_SIZE, right_file
    );
    let mut total_matches = 0;
    // Consume the appropriate voxel offset
    left_rdr.consume(vox_offset);
    right_rdr.consume(vox_offset);
    loop {
        let length = {
            let left_buffer = left_rdr.fill_buf().expect("UO");
            let right_buffer = right_rdr.fill_buf().expect("UO");
            if left_buffer.len() !=  0 {
                total_matches += buffer_differ(&left_buffer, &right_buffer);
            }
            left_buffer.len()
        };
        left_rdr.consume(length);
        right_rdr.consume(length);
        if length == 0 { break; }
    }
    total_matches
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
        // Check to see if data types match
        if left_reader.header().datatype != right_reader.header().datatype {
            d.report = format!("{} vs {}: Shapes match, types diverge \
                               ({:?} vs. {:?})",
                               left, right,
                               left_reader.header().datatype,
                               right_reader.header().datatype
                        );
            return d;
        }
        let hdr = left_reader.header();
        let dtype = hdr.datatype;
        let vox_offset = hdr.vox_offset as usize;
        // Build a function to run the correct buffer transmuter
        let buffer_differ = match dtype {
            4 => |a: &[u8], b: &[u8]| diff_transmute_buffers_i16(a, b),
            8 => |a: &[u8], b: &[u8]| diff_transmute_buffers_i32(a, b),
            16 => |a: &[u8], b: &[u8]| diff_transmute_buffers_f32(a, b, TOLERANCE),
            64 => |a: &[u8], b: &[u8]| diff_transmute_buffers_f64(a, b, TOLERANCE as f64),
            512 => |a: &[u8], b: &[u8]| diff_transmute_buffers_u16(a, b),
            768 => |a: &[u8], b: &[u8]| diff_transmute_buffers_u32(a, b),
            1024 => |a: &[u8], b: &[u8]| diff_transmute_buffers_i64(a, b),
            1280 => |a: &[u8], b: &[u8]| diff_transmute_buffers_i64(a, b),
            _ => panic!("Unsupported data type {}, sorry!", dtype),
        };
        let total_matches = {
            if left.ends_with("gz") {
                diff_voxels_nii_gz(left, right, vox_offset, buffer_differ)
            }
            else {
                diff_voxels_nii(left, right, vox_offset, buffer_differ)
            }
        };
        let mut total_voxels: usize = 1;
        for  d in left_reader.header().dim.iter() {
            let mut value = *d;
            if value == 0 {
                value = 1;
            }
            total_voxels *= value as usize;
        }
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
pub fn diff_buffer(left: &[u8], right: &[u8]) -> usize {
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
