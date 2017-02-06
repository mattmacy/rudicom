use std::collections::HashMap;

#[derive(Debug)]
pub struct DicomDictElt<'a> {
    pub vr: &'a str,
    pub vm: &'a str,
    pub name: &'a str,
    pub retired: &'a str,
    pub keyword: &'a str,
}

#[derive(Debug)]
pub struct DicomObject { }

pub type DicomDict<'a> = HashMap<u32, DicomDictElt<'a>>;
