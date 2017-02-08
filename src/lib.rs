extern crate memmap;
extern crate flate2;

use std::collections::HashMap;
use std::sync::{Once, ONCE_INIT};
use std::marker::PhantomData;
use std::path::Path;
use std::str;
use std::mem;
mod dicom_types;
use dicom_types::{DicomDict, DicomObjectDict, DicomDictElt, DicomElt, DicomKeywordDict};
mod dicom_dict;
use dicom_dict::dicom_dictionary_init;
use dicom_types::DicomObject;
use std::io::Result;
use memmap::{Mmap, Protection};

//mod filereader;

struct DicomLib<'a> {
    dict: DicomDict<'a>,
}

const VR_NAMES:[&'static str; 27] = [ "AE","AS","AT","CS","DA","DS","DT","FL","FD","IS","LO","LT","OB","OF",
       "OW","PN","SH","SL","SQ","SS","ST","TM","UI","UL","UN","US","UT" ];

fn tou16(bytes: &[u8]) -> &u16 { unsafe { mem::transmute(bytes.as_ptr()) } }
fn tou32(bytes: &[u8]) -> &u32 { unsafe { mem::transmute(bytes.as_ptr()) } }
fn tostr(bytes: &[u8]) -> &str { str::from_utf8(bytes).unwrap() }
fn isodd(x : usize) -> bool { x % 2 == 1 }


fn element<'a>(data: &[u8], start: usize, evr: bool, elements: &HashMap<u32, DicomElt>) ->
    (u32, DicomElt) {
        let mut off = start;
        let (grp, elt, gelt) = (tou16(&data[off..off+2]), tou16(&data[off+2..off+4]),
                                tou32(&data[off..off+4]));
        off += 4;
        let entry = DicomElt::Float64(1.0);
        (*gelt, entry)
}

fn read_dataset<'a>(dict: &DicomDict, data: &[u8], start: usize) -> Result<DicomObject<'a>> {
    let mut off = start;
    let sig = tostr(&data[off+4..off+6]);
    let evr = VR_NAMES.contains(&sig);
    let elements : DicomObjectDict = HashMap::new();
    let state : DicomKeywordDict = HashMap::new();
    /*
    while off < data.len() - 2 {
        let (tag, elt) = element(data, &mut off, evr, &elements);
        elements.insert(tag, elt);
    }
     */
    Ok(DicomObject {odict : elements, keydict : state } )
}

impl<'a> DicomLib<'a> {
    fn new() -> Self {
        DicomLib { dict : dicom_dictionary_init() }

    }
    fn parse<P>(&self, path: P) -> Result<DicomObject> where P : AsRef<Path> {
        let mut off = 0x80;
        let file_mmap = Mmap::open_path(path, Protection::Read).expect("mmap fail");
        let data: &[u8] = unsafe { file_mmap.as_slice() };
        let magic = str::from_utf8(&data[off..off+4]).unwrap();

        if magic != "DICM" { panic!("bad magic in header"); };
        off += 4;
        read_dataset(&self.dict, data, off)
    }

}


#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_works() {
        let dlib = DicomLib::new();
        let result = dlib.parse("resources/000001.dcm");
    }
}
