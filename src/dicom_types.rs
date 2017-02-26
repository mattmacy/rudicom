use std::collections::HashMap;
use std::ops::Index;

#[derive(Debug)]
pub struct DicomDictElt<'a> {
    pub vr: &'a str,
    pub vm: &'a str,
    pub name: &'a str,
    pub retired: &'a str,
    pub keyword: &'a str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DicomSlice {
    pub keydict: DicomKwEltDict,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DcmImg16 {
    pub xr : usize,
    pub yr : usize,
    pub zr : usize,
    pub data : Vec<i16>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DcmImg8 {
    pub xr : usize,
    pub yr : usize,
    pub zr : usize,
    pub data : Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DicomScan {
    pub slice_data: Vec<DicomSlice>,
    pub image: DcmImg16
}


#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DicomElt {
    Int16s(Vec<i16>),
    UInt16s(Vec<u16>),
    Int32s(Vec<i32>),
    UInt32s(Vec<u32>),
    Float64s(Vec<f64>),
    Float32s(Vec<f32>),
    Seq(Vec<DicomElt>),
    String(String),
    Bytes(Vec<u8>),
    Image16(DcmImg16),
    Image8(DcmImg8),
    Empty,
}

pub type DicomDict<'a> = HashMap<u32, DicomDictElt<'a>>;
pub type DicomGeltEltDict = HashMap<u32, DicomElt>;
pub type DicomKwEltDict = HashMap<String, DicomElt>;

impl Index<String> for DicomSlice {
    type Output = DicomElt;

    fn index<'a>(&'a self, key: String) -> &'a DicomElt {
        &self.keydict[&key]
    }
}

impl DicomSlice {
    pub fn pos(&self) -> f64 {
        match self["ImagePositionPatient".to_owned()] {
            DicomElt::Float64s(ref v) => v[2],
            _ =>  panic!("no patient position"),
        }
    }

    pub fn pixel_data(&self) -> &DcmImg16 {
        match self["PixelData".to_owned()] {
            DicomElt::Image16(ref v) => v,
            _ =>  panic!("unexpected image type"),
        }
    }
    pub fn slope(&self) -> f64 {
        match self["RescaleSlope".to_owned()] {
            DicomElt::Float64s(ref v) => v[0],
            _ => panic!("unknown slope type"),
        }
    }
    pub fn intercept(&self) -> i16 {
        match self["RescaleIntercept".to_owned()] {
            DicomElt::UInt32s(ref v)  => v[0] as i16,
            DicomElt::Float64s(ref v) => v[0] as i16,
            _ => panic!("unknown intercept type"),
        }
    }
    pub fn thickness(&self) -> f64 {
        match self["SliceThickness".to_owned()] {
            DicomElt::Float64s(ref v) => v[0],
            _ => 0.0,
        }
    }
}

