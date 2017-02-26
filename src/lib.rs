extern crate memmap;
extern crate flate2;
extern crate byteorder;

#[macro_use]
extern crate serde_derive;

extern crate bincode;
extern crate serde;

mod dicom_types;
use dicom_types::{DicomSlice, DicomScan, DicomDict, DcmImg16, DicomElt};
mod dicom_dict;
use dicom_dict::dicom_dictionary_init;
mod dataset;
use dataset::read_dataset;

use std::path::Path;
use std::fs;
use memmap::{Mmap, Protection};
use std::io::{Error, ErrorKind, Result};
use std::str;

use std::fs::File;
use std::io::prelude::*;
use bincode::SizeLimit;
use bincode::{serialize, deserialize};


pub struct DicomLib<'a> {
    dict: DicomDict<'a>,
}

impl<'a> DicomLib<'a> {
    pub fn new() -> Self {
        DicomLib { dict : dicom_dictionary_init() }
    }

    pub fn parse<P>(&self, path: P) -> Result<DicomSlice> where P : AsRef<Path> {
        let mut off = 0x80;
        let file_mmap = Mmap::open_path(path, Protection::Read)?;
        let data: &[u8] = unsafe { file_mmap.as_slice() };
        let magic = str::from_utf8(&data[off..off+4]).unwrap();

        if magic != "DICM" { panic!("bad magic in header"); };
        off += 4;
        read_dataset(&self.dict, data, off)
    }

    pub fn parse_scan<P>(&self, set: P) -> Result<DicomScan> where P : AsRef<Path> {
        let set = set.as_ref();
        let mut v = Vec::new();
        for entry in fs::read_dir(set)? {
            let entry = entry?;
            let path = entry.path();
            if !path.to_str().unwrap().contains(".dcm") { continue;}
            v.push(self.parse(path)?);
        }
        v.sort_by(|a, b| a.pos().partial_cmp(&b.pos()).expect("Nan"));
        let pix_data = v[0].pixel_data().clone();
        let pix_len = pix_data.data.len();
        let scan_len = v.len();
        assert_eq!(pix_len, pix_data.xr*pix_data.yr);
        let mut ivec : Vec<i16> = Vec::with_capacity(scan_len*pix_len);
        for slice in v.iter_mut() {
            let pix_data = match slice.keydict.remove("PixelData") {
                Some(DicomElt::Image16(v)) => v,
                Some(_) | None => panic!("no PixelData")
            };
            ivec.extend_from_slice(&pix_data.data[0..]);
        };
        let image = DcmImg16 {xr: pix_data.xr, yr: pix_data.yr, zr: scan_len, data: ivec};
        Ok(DicomScan {slice_data: v, image: image})
    }

    pub fn get_pixels_hu(ref scan: DicomScan) -> Vec<i16> {
        let mut image : Vec<i16> = scan.image.data.clone();
        for v in image.iter_mut().filter(|x| **x == -2000) { *v = 0; };
        let increment = scan.image.xr*scan.image.yr;
        for i in 0..scan.slice_data.len() {
            let intercept = scan.slice_data[i].intercept();
            let slope = scan.slice_data[i].slope();
            let offset = i*increment;
            if slope != 1.0 {
                for v in image[offset..offset+increment].iter_mut() { *v = ((*v as f64) * slope) as i16;};
            }
            for v in image[offset..offset+increment].iter_mut() { *v += intercept;};
        }
        image
    }

    pub fn serialize_scan<P>(&self, path: P, set: DicomScan) -> Result<usize> where P : AsRef<Path> {
        let mut buffer = File::create(path)?;
        let limit = SizeLimit::Infinite;
        let encoded : Vec<u8> = serialize(&set, limit).expect("serialize fail");
        let size = buffer.write(&encoded)?;
        Ok(size)
    }

    pub fn deserialize_scan<P>(&self, path: P) -> Result<DicomScan> where P : AsRef<Path> {
        let mut buffer = File::open(path)?;
        let mut encoded = Vec::new();
        let size = buffer.read_to_end(&mut encoded)?;
        if size == 0 { return Err(Error::new(ErrorKind::UnexpectedEof, "Empty File")); };
        let decoded = deserialize(&encoded[..]).expect("deserialize fail");
        Ok(decoded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_works() {
        let dlib = DicomLib::new();
        let result = (dlib.parse("resources/000001.dcm")).unwrap();
        for (k, v) in result.keydict.iter() {
            if *k != "PixelData" {
                println!("{}: {:?}", k, v);
            };
        }
    }

    #[test]
    fn parse_set_works() {
        let dlib = DicomLib::new();
        let result = dlib.parse_scan("resources/LIDC").unwrap();
        let thickness = result[0].thickness();
        println!("thickness: {}", thickness);
    }

    #[test]
    fn parse_scan_serde() {
        let dlib = DicomLib::new();
        let result = dlib.parse_scan("resources/LIDC").unwrap();
        let limit = SizeLimit::Infinite;
        let encoded : Vec<u8> = serialize(&result, limit).unwrap();
        let decoded : Vec<DicomSlice> = deserialize(&encoded[..]).unwrap();
        assert_eq!(result, decoded);
        {
            let mut buffer = File::create("dicom.rsbin").unwrap();
            buffer.write(&encoded);
        }
        let mut buffer = File::open("dicom.rsbin").unwrap();
        let mut encoded2 = Vec::new();
        buffer.read_to_end(&mut encoded2);
        let decoded2 : Vec<DicomSlice> = deserialize(&encoded2[..]).unwrap();
        assert_eq!(result, decoded2);
    }
}
