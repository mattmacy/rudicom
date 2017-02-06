

//use std::collections::HashMap;
use std::mem;
use std::str;
use std::path::Path;
use std::io::Result;
use memmap::{Mmap, Protection};
use dicom_types::DicomObject;


// pub fn read_preamble<'a>(std::io::Bufread &rd) -> &str<'a> {
    
// }


// pub fn read_partial<'a>(std::io::Bufread &rd) -> &str<'a> { 
    
// }
#[derive(Debug)]
#[repr(C, packed)]
struct MetaInfo {
    group: u16,
    elem: u16,
    vr: [u8; 2],
    length: u16,
}
const EXTRA_LENGTH_VRS:[&'static str; 6] = ["OB", "OW", "OF", "SQ", "UN", "UT"];
const TEXT_VRS:[&'static str; 6] = ["SH", "LO", "ST", "LT",  "UR", "UT"];  // and PN, but it is handled separately.

pub fn parse<'a, P>(path: P) -> Result<DicomObject> where P : AsRef<Path> {
    let mut off = 0x80;
    let file_mmap = Mmap::open_path(path, Protection::Read).expect("mmap fail");
    let data: &[u8] = unsafe { file_mmap.as_slice() };
    let magic = str::from_utf8(&data[off..off+4]).unwrap();
    
    if magic != "DICM" { panic!("bad magic in header"); };
    off += 4;
    let mi : &MetaInfo = unsafe {
        mem::transmute::<*const u8, &MetaInfo>(data[off..off+8].as_ptr())
    };
    let vr = str::from_utf8(&mi.vr[0..2]).unwrap();
    println!("magic: {} mi: {:?} vr: {} invrs: {}", magic, mi, vr, EXTRA_LENGTH_VRS.contains(&vr));

    Ok(DicomObject {})
}
