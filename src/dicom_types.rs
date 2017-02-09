use std::collections::HashMap;

#[derive(Debug)]
pub struct DicomDictElt<'a> {
    pub vr: &'a str,
    pub vm: &'a str,
    pub name: &'a str,
    pub retired: &'a str,
    pub keyword: &'a str,
}

pub struct DicomObject<'a> {
    pub odict: DicomObjectDict<'a>,
    pub keydict: DicomKeywordDict<'a>,
}

pub struct SeqItem {
}


#[derive(Debug)]
pub enum DicomElt<'a> {
    Int16(i16),
    Int32(i32),
    UInt16(u16),
    UInt16s(Vec<u16>),
    UInt32(u32),
    Float32(f32),
    Float64(f64),
    Float64s(Vec<f64>),
    Float32s(Vec<f32>),
    Seq(Vec<DicomElt<'a>>),
    String(&'a str),
    Bytes(&'a [u8]),
    Empty,
}

pub type DicomDict<'a> = HashMap<u32, DicomDictElt<'a>>;
pub type DicomObjectDict<'a> = HashMap<u32, DicomElt<'a>>;
pub type DicomKeywordDict<'a> = HashMap<&'a str, DicomElt<'a>>;
