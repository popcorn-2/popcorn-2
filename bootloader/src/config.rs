use alloc::vec::Vec;
use hashbrown::HashMap;
use serde::{Deserialize, Deserializer};
use serde::de::Error;
use uefi::{CString16, Guid};
use uefi::fs::PathBuf;

#[derive(Deserialize, Debug)]
pub struct Config<'a> {
    #[serde(borrow)]
    pub fonts: FontList<'a>,
    #[serde(rename = "kernel")]
    pub kernel_config: KernelConfig<'a>
}

#[derive(Deserialize, Debug)]
pub struct FontList<'a> {
    pub default: &'a str,
    #[serde(flatten, rename = "fonts")]
    pub font_list: HashMap<&'a str, Font>
}

#[derive(Deserialize, Debug)]
pub struct Font {
    #[serde(deserialize_with = "deserialize_pathbuf")]
    pub regular: PathBuf,
    #[serde(deserialize_with = "deserialize_optional_pathbuf", default)]
    pub bold: Option<PathBuf>,
    #[serde(deserialize_with = "deserialize_optional_pathbuf", default)]
    pub italic: Option<PathBuf>,
    #[serde(deserialize_with = "deserialize_optional_pathbuf", default)]
    pub bold_italic: Option<PathBuf>
}

#[derive(Deserialize, Debug)]
pub struct KernelConfig<'a> {
    #[serde(deserialize_with = "deserialize_guid")]
    pub root_disk: Guid,
    pub image: &'a str,
    pub modules: Vec<&'a str>
}

fn deserialize_guid<'de, D>(deserializer: D) -> Result<Guid, D::Error> where D: Deserializer<'de> {
    let str = <&str as Deserialize>::deserialize(deserializer)?;
    Guid::try_parse(str).map_err(Error::custom)
}

fn deserialize_pathbuf<'de, D>(deserializer: D) -> Result<PathBuf, D::Error> where D: Deserializer<'de> {
    let str = <&str as Deserialize>::deserialize(deserializer)?;
    let str = CString16::try_from(str).map_err(Error::custom)?;
    Ok(PathBuf::from(str))
}

fn deserialize_optional_pathbuf<'de, D>(deserializer: D) -> Result<Option<PathBuf>, D::Error> where D: Deserializer<'de> {
    let Some(str) = <Option<&str> as Deserialize>::deserialize(deserializer)? else {
        return Ok(None);
    };

    let str = CString16::try_from(str).map_err(Error::custom)?;
    Ok(Some(PathBuf::from(str)))
}
