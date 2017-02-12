extern crate memmap;
extern crate flate2;
extern crate byteorder;

#[macro_use]
extern crate serde_derive;

extern crate bincode;
extern crate serde;

mod dicom_types;
use dicom_types::{DicomObject, DicomDict};
mod dicom_dict;
use dicom_dict::dicom_dictionary_init;
mod dataset;
use dataset::read_dataset;

use std::path::Path;
use std::fs;
use memmap::{Mmap, Protection};
use std::io::Result;
use std::str;

pub struct DicomLib<'a> {
    dict: DicomDict<'a>,
}

impl<'a> DicomLib<'a> {
    pub fn new() -> Self {
        DicomLib { dict : dicom_dictionary_init() }
    }

    pub fn parse<P>(&self, path: P) -> Result<DicomObject> where P : AsRef<Path> {
        let mut off = 0x80;
        let file_mmap = Mmap::open_path(path, Protection::Read)?;
        let data: &[u8] = unsafe { file_mmap.as_slice() };
        let magic = str::from_utf8(&data[off..off+4]).unwrap();

        if magic != "DICM" { panic!("bad magic in header"); };
        off += 4;
        read_dataset(&self.dict, data, off)
    }

    pub fn parse_set<P>(&self, set: P) -> Result<Vec<DicomObject>> where P : AsRef<Path> {
        let set = set.as_ref();
        let mut v = Vec::new();
        for entry in fs::read_dir(set)? {
            let entry = entry?;
            let path = entry.path();
            if !path.to_str().unwrap().contains(".dcm") { continue;}
            v.push(self.parse(path)?);
        }
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::prelude::*;
    use std::fs::File;
    use bincode::SizeLimit;
    use bincode::{serialize, deserialize};

    #[test]
    fn parse_works() {
        let dlib = DicomLib::new();
        let result = (dlib.parse("resources/000001.dcm")).unwrap();
        //let result = result?;
        for (k, v) in result.keydict.iter() {
            if *k != "PixelData" {
                println!("key: {} val: {:?}", k, v);
            };
        }
    }

    #[test]
    fn parse_set_works() {
        let dlib = DicomLib::new();
        let result = dlib.parse_set("resources/LIDC").unwrap();
    }

    #[test]
    fn parse_set_serde() {
        let dlib = DicomLib::new();
        let result = dlib.parse_set("resources/LIDC").unwrap();
        let limit = SizeLimit::Infinite;
        let encoded : Vec<u8> = serialize(&result, limit).unwrap();
        let decoded : Vec<DicomObject> = deserialize(&encoded[..]).unwrap();
        assert_eq!(result, decoded);
        {
            let mut buffer = File::create("dicom.rsbin").unwrap();
            buffer.write(&encoded);
        }
        let mut buffer = File::open("dicom.rsbin").unwrap();
        let mut encoded2 = Vec::new();
        buffer.read_to_end(&mut encoded2);
        let decoded2 : Vec<DicomObject> = deserialize(&encoded2[..]).unwrap();
        assert_eq!(result, decoded2);
    }
}
