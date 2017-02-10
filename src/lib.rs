extern crate memmap;
extern crate flate2;
extern crate byteorder;

use byteorder::{ReadBytesExt, BigEndian, LittleEndian};
use std::io::Cursor;
use std::collections::HashMap;
use std::sync::{Once, ONCE_INIT};
use std::marker::PhantomData;
use std::path::Path;
use std::str;
use std::mem;
mod dicom_types;
use dicom_types::{DicomDict, DicomObjectDict, DicomDictElt, DicomElt, DicomKeywordDict, SeqItem};
mod dicom_dict;
use dicom_dict::dicom_dictionary_init;
use dicom_types::DicomObject;
use std::io::Result;
use memmap::{Mmap, Protection};

//mod filereader;

struct DicomLib<'a> {
    dict: DicomDict<'a>,
}

const EXTRA_LENGTH_VRS:[&'static str; 6] = ["OB", "OW", "OF", "SQ", "UN", "UT"];
const VR_NAMES:[&'static str; 27] = [ "AE","AS","AT","CS","DA","DS","DT","FL","FD","IS","LO","LT","OB","OF",
       "OW","PN","SH","SL","SQ","SS","ST","TM","UI","UL","UN","US","UT" ];

fn u8tou16(bytes: &[u8]) -> u16 { let val: &u16 = unsafe { mem::transmute(bytes.as_ptr())}; *val  }
fn u8stou16s<'a>(bytes: &[u8]) -> &'a [u16] { unsafe { mem::transmute(bytes) } }
fn u8tou32(bytes: &[u8]) -> u32 { let val: &u32 = unsafe { mem::transmute(bytes.as_ptr()) }; *val }
fn u8tostr(bytes: &[u8]) -> &str { str::from_utf8(bytes).unwrap() }
fn u16tou32(bytes: &[u16]) -> u32 { let val: &u32 = unsafe { mem::transmute(bytes.as_ptr()) }; *val }

fn isodd(x : usize) -> bool { x % 2 == 1 }


fn always_implicit(grp: u16, elt: u16) -> bool {
    grp == 0xFFFE && (elt == 0xE0DD || elt == 0xE000 || elt == 0xE00D)
}

fn pixeldata_parse<'a, 'b, 'c>(data: &'a [u8], sz: usize, vr: &str, elementsopt: Option<&DicomObjectDict<'b>>) -> (DicomElt<'c>, usize) {
    let (xr, wsize) = if vr == "OB" {(sz, 1)} else { (sz/2, 2) };

    let (xr, yr, zr) = match elementsopt {
        Some(elements) => {
            let (xa, ya, za) = (0x00280010, 0x00280011, 0x00280012);
            let xr = match elements.get(&xa) {
                Some(&DicomElt::UInt16(val)) => val as usize,
                Some(_) | None => xr,
            };
            let yr = match elements.get(&ya) {
                Some(&DicomElt::UInt16(val)) => val as usize,
                Some(_) | None => 1 as usize,
            };
            let zr = match elements.get(&za) {
                Some(&DicomElt::UInt16(val)) => val as usize,
                Some(_) | None => 1 as usize,
            };
            (xr, yr, zr)
        },
        None => (xr, 1 as usize, 1 as usize),
    };
    if yr != 1 || zr != 1 {panic!("don't yet support > 1D pixel arrays")}
    let (result, newoff) = if sz != 0xffffffff {
        let dp : &[u8]= &data[0..sz];
        let v = match wsize {
            2 => {
                let dp16 : &[u16] = u8stou16s(dp);
                let mut resvec16 : Vec<u16> = Vec::new();
                resvec16.extend_from_slice(dp16);
                //let resvec16 = vec![].extend(u8stou16s(&data[0..sz]).iter().cloned());
                DicomElt::UInt16s(resvec16)
            },
            1 => {
                let mut resvec8 : Vec<u8> = Vec::new();
                resvec8.extend_from_slice(dp);
                DicomElt::Bytes(resvec8)
            },
            _ => panic!("bad wsize"),

        };
        (v, sz)
    } else {
        let mut off = 0;
        let mut resvec8 = Vec::new();
        let mut resvec16 = Vec::new();
        loop {
            let (grp, elt) = (u8tou16(&data[off..off+2]), u8tou16(&data[off+2..off+4]));
            let xr = u8tou32(&data[off+4..off+8]) as usize;
            off += 8;
            if grp == 0xFFFE && elt == 0xE0DD { break; }
            if grp != 0xFFFE || elt != 0xE000 { panic!("dicom: expected item tag in encapsulated pixel data"); }
            let dp = &data[off..off+xr];
            let val = match wsize {
                2 => resvec16.extend_from_slice(u8stou16s(dp)),
                1 => resvec8.extend_from_slice(dp),
                _ => panic!("bad wsize"),
            };
            off += xr;
        };
        match wsize {
            2 => (DicomElt::UInt16s(resvec16), off),
            1 => (DicomElt::Bytes(resvec8), off),
            _ => panic!("bad wsize"),
        }
    };
    (result, newoff)
}

fn sequence_item<'a>(dict: &DicomDict<'a>, bytes : &'a [u8], off : &mut usize, evr: bool, sz : usize, items : &mut Vec<DicomElt<'a>>) {

    while *off < sz {
        let (gelt, off,  elt) = element(dict, bytes, off, evr, None);
        if gelt == (0xFFFE, 0xE00D) {break}
        items.push(elt);
    }
}

fn undefined_length(data : &[u8]) -> (usize, Vec<u16>) {
    let mut v = Vec::new();
    let (mut w1, mut w2) = (0, 0);
    let mut off = 0;
    loop {
        w1 = w2;
        w2 = u8tou16(&data[off..off+2]);
        off += 2;
        if w1 == 0xFFFE {
            if w2 == 0xE0DD { break }
            v.push(w1);
        }
        if w2 != 0xFFFE { v.push(w2); }
    }
    off += 4;
    (off, v)
}

fn sequence_parse<'a>(dict: &DicomDict<'a>, data : &'a [u8], evr: bool) -> (usize, DicomElt<'a>) {
    let mut sq  = Vec::new();
    let mut off = 0;
    let len = data.len();
    while off < len {
        let (grp, elt) = (u8tou16(&data[off..off+2]), u8tou16(&data[off+2..off+4]));
        let itemlen = u8tou32(&data[off+4..off+8]) as usize;
        off += 8;
        if grp == 0xFFFE && elt == 0xE0DD { break }
        if grp != 0xFFFE && elt != 0xE000 { panic!("dicom: expected item tag in sequence") }
        sequence_item(dict, data, &mut off, evr, itemlen, &mut sq);
    }
    (off, DicomElt::Seq(sq))
}

fn numeric_parse<'a>(mut c : Cursor<&[u8]>, elt : DicomElt<'a>, count : usize) -> DicomElt<'a> {
    let mut uv = Vec::new();
    let mut f32v = Vec::new();
    let mut f64v = Vec::new();
    for _ in 0..count {
        match elt {
            DicomElt::UInt16s(_) => uv.push(c.read_u16::<BigEndian>().unwrap()),
            DicomElt::Float32s(_) => f32v.push(c.read_f32::<BigEndian>().unwrap()),
            DicomElt::Float64s(_) => f64v.push(c.read_f64::<BigEndian>().unwrap()),
            _ => panic!("bad type: {:?}", elt)
        }
    }
    match elt {
        DicomElt::UInt16s(_) => DicomElt::UInt16s(uv),
        DicomElt::Float32s(_) => DicomElt::Float32s(f32v),
        DicomElt::Float64s(_) => DicomElt::Float64s(f64v),
        _ => panic!("bad type: {:?}", elt)
    }
}

fn lookup_vr<'a>(dict: &DicomDict<'a>, gelt: (u16, u16)) -> Option<&'a str> {
    let key = if gelt.0 & 0xff00 == 0x5000 {
        [0x5000, gelt.1]
    } else if gelt.0 & 0xff00 == 0x6000 {
        [0x6000, gelt.1]
    } else {
        [gelt.0, gelt.1]
    };
    let vr_key = u16tou32(&key);
    let result = dict.get(&vr_key);
    match result {
        Some(elt) => Some(elt.vr),
        None => return None,
    }
}

fn element<'a,  'c>(dict: &DicomDict<'a>, data: &'a [u8], start: &mut usize, evr: bool, elements: Option<&DicomObjectDict<'c>>)
               -> ((u16, u16), usize, DicomElt<'a>) {
    let mut off = *start;
    let (grp, elt) = (u8tou16(&data[off..off+2]), u8tou16(&data[off+2..off+4]));

    off += 4;
    let gelt = (grp, elt);
    let (mut vr, lenbytes, diffvr) = if evr && !always_implicit(grp, elt) {
        let vr = u8tostr(&data[off..off+2]);
        let lenbytes = if EXTRA_LENGTH_VRS.contains(&vr) { off += 2; 4} else { 2 };
        let diffvr = match lookup_vr(dict, gelt) {
            Some(newvr) => newvr == vr,
            None => false
        };
        (vr, lenbytes, diffvr)
    } else {
        let vr = match lookup_vr(dict, gelt) {
            Some(vr) => vr,
            None => panic!("bad vr"),
        };
        (vr, 4, false)
    };

    if isodd(grp as usize) && grp > 0x0008 && 0x0010 <= elt && elt < 0x00FF {
        vr = "LO";
    } else if isodd(grp as usize) && grp > 0x0008 {
        vr = "UN";
    }
    let mut sz = if lenbytes == 4 {
        u8tou32(&data[off..off+4]) as usize }
    else {
        u8tou16(&data[off..off+2]) as usize
    };
    off += lenbytes;
    let end = off + sz;
    let entry = if sz == 0 || vr == "XX" {
        DicomElt::Empty
    } else if sz == 0xffffffff {
        let (len, v) = undefined_length(&data[off..]);
        sz = len;
        DicomElt::UInt16s(v)
    } else if gelt == (0x7FE0, 0x0010) {
        let (elt, len) = pixeldata_parse(&data[off..], sz, vr, elements);
        sz = len;
        elt
    } else {
        let mut r = Cursor::new(&data[off..off+sz]);
        match vr {
            "AT" => DicomElt::UInt16s(vec![r.read_u16::<LittleEndian>().unwrap(),
                                           r.read_u16::<LittleEndian>().unwrap()]),
            "AE" | "AS" | "CS" | "DA" | "DT" | "LO" | "PN" | "SH" | "TM" | "UI" =>
                DicomElt::String(u8tostr(&data[off..off+sz]).clone()),
            "DS" => panic!("VR DS unimplemented"),
            "IS" => panic!("VR IS unimplemented"),
            "ST" | "LT" | "UT" => DicomElt::String(u8tostr(&data[off..off+sz]).clone()),
            "FL" => DicomElt::Float32(r.read_f32::<LittleEndian>().unwrap()),
            "FD" => DicomElt::Float64(r.read_f64::<LittleEndian>().unwrap()),
            "SL" => DicomElt::Int32(r.read_i32::<LittleEndian>().unwrap()),
            "SS" => DicomElt::Int16(r.read_i16::<LittleEndian>().unwrap()),
            "UL" => DicomElt::UInt32(r.read_u32::<LittleEndian>().unwrap()),
            "US" => DicomElt::UInt16(r.read_u16::<LittleEndian>().unwrap()),
            "OB" => { let mut v = Vec::new(); v.extend_from_slice(&data[off..end]); DicomElt::Bytes(v)},
            "OD" => numeric_parse(r, DicomElt::Float64s(vec![]), sz/8),
            "OF" => numeric_parse(r, DicomElt::Float32s(vec![]), sz/4),
            "OW" => numeric_parse(r, DicomElt::UInt16s(vec![]), sz/2),
            "SQ" => {let (newoff, newelt) = sequence_parse(dict, &data[off..end], evr);
                     assert!(newoff <= sz); sz -= sz - newoff; newelt} ,
             _ => panic!("bad vr: {}", vr),
        }
    };
    off += sz as usize;
    (gelt, off, entry)
}

fn read_dataset<'a>(dict: &'a DicomDict, data: &'a [u8], start: usize) -> Result<DicomObject<'a>> {
    let mut off = start;
    let sig = u8tostr(&data[off+4..off+6]);
    let evr = VR_NAMES.contains(&sig);
    let mut elements : DicomObjectDict = HashMap::new();
    let state : DicomKeywordDict = HashMap::new();
    while off < data.len() - 2 {
        let (gelt, off, elt) = element(dict, data, &mut off, evr, Some(&elements));
        let tag = u16tou32(&[gelt.0, gelt.1] );
        elements.insert(tag, elt);
    }
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
