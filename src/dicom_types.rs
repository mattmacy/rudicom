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

pub enum DicomElt {
    Float64(f64)
}

pub type DicomDict<'a> = HashMap<u32, DicomDictElt<'a>>;
pub type DicomObjectDict<'a> = HashMap<u32, DicomElt>;
pub type DicomKeywordDict<'a> = HashMap<&'a str, DicomElt>;
