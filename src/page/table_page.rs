use anyhow::{anyhow, Result};

use crate::{
    common::{PageID, TransactionID, INVALID_PAGE_ID, LSN, PAGE_SIZE},
    tuple::Tuple,
};

use super::{PageType, PAGE_ID_OFFSET, PAGE_ID_SIZE, PAGE_TYPE_OFFSET, PAGE_TYPE_SIZE};

pub const TABLE_PAGE_PAGE_TYPE: PageType = PageType(1);

const LSN_OFFSET: usize = PAGE_ID_OFFSET + PAGE_ID_SIZE;
const LSN_SIZE: usize = 8;
const NEXT_PAGE_ID_OFFSET: usize = LSN_OFFSET + LSN_SIZE;
const NEXT_PAGE_ID_SIZE: usize = 4;
const LOWER_OFFSET_OFFSET: usize = NEXT_PAGE_ID_OFFSET + NEXT_PAGE_ID_SIZE;
const LOWER_OFFSET_SIZE: usize = 4;
const UPPER_OFFSET_OFFSET: usize = LOWER_OFFSET_OFFSET + LOWER_OFFSET_SIZE;
const UPPER_OFFSET_SIZE: usize = 4;
const HEADER_SIZE: usize = PAGE_TYPE_SIZE
    + PAGE_ID_SIZE
    + LSN_SIZE
    + NEXT_PAGE_ID_SIZE
    + LOWER_OFFSET_SIZE
    + UPPER_OFFSET_SIZE;
const LINE_POINTER_OFFSET_SIZE: usize = 4;
const LINE_POINTER_SIZE_SIZE: usize = 4;
const LINE_POINTER_SIZE: usize = LINE_POINTER_OFFSET_SIZE + LINE_POINTER_SIZE_SIZE;

#[derive(Debug)]
pub struct TablePage {
    pub data: Box<[u8]>,
}

impl TablePage {
    pub fn new(page_id: PageID) -> Self {
        let mut data = vec![0u8; PAGE_SIZE];
        data[PAGE_TYPE_OFFSET..(PAGE_TYPE_OFFSET + PAGE_TYPE_SIZE)]
            .copy_from_slice(&TABLE_PAGE_PAGE_TYPE.0.to_le_bytes());
        data[PAGE_ID_OFFSET..(PAGE_ID_OFFSET + PAGE_ID_SIZE)]
            .copy_from_slice(&page_id.0.to_le_bytes());
        data[NEXT_PAGE_ID_OFFSET..(NEXT_PAGE_ID_OFFSET + NEXT_PAGE_ID_SIZE)]
            .copy_from_slice(&INVALID_PAGE_ID.0.to_le_bytes());
        data[LOWER_OFFSET_OFFSET..(LOWER_OFFSET_OFFSET + LOWER_OFFSET_SIZE)]
            .copy_from_slice(&(HEADER_SIZE as u32).to_le_bytes());
        data[UPPER_OFFSET_OFFSET..(UPPER_OFFSET_OFFSET + UPPER_OFFSET_SIZE)]
            .copy_from_slice(&(PAGE_SIZE as u32).to_le_bytes());
        TablePage { data: data.into() }
    }
    pub fn from_data(data: &[u8]) -> Self {
        TablePage { data: data.into() }
    }
    pub fn insert(&mut self, data: &[u8]) -> Result<()> {
        // TODO: too large for one page
        if self.free_space() < data.len() + LINE_POINTER_SIZE {
            return Err(anyhow!("free space not enough"));
        }

        let data_size = data.len() as u32;
        let lower_offset = self.lower_offset();
        let upper_offset = self.upper_offset();
        let next_lower_offset: u32 = lower_offset + LINE_POINTER_SIZE as u32;
        let next_upper_offset: u32 = upper_offset - data.len() as u32;
        self.data[LOWER_OFFSET_OFFSET..(LOWER_OFFSET_OFFSET + LOWER_OFFSET_SIZE)]
            .copy_from_slice(&next_lower_offset.to_le_bytes());
        self.data[UPPER_OFFSET_OFFSET..(UPPER_OFFSET_OFFSET + UPPER_OFFSET_SIZE)]
            .copy_from_slice(&next_upper_offset.to_le_bytes());
        self.data[(lower_offset as usize)..(lower_offset as usize + LINE_POINTER_OFFSET_SIZE)]
            .copy_from_slice(&next_upper_offset.to_le_bytes());
        self.data[((lower_offset as usize) + LINE_POINTER_OFFSET_SIZE)
            ..((lower_offset as usize) + LINE_POINTER_SIZE)]
            .copy_from_slice(&data_size.to_le_bytes());
        self.data[(next_upper_offset as usize)..(upper_offset as usize)].copy_from_slice(data);

        Ok(())
    }
    pub fn delete(&mut self, index: u32, txn_id: TransactionID) {
        let offset = self.line_pointer_offset(index as usize) as usize;
        let size = self.line_pointer_size(index as usize) as usize;
        let mut tuple = Tuple::new(None, &self.data[offset..(offset + size)]);
        tuple.set_xmax(txn_id);
        self.data[offset..(offset + size)].copy_from_slice(&tuple.data);
    }
    pub fn get_tuples(&self) -> Vec<Box<[u8]>> {
        let count = self.tuple_count();
        (0..count).map(|i| self.get_tuple(i)).collect()
    }
    pub fn get_tuple(&self, index: usize) -> Box<[u8]> {
        let offset = self.line_pointer_offset(index) as usize;
        let size = self.line_pointer_size(index) as usize;
        self.data[offset..(offset + size)].into()
    }
    pub fn tuple_count(&self) -> usize {
        let lower_offset = self.lower_offset();
        (lower_offset as usize - HEADER_SIZE) / LINE_POINTER_SIZE
    }
    pub fn page_id(&self) -> PageID {
        let mut bytes = [0u8; 4];
        bytes.copy_from_slice(&self.data[PAGE_ID_OFFSET..(PAGE_ID_OFFSET + PAGE_ID_SIZE)]);
        PageID(u32::from_le_bytes(bytes))
    }
    pub fn lsn(&self) -> LSN {
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&self.data[LSN_OFFSET..(LSN_OFFSET + LSN_SIZE)]);
        LSN(u64::from_le_bytes(bytes))
    }
    pub fn next_page_id(&self) -> PageID {
        let mut bytes = [0u8; 4];
        bytes.copy_from_slice(
            &self.data[NEXT_PAGE_ID_OFFSET..(NEXT_PAGE_ID_OFFSET + NEXT_PAGE_ID_SIZE)],
        );
        PageID(u32::from_le_bytes(bytes))
    }
    pub fn set_lsn(&mut self, lsn: LSN) {
        self.data[LSN_OFFSET..(LSN_OFFSET + LSN_SIZE)].copy_from_slice(&lsn.0.to_le_bytes());
    }
    pub fn set_next_page_id(&mut self, page_id: PageID) {
        self.data[NEXT_PAGE_ID_OFFSET..(NEXT_PAGE_ID_OFFSET + NEXT_PAGE_ID_SIZE)]
            .copy_from_slice(&page_id.0.to_le_bytes());
    }
    fn free_space(&self) -> usize {
        let lower_offset = self.lower_offset();
        let upper_offset = self.upper_offset();
        (upper_offset - lower_offset) as usize
    }
    fn lower_offset(&self) -> u32 {
        let mut bytes = [0u8; 4];
        bytes.copy_from_slice(
            &self.data[LOWER_OFFSET_OFFSET..(LOWER_OFFSET_OFFSET + LOWER_OFFSET_SIZE)],
        );
        u32::from_le_bytes(bytes)
    }
    fn upper_offset(&self) -> u32 {
        let mut bytes = [0u8; 4];
        bytes.copy_from_slice(
            &self.data[UPPER_OFFSET_OFFSET..(UPPER_OFFSET_OFFSET + UPPER_OFFSET_SIZE)],
        );
        u32::from_le_bytes(bytes)
    }
    fn line_pointer_offset(&self, index: usize) -> u32 {
        let offset = HEADER_SIZE + index * LINE_POINTER_SIZE;
        let mut bytes = [0u8; 4];
        bytes.copy_from_slice(&self.data[offset..(offset + LINE_POINTER_OFFSET_SIZE)]);
        u32::from_le_bytes(bytes)
    }
    fn line_pointer_size(&self, index: usize) -> u32 {
        let offset = HEADER_SIZE + index * LINE_POINTER_SIZE + LINE_POINTER_OFFSET_SIZE;
        let mut bytes = [0u8; 4];
        bytes.copy_from_slice(&self.data[offset..(offset + LINE_POINTER_SIZE_SIZE)]);
        u32::from_le_bytes(bytes)
    }
}
