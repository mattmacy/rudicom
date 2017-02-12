use std::collections::HashMap;
use rulinalg::matrix::Matrix;

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

#[derive(Debug, Clone)]
pub enum DicomElt {
    Int16s(Vec<i16>),
    UInt16s(Vec<u16>),
    UInt16m(Matrix<u16>),
    Int32s(Vec<i32>),
    UInt32s(Vec<u32>),
    Float64s(Vec<f64>),
    Float32s(Vec<f32>),
    Seq(Vec<DicomElt>),
    String(String),
    Bytes(Vec<u8>),
    Empty,
}

pub type DicomDict<'a> = HashMap<u32, DicomDictElt<'a>>;
pub type DicomObjectDict<'a> = HashMap<u32, DicomElt>;
pub type DicomKeywordDict<'a> = HashMap<&'a str, DicomElt>;
