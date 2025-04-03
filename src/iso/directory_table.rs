use byteorder::{LE, ReadBytesExt};

use std::io::{Read, Seek, SeekFrom};

use bitflags::bitflags;

use anyhow::Error;

use super::*;

pub struct DirectoryTable {
    pub sector: u32,
    pub size: u32,
    pub entries: Vec<DirectoryEntry>,
}

pub struct DirectoryEntry {
    pub attributes: DirectoryEntryAttributes,
    pub name: String,
    pub name_length: u8,
    pub sector: u32,
    pub size: u32,
    pub subtree_left: u16,
    pub subtree_right: u16,
    pub subdirectory: Option<DirectoryTable>,
}

bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct DirectoryEntryAttributes: u8 {
        const ARCHIVE = 0x20;
        const DIRECTORY = 0x10;
        const HIDDEN = 0x02;
        const NORMAL = 0x80;
        const READ_ONLY = 0x01;
        const SYSTEM = 0x04;
    }
}

impl DirectoryTable {
    pub fn read_root<R: Read + Seek>(
        mut reader: R,
        volume: &VolumeDescriptor,
    ) -> Result<DirectoryTable, Error> {
        Self::read(
            &mut reader,
            volume,
            volume.root_directory_sector,
            volume.root_directory_size,
        )
    }

    fn read<R: Read + Seek>(
        reader: &mut R,
        volume: &VolumeDescriptor,
        sector: u32,
        size: u32,
    ) -> Result<DirectoryTable, Error> {
        let mut entries = Vec::<DirectoryEntry>::new();

        let sector_count = size.div_ceil(SECTOR_SIZE as u32);
        for sector_index in 0..sector_count {
            let sector_position =
                ((sector + sector_index) as u64) * volume.sector_size + volume.root_offset;
            reader.seek(SeekFrom::Start(sector_position))?;

            while let Some(entry) = DirectoryEntry::read(reader, volume)? {
                entries.push(entry);
            }
        }

        Ok(DirectoryTable {
            sector,
            size,
            entries,
        })
    }

    pub fn get_entry(&self, name: &str) -> Option<&DirectoryEntry> {
        self.entries
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(name))
    }
}

impl DirectoryEntry {
    fn read<R: Read + Seek>(
        reader: &mut R,
        volume: &VolumeDescriptor,
    ) -> Result<Option<DirectoryEntry>, Error> {
        let subtree_left = reader.read_u16::<LE>()?;
        let subtree_right = reader.read_u16::<LE>()?;

        if subtree_left == 0xffff || subtree_right == 0xffff {
            return Ok(None);
        }

        let sector = reader.read_u32::<LE>()?;
        let size = reader.read_u32::<LE>()?;

        // This entry is empty
        if size == 0 {
            return Ok(None);
        }

        let attributes = DirectoryEntryAttributes::from_bits_truncate(reader.read_u8()?);

        let name_length = reader.read_u8()?;

        let mut name = vec![0_u8; name_length as usize];
        reader.read_exact(&mut name)?;
        let name = String::from_utf8_lossy(&name).into_owned();

        let alignment_mismatch = ((4 - reader.stream_position()? % 4) % 4) as i64;
        reader.seek(SeekFrom::Current(alignment_mismatch))?;

        let is_directory = attributes.contains(DirectoryEntryAttributes::DIRECTORY);
        let subdirectory = if is_directory {
            let reader_position = reader.stream_position()?;
            let subdir = DirectoryTable::read(reader, volume, sector, size)?;
            reader.seek(SeekFrom::Start(reader_position))?;
            Some(subdir)
        } else {
            None
        };

        Ok(Some(DirectoryEntry {
            subtree_left,
            subtree_right,
            sector,
            size,
            attributes,
            name_length,
            name,
            subdirectory,
        }))
    }

    pub fn is_directory(&self) -> bool {
        self.attributes
            .contains(DirectoryEntryAttributes::DIRECTORY)
    }
}
