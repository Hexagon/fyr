use std::convert::TryFrom;

use crate::errors::{Error, Result};

/// Namespaces seperate different types of directory entries - which might have the same title -
/// stored in the ZIM File Format.
#[derive(Debug, Copy, Clone)]
#[repr(u8)]
pub enum Namespace {
    Layout = b'-',
    Articles = b'A',
    ArticleMetaData = b'B',
    UserContent = b'C',
    ImagesFile = b'I',
    ImagesText = b'J',
    Metadata = b'M',
    CategoriesText = b'U',
    CategoriesArticleList = b'V',
    CategoriesArticle = b'W',
    FulltextIndex = b'X',
}

impl TryFrom<u8> for Namespace {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self> {
        use Namespace::*;
        match value {
            b'-' => Ok(Layout),
            b'A' => Ok(Articles),
            b'B' => Ok(ArticleMetaData),
            b'C' => Ok(UserContent),
            b'I' => Ok(ImagesFile),
            b'J' => Ok(ImagesText),
            b'M' => Ok(Metadata),
            b'U' => Ok(CategoriesText),
            b'V' => Ok(CategoriesArticleList),
            b'W' => Ok(CategoriesArticle),
            b'X' => Ok(FulltextIndex),
            _ => Err(Error::InvalidNamespace),
        }
    }
}
