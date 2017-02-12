use std::collections::HashMap;

#[derive(Debug)]
pub struct DicomDictElt<'a> {
    pub vr: &'a str,
    pub vm: &'a str,
    pub name: &'a str,
    pub retired: &'a str,
    pub keyword: &'a str,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DicomObject {
    pub keydict: DicomKeywordDict,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DcmImg16 {
    pub xr : usize,
    pub yr : usize,
    pub zr : usize,
    pub data : Vec<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DcmImg8 {
    pub xr : usize,
    pub yr : usize,
    pub zr : usize,
    pub data : Vec<u8>,
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
pub type DicomObjectDict<'a> = HashMap<u32, DicomElt>;
pub type DicomKeywordDict = HashMap<String, DicomElt>;
