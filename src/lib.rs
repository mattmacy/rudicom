extern crate memmap;

use std::collections::HashMap;
mod dicom_types;
use dicom_types::{DicomDict, DicomDictElt};
mod dicom_dict;
use dicom_dict::dicom_dictionary_init;
mod filereader;


#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dict_works() {
        let mydict : DicomDict = dicom_dictionary_init();
        for (key, val) in mydict.iter() {
            println!("key: {} val: {:?}", key, val);
        }
    }
    #[test]
    fn parse_works() {
        let result = filereader::parse("resources/000001.dcm");
    }
}
