
use std::io::Result;
use byteorder::{ReadBytesExt, BigEndian, LittleEndian};
use std::io::Cursor;
use std::collections::HashMap;
use std::str;

use dicom_types::{DicomDict, DicomSlice, DicomGeltEltDict, DicomElt, DicomKwEltDict, DcmImg16, DcmImg8};

enum Endian {
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

fn pixeldata_parse<'a>(data: &[u8], sz: usize, vr: &str, elementsopt: Option<&DicomGeltEltDict>) -> (DicomElt, usize) {
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
    let (result, newoff) = if sz != 0xffffffff {
        let dp : &[u8]= &data[0..sz];
        let v = match wsize {
            2 => {
                let mut r = Cursor::new(dp);
                let mut resvec16 : Vec<i16> = Vec::new();
                for _ in 0..(sz/2) { resvec16.push(r.read_i16::<LittleEndian>().unwrap()); }
                DicomElt::Image16( DcmImg16 { xr : xr, yr : yr, zr : zr, data : resvec16 } )
            },
            1 => {
                DicomElt::Image8( DcmImg8 { xr : xr, yr : yr, zr : zr, data : dp.to_owned() } )
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
            match wsize {
                2 =>  {
                    let mut r = Cursor::new(dp);
                    for _ in 0..(xr/2) {
                        resvec16.push(r.read_i16::<LittleEndian>().unwrap())
                    };
                }
                1 => resvec8.extend_from_slice(dp),
                _ => panic!("bad wsize"),
            };
            off += xr;
        };
        match wsize {
            2 => (DicomElt::Image16( DcmImg16 { xr : xr, yr : yr, zr : zr, data : resvec16 } ), off),
            1 => (DicomElt::Image8( DcmImg8 { xr : xr, yr : yr, zr : zr, data : resvec8 } ), off) ,
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
    let (mut w1, mut w2, mut off);
    off = 0;
    w2 = 0;
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
    let mut v : Vec<f64> = Vec::new();
    for &s in &vstr {
        v.push(s.trim().parse().unwrap());
    };
    DicomElt::Float64s(v)
}

fn numeric_parse(c : Cursor<&[u8]>, elt : DicomElt, count : usize, order: Endian) -> DicomElt {
    match order {
        Endian::Big => numeric_parse_big(c, elt, count),
        Endian::Little => numeric_parse_little(c, elt, count),
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

fn element<'a>(dict: &DicomDict<'a>, data: &[u8], start: &mut usize, evr: bool, elements: Option<&DicomGeltEltDict>)
               -> ((u16, u16), DicomElt) {
    let mut off = *start;
    let (grp, elt) = (u8tou16(&data[off..off+2]), u8tou16(&data[off+2..off+4]));
    off += 4;
    let gelt = (grp, elt);
    let (mut vr, lenbytes) = if evr && !always_implicit(grp, elt) {
        let vr = u8tostr(&data[off..off+2]);
        let lenbytes = if EXTRA_LENGTH_VRS.contains(&vr) { off += 4; 4} else { off += 2; 2 };
        (vr, lenbytes)
    } else {
        let vr = match lookup_vr(dict, gelt) {
            Some(vr) => vr,
            None => panic!("bad vr"),
        };
        (vr, 4)
    };

    if isodd(grp as usize) && grp > 0x0008 && 0x0010 <= elt && elt < 0x00FF {
        vr = "LO";
    } else if isodd(grp as usize) && grp > 0x0008 {
        vr = "UN";
    }
//    println!("grp: {} elt: {} diffvr: {} vr: {} lenbytes: {} data[0]: {} data[1]: {}",
//             grp, elt, diffvr, vr, lenbytes, data[off], data[off+1]);
    let mut sz = if lenbytes == 4 {
        u8tou32(&data[off..off+4]) as usize }
    else {
        let val = u8tou16(&data[off..off+2]) as usize;
        //println!("data[0]: {} data[1]: {}, val: {}", data[off], data[off+1], val);
        val
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
                DicomElt::String(u8tostr(&data[off..off+sz]).to_string()),
            "IS" | "DS" => string_parse(&data[off..off+sz]),
            "ST" | "LT" | "UT" => DicomElt::String(u8tostr(&data[off..off+sz]).to_string()),
            "FL" => numeric_parse(r, DicomElt::Float32s(vec![]), sz/8, Endian::Little),
            "FD" => numeric_parse(r, DicomElt::Float64s(vec![]), sz/4, Endian::Little),
            "SL" => numeric_parse(r, DicomElt::Int32s(vec![]), sz/4, Endian::Little),
            "SS" => numeric_parse(r, DicomElt::Int16s(vec![]), sz/2, Endian::Little),
            "UL" => numeric_parse(r, DicomElt::UInt32s(vec![]), sz/4, Endian::Little),
            "US" => numeric_parse(r, DicomElt::UInt16s(vec![]), sz/2, Endian::Little),
            "OB" | "UN" => { DicomElt::Bytes(data[off..end].to_owned())},
            "OD" => numeric_parse(r, DicomElt::Float64s(vec![]), sz/8, Endian::Big),
            "OF" => numeric_parse(r, DicomElt::Float32s(vec![]), sz/4, Endian::Big),
            "OW" => numeric_parse(r, DicomElt::UInt16s(vec![]), sz/2, Endian::Big),
            "SQ" => {let (newoff, newelt) = sequence_parse(dict, &data[off..end], evr);
                     assert!(newoff <= sz); sz -= sz - newoff; newelt} ,
             _ => panic!("bad vr: {}", vr),
        }
    };
    off += sz as usize;
    if isodd(sz) {off += 1;}
    *start = off;
    (gelt, entry)
}

pub fn read_dataset<'a>(dict: &DicomDict<'a>, data: &[u8], start: usize) -> Result<DicomSlice> {
    let mut off = start;
    let sig = u8tostr(&data[off+4..off+6]);
    let evr = VR_NAMES.contains(&sig);
    let mut elements : DicomGeltEltDict = HashMap::new();
    let mut state : DicomKwEltDict = HashMap::new();
    while off < data.len() - 2 {
        let (gelt, elt) = element(dict, data, &mut off, evr, Some(&elements));
        let tag = u16tou32(&[gelt.1, gelt.0] );
        if dict.contains_key(&tag) {
            let ref dictelt = dict[&tag];
            let keyword = dictelt.keyword;
            assert!(!state.contains_key(keyword));
            state.insert(keyword.to_string(), elt.to_owned());
        } else {
            //println!("tag: {:08X} - {:04X} {:04X} not found in dict", tag, gelt.0, gelt.1);
        }
        assert!(!elements.contains_key(&tag));
        //println!("tag: {:08X} off: {}", tag, off);
        elements.insert(tag, elt);
    }
    Ok(DicomSlice { keydict : state } )
}
