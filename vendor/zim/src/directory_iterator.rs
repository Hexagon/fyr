use crate::directory_entry::DirectoryEntry;
use crate::zim::Zim;

pub struct DirectoryIterator<'a> {
    max: u32,
    next: u32,
    zim: &'a Zim,
}

impl<'a> DirectoryIterator<'a> {
    pub fn new(zim: &'a Zim) -> DirectoryIterator<'a> {
        DirectoryIterator {
            max: zim.header.article_count,
            next: 0,
            zim,
        }
    }
}

impl<'a> std::iter::Iterator for DirectoryIterator<'a> {
    type Item = DirectoryEntry;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next >= self.max {
            return None;
        }

        let dir_entry_ptr = self.zim.url_list[self.next as usize] as usize;
        self.next += 1;
        let slice = self.zim.master_view.get(dir_entry_ptr..);

        match slice {
            Some(slice) => DirectoryEntry::new(self.zim, slice).ok(),
            None => None,
        }
    }
}
