

//use std::collections::HashMap;
use std::str;
use std::path::Path;
use std::io::Result;
use memmap::{Mmap, Protection};
use dicom_types::DicomObject;


// pub fn read_preamble<'a>(std::io::Bufread &rd) -> &str<'a> {
    
// }


// pub fn read_partial<'a>(std::io::Bufread &rd) -> &str<'a> { 
    
// }


pub fn parse<'a, P>(path: P) -> Result<DicomObject> where P : AsRef<Path> {
    let pre_off = 0x80;
    let file_mmap = Mmap::open_path(path, Protection::Read).expect("mmap fail");
    let bytes: &[u8] = unsafe { file_mmap.as_slice() };
    
    if str::from_utf8(&bytes[pre_off..pre_off+4]).unwrap() != "DICM" { panic!("bad magic in header"); }

    Ok(DicomObject {})
}
