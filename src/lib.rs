use std::collections::HashMap;
mod dicom_types;
mod dicom_dict;
use dicom_types::DicomDict;
use dicom_types::DicomDictElt;
use dicom_dict::dicom_dictionary_init;


#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn it_works() {
        let mydict : DicomDict = dicom_dictionary_init();
        for (key, val) in mydict.iter() {
            println!("key: {} val: {:?}", key, val);
        }
    }
}
