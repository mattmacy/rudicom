extern crate memmap;
extern crate flate2;
extern crate byteorder;

use byteorder::{ReadBytesExt, WriteBytesExt, BigEndian, LittleEndian};
use std::io::Cursor;
use std::collections::HashMap;
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

enum endian {
    Big,
    Little
}

const EXTRA_LENGTH_VRS:[&'static str; 6] = ["OB", "OW", "OF", "SQ", "UN", "UT"];
const VR_NAMES:[&'static str; 27] = [ "AE","AS","AT","CS","DA","DS","DT","FL","FD","IS","LO","LT","OB","OF",
       "OW","PN","SH","SL","SQ","SS","ST","TM","UI","UL","UN","US","UT" ];

fn u8tou16(bytes: &[u8]) -> u16 { (bytes[1]  as u16) << 8 | bytes[0] as u16 }

fn u8tou32(bytes: &[u8]) -> u32 {
    (bytes[3] as u32) << 24 | (bytes[2] as u32) << 16 |
    (bytes[1] as u32) << 8 | bytes[0] as u32
}

fn u16tou32(bytes: &[u16]) -> u32 { (bytes[1]  as u32) << 16 | bytes[0] as u32 }

fn u8tostr(bytes: &[u8]) -> &str { str::from_utf8(bytes).unwrap() }

fn isodd(x : usize) -> bool { x % 2 == 1 }


fn always_implicit(grp: u16, elt: u16) -> bool {
    grp == 0xFFFE && (elt == 0xE0DD || elt == 0xE000 || elt == 0xE00D)
}

fn pixeldata_parse<'a>(data: &[u8], sz: usize, vr: &str, elementsopt: Option<&DicomObjectDict<'a>>) -> (DicomElt, usize) {
    let (xr, wsize) = if vr == "OB" {(sz, 1)} else { (sz/2, 2) };

    let (xr, yr, zr) = match elementsopt {
        Some(elements) => {
            let (xa, ya, za) = (0x00280010, 0x00280011, 0x00280012);
            let xr = match elements.get(&xa) {
                Some(&DicomElt::UInt16s(ref val)) => val[0] as usize,
                Some(_) | None => xr,
            };
            let yr = match elements.get(&ya) {
                Some(&DicomElt::UInt16s(ref val)) => val[0] as usize,
                Some(_) | None => 1 as usize,
            };
            let zr = match elements.get(&za) {
                Some(&DicomElt::UInt16s(ref val)) => val[0] as usize,
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
                let mut r = Cursor::new(dp);
                let mut resvec16 : Vec<u16> = Vec::new();
                for _ in 0..(sz/2) {
                    resvec16.push(r.read_u16::<LittleEndian>().unwrap())
                };
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
                2 =>  {
                    let mut r = Cursor::new(dp);
                    for _ in 0..(xr/2) {
                        resvec16.push(r.read_u16::<LittleEndian>().unwrap())
                    };
                }
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

fn sequence_item<'a>(dict: &DicomDict<'a>, bytes : &[u8], off : &mut usize, evr: bool, sz : usize, items : &mut Vec<DicomElt>) {

    while *off < sz {
        let (gelt, elt) = element(dict, bytes, off, evr, None);
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

fn sequence_parse<'a>(dict: &DicomDict<'a>, data : &[u8], evr: bool) -> (usize, DicomElt) {
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

fn numeric_parse_little<'a>(mut c : Cursor<&[u8]>, elt : DicomElt, count : usize) -> DicomElt {
    let (mut i32s, mut u32s, mut u16s, mut i16s, mut f32s, mut f64s);

    match elt {
        DicomElt::UInt16s(_) => {
            u16s = Vec::new();
            for _ in 0..count {u16s.push(c.read_u16::<LittleEndian>().unwrap())}
            DicomElt::UInt16s(u16s)
        },
        DicomElt::Int16s(_) => {
            i16s = Vec::new();
            for _ in 0..count {i16s.push(c.read_i16::<LittleEndian>().unwrap())}
            DicomElt::Int16s(i16s)
        },
        DicomElt::UInt32s(_) => {
            u32s = Vec::new();
            for _ in 0..count {u32s.push(c.read_u32::<LittleEndian>().unwrap())}
            DicomElt::UInt32s(u32s)
        },
        DicomElt::Int32s(_) => {
            i32s = Vec::new();
            for _ in 0..count {i32s.push(c.read_i32::<LittleEndian>().unwrap())}
            DicomElt::Int32s(i32s)
        },
        DicomElt::Float32s(_) => {
            f32s = Vec::new();
            for _ in 0..count {f32s.push(c.read_f32::<LittleEndian>().unwrap())}
            DicomElt::Float32s(f32s)
        },
        DicomElt::Float64s(_) => {
            f64s = Vec::new();
            for _ in 0..count {f64s.push(c.read_f64::<LittleEndian>().unwrap())}
            DicomElt::Float64s(f64s)
        },
        _ => panic!("bad juju"),
    }
}
fn numeric_parse_big<'a>(mut c : Cursor<&[u8]>, elt : DicomElt, count : usize) -> DicomElt {
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

fn string_parse(data: &[u8]) -> DicomElt {
    let dsstr = u8tostr(data);
    let vstr : Vec<&str> = dsstr.split('\\').collect();
    println!("vsstr: {:?}", vstr);
    let mut isf64 = false;
    {
        for &s in &vstr {
            if s.contains(".") || s.contains("-") {isf64 = true; break}
        }
    }
    if isf64 {
        let mut v : Vec<f64> = Vec::new();
        for &s in &vstr {
            v.push(s.trim().parse().unwrap());
        };
        DicomElt::Float64s(v)
    } else {
        let mut v : Vec<u32> = Vec::new();
        for &s in &vstr {
            v.push(s.trim().parse().unwrap());
        };
        DicomElt::UInt32s(v)
    }
}

fn numeric_parse(mut c : Cursor<&[u8]>, elt : DicomElt, count : usize, order: endian) -> DicomElt {
    match order {
        endian::Big => numeric_parse_big(c, elt, count),
        endian::Little => numeric_parse_little(c, elt, count),
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

fn element<'a>(dict: &DicomDict<'a>, data: &[u8], start: &mut usize, evr: bool, elements: Option<&DicomObjectDict<'a>>)
               -> ((u16, u16), DicomElt) {
    let mut off = *start;
    let (grp, elt) = (u8tou16(&data[off..off+2]), u8tou16(&data[off+2..off+4]));
    println!("grp: {} elt: {}", grp, elt);
    off += 4;
    let gelt = (grp, elt);
    let (mut vr, lenbytes, diffvr) = if evr && !always_implicit(grp, elt) {
        let vr = u8tostr(&data[off..off+2]);
        let lenbytes = if EXTRA_LENGTH_VRS.contains(&vr) { off += 4; 4} else { off += 2; 2 };
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
    println!("grp: {} elt: {} diffvr: {} vr: {} lenbytes: {} data[0]: {} data[1]: {}",
             grp, elt, diffvr, vr, lenbytes, data[off], data[off+1]);
    let mut sz = if lenbytes == 4 {
        u8tou32(&data[off..off+4]) as usize }
    else {
        let val = u8tou16(&data[off..off+2]) as usize;
        println!("data[0]: {} data[1]: {}, val: {}", data[off], data[off+1], val);
        val
    };
    off += lenbytes;
    let end = off + sz;
    println!("vr: {} off: {} sz: {}", vr, off, sz);
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
                DicomElt::String(u8tostr(&data[off..off+sz]).to_string()),
            "IS" | "DS" => string_parse(&data[off..off+sz]),
            "ST" | "LT" | "UT" => DicomElt::String(u8tostr(&data[off..off+sz]).to_string()),
            "FL" => numeric_parse(r, DicomElt::Float32s(vec![]), sz/8, endian::Little),
            "FD" => numeric_parse(r, DicomElt::Float64s(vec![]), sz/4, endian::Little),
            "SL" => numeric_parse(r, DicomElt::Int32s(vec![]), sz/4, endian::Little),
            "SS" => numeric_parse(r, DicomElt::Int16s(vec![]), sz/2, endian::Little),
            "UL" => numeric_parse(r, DicomElt::UInt32s(vec![]), sz/4, endian::Little),
            "US" => numeric_parse(r, DicomElt::UInt16s(vec![]), sz/2, endian::Little),
            "OB" | "UN" => { let mut v = Vec::new(); v.extend_from_slice(&data[off..end]); DicomElt::Bytes(v)},
            "OD" => numeric_parse(r, DicomElt::Float64s(vec![]), sz/8, endian::Big),
            "OF" => numeric_parse(r, DicomElt::Float32s(vec![]), sz/4, endian::Big),
            "OW" => numeric_parse(r, DicomElt::UInt16s(vec![]), sz/2, endian::Big),
            "SQ" => {let (newoff, newelt) = sequence_parse(dict, &data[off..end], evr);
                     assert!(newoff <= sz); sz -= sz - newoff; newelt} ,
             _ => panic!("bad vr: {}", vr),
        }
    };
    println!("sz: {}", sz);
    off += sz as usize;
    if isodd(sz) {off += 1;}
    *start = off;
    (gelt, entry)
}

fn read_dataset<'a>(dict: &DicomDict<'a>, data: &[u8], start: usize) -> Result<DicomObject<'a>> {
    let mut off = start;
    let sig = u8tostr(&data[off+4..off+6]);
    let evr = VR_NAMES.contains(&sig);
    let mut elements : DicomObjectDict = HashMap::new();
    let state : DicomKeywordDict = HashMap::new();
    println!("start: {}", off);
    while off < data.len() - 2 {
        let (gelt, elt) = element(dict, data, &mut off, evr, Some(&elements));
        let tag = u16tou32(&[gelt.0, gelt.1] );
        println!("tag: {} off: {}", tag, off);
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
