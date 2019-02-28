// Copyright (C) 2019 Alibaba Cloud Computing. All rights reserved.
// SPDX-License-Identifier: Apache-2.0
//
// Portions Copyright 2018 Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0
//
// Portions Copyright 2017 The Chromium OS Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the THIRD-PARTY file.

//! A default implementation of GuestMemory by mmap()-ing guest's memory into current process.
//!
//! The main structs to access guest's memory are:
//! - MmapRegion: mmap a continuous region of guest's memory into current process
//! - GuestRegionMmap: map from guest physical address into mmapped offset
//! - GuestMemoryMmap: manage a collection of GuestRegionMmap objects

use libc;
use std::io::{self, Read, Write};
use std::os::unix::io::AsRawFd;
use std::ptr::null_mut;
use std::sync::Arc;

use address::Address;
use guest_memory::*;
use volatile_memory::{self, calc_offset, VolatileMemory, VolatileSlice};
use Bytes;

/// A backend driver to access guest's physical memory by mmapping guest's memory into current
/// process.
/// For a combination of 32-bit hypervisor and 64-bit virtual machine, only partial of guest's
/// physical memory may be mapped into current process due to limited process virtual address
/// space size.
#[derive(Debug)]
pub struct MmapRegion {
    addr: *mut u8,
    size: usize,
}

/// Errors that can happen when creating a memory map
#[derive(Debug)]
pub enum MmapError {
    /// Syscall returned the given error.
    SystemCallFailed(io::Error),
    /// No memory region found.
    NoMemoryRegion,
    /// Some of the memory regions intersect with each other.
    MemoryRegionOverlap,
}

// Send and Sync aren't automatically inherited for the raw address pointer.
// Accessing that pointer is only done through the stateless interface which
// allows the object to be shared by multiple threads without a decrease in
// safety.
unsafe impl Send for MmapRegion {}
unsafe impl Sync for MmapRegion {}

impl MmapRegion {
    /// Creates an anonymous shared mapping of `size` bytes.
    ///
    /// # Arguments
    /// * `size` - Size of memory region in bytes.
    pub fn new(size: usize) -> io::Result<Self> {
        // This is safe because we are creating an anonymous mapping in a place not already used by
        // any other area in this process.
        let addr = unsafe {
            libc::mmap(
                null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_ANONYMOUS | libc::MAP_SHARED | libc::MAP_NORESERVE,
                -1,
                0,
            )
        };
        if addr == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            addr: addr as *mut u8,
            size,
        })
    }

    /// Maps the `size` bytes starting at `offset` bytes of the given `fd`.
    ///
    /// # Arguments
    /// * `fd` - File descriptor to mmap from.
    /// * `size` - Size of memory region in bytes.
    /// * `offset` - Offset in bytes from the beginning of `fd` to start the mmap.
    pub fn from_fd(fd: &AsRawFd, size: usize, offset: libc::off_t) -> io::Result<Self> {
        // This is safe because we are creating a mapping in a place not already used by any other
        // area in this process.
        let addr = unsafe {
            libc::mmap(
                null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd.as_raw_fd(),
                offset as libc::off_t,
            )
        };
        if addr == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            addr: addr as *mut u8,
            size,
        })
    }

    /// Returns a pointer to the beginning of the memory region.  Should only be
    /// used for passing this region to ioctls for setting guest memory.
    pub fn as_ptr(&self) -> *mut u8 {
        self.addr
    }

    unsafe fn as_slice(&self) -> &[u8] {
        // This is safe because we mapped the area at addr ourselves, so this slice will not
        // overflow. However, it is possible to alias.
        std::slice::from_raw_parts(self.addr, self.size)
    }

    unsafe fn as_mut_slice(&self) -> &mut [u8] {
        // This is safe because we mapped the area at addr ourselves, so this slice will not
        // overflow. However, it is possible to alias.
        std::slice::from_raw_parts_mut(self.addr, self.size)
    }
}

impl VolatileMemory for MmapRegion {
    fn len(&self) -> usize {
        self.size
    }

    fn get_slice(&self, offset: usize, count: usize) -> volatile_memory::Result<VolatileSlice> {
        let end = calc_offset(offset, count)?;
        if end > self.size {
            return Err(volatile_memory::Error::OutOfBounds { addr: end });
        }

        // Safe because we checked that offset + count was within our range and we only ever hand
        // out volatile accessors.
        Ok(unsafe { VolatileSlice::new((self.addr as usize + offset) as *mut _, count) })
    }
}

impl Drop for MmapRegion {
    fn drop(&mut self) {
        // This is safe because we mmap the area at addr ourselves, and nobody
        // else is holding a reference to it.
        unsafe {
            libc::munmap(self.addr as *mut libc::c_void, self.size);
        }
    }
}

/// Tracks a mapping of memory in the current process and the corresponding base address
/// in the guest's memory space.
pub struct GuestRegionMmap {
    mapping: MmapRegion,
    guest_base: GuestAddress,
}

impl GuestRegionMmap {
    /// Create a new memory-mapped memory region for guest's physical memory.
    /// Note: caller needs to ensure that (mapping.len() + guest_base) doesn't wrapping around.
    pub fn new(mapping: MmapRegion, guest_base: GuestAddress) -> Self {
        GuestRegionMmap {
            mapping,
            guest_base,
        }
    }

    fn as_volatile_slice(&self) -> VolatileSlice {
        self.mapping.as_volatile_slice()
    }
}

impl Bytes<MemoryRegionAddress> for GuestRegionMmap {
    type E = Error;

    /// # Examples
    /// * Write a slice at guest address 0x1200.
    ///
    /// ```
    /// # use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};
    /// # let start_addr = GuestAddress(0x1000);
    /// # let mut gm = GuestMemoryMmap::new(&vec![(start_addr, 0x400)]).unwrap();
    ///   let res = gm.write(&[1,2,3,4,5], GuestAddress(0x1200)).unwrap();
    ///   assert_eq!(5, res);
    /// ```
    fn write(&self, buf: &[u8], addr: MemoryRegionAddress) -> Result<usize> {
        let maddr = addr.raw_value() as usize;
        self.as_volatile_slice()
            .write(buf, maddr)
            .map_err(Into::into)
    }

    /// # Examples
    /// * Read a slice of length 16 at guestaddress 0x1200.
    ///
    /// ```
    /// # use vm_memory::{Bytes, GuestAddress, GuestMemoryMmap};
    /// # let start_addr = GuestAddress(0x1000);
    /// # let mut gm = GuestMemoryMmap::new(&vec![(start_addr, 0x400)]).unwrap();
    ///   let buf = &mut [0u8; 16];
    ///   let res = gm.read(buf, GuestAddress(0x1200)).unwrap();
    ///   assert_eq!(16, res);
    /// ```
    fn read(&self, buf: &mut [u8], addr: MemoryRegionAddress) -> Result<usize> {
        let maddr = addr.raw_value() as usize;
        self.as_volatile_slice()
            .read(buf, maddr)
            .map_err(Into::into)
    }

    fn write_slice(&self, buf: &[u8], addr: MemoryRegionAddress) -> Result<()> {
        let maddr = addr.raw_value() as usize;
        self.as_volatile_slice()
            .write_slice(buf, maddr)
            .map_err(Into::into)
    }

    fn read_slice(&self, buf: &mut [u8], addr: MemoryRegionAddress) -> Result<()> {
        let maddr = addr.raw_value() as usize;
        self.as_volatile_slice()
            .read_slice(buf, maddr)
            .map_err(Into::into)
    }

    /// # Examples
    ///
    /// * Read bytes from /dev/urandom
    ///
    /// ```
    /// # use vm_memory::{Address, Bytes, GuestAddress, GuestMemoryMmap};
    /// # use std::fs::File;
    /// # use std::path::Path;
    /// # let start_addr = GuestAddress(0x1000);
    /// # let gm = GuestMemoryMmap::new(&vec![(start_addr, 0x400)]).unwrap();
    ///   let mut file = File::open(Path::new("/dev/urandom")).unwrap();
    ///   let addr = GuestAddress(0x1010);
    ///   gm.write_from_stream(addr, &mut file, 128).unwrap();
    ///   let read_addr = addr.checked_add(8).unwrap();
    ///   let _: u32 = gm.read_obj(read_addr).unwrap();
    /// ```
    fn write_from_stream<F>(
        &self,
        addr: MemoryRegionAddress,
        src: &mut F,
        count: usize,
    ) -> Result<()>
    where
        F: Read,
    {
        let maddr = addr.raw_value() as usize;
        self.as_volatile_slice()
            .write_from_stream::<F>(maddr, src, count)
            .map_err(Into::into)
    }

    /// Reads data from the region to a writable object.
    ///
    /// # Examples
    ///
    /// * Write 128 bytes to /dev/null
    ///
    /// ```
    /// # use vm_memory::{Address, Bytes, GuestAddress, GuestMemoryMmap};
    /// # use std::fs::OpenOptions;
    /// # let start_addr = GuestAddress(0x1000);
    /// # let gm = GuestMemoryMmap::new(&vec![(start_addr, 0x400)]).unwrap();
    ///   let mut file = OpenOptions::new().write(true).open("/dev/null").unwrap();
    ///   let mut mem = [0u8; 1024];
    ///   gm.read_into_stream(start_addr, &mut file, 128).unwrap();
    /// ```
    fn read_into_stream<F>(
        &self,
        addr: MemoryRegionAddress,
        dst: &mut F,
        count: usize,
    ) -> Result<()>
    where
        F: Write,
    {
        let maddr = addr.raw_value() as usize;
        self.as_volatile_slice()
            .read_into_stream::<F>(maddr, dst, count)
            .map_err(Into::into)
    }
}

impl GuestMemoryRegion for GuestRegionMmap {
    fn len(&self) -> GuestAddressValue {
        self.mapping.len() as GuestAddressValue
    }

    fn min_addr(&self) -> GuestAddress {
        self.guest_base
    }

    unsafe fn as_slice(&self) -> Option<&[u8]> {
        Some(self.mapping.as_slice())
    }

    unsafe fn as_mut_slice(&self) -> Option<&mut [u8]> {
        Some(self.mapping.as_mut_slice())
    }
}

/// Tracks memory regions allocated/mapped for the guest in the current process.
#[derive(Clone)]
pub struct GuestMemoryMmap {
    regions: Arc<Vec<GuestRegionMmap>>,
}

impl GuestMemoryMmap {
    /// Creates a container and allocates anonymous memory for guest memory regions.
    /// Valid memory regions are specified as a Vec of (Address, Size) tuples sorted by Address.
    pub fn new(ranges: &[(GuestAddress, usize)]) -> std::result::Result<Self, MmapError> {
        if ranges.is_empty() {
            return Err(MmapError::NoMemoryRegion);
        }

        let mut regions = Vec::<GuestRegionMmap>::new();
        for range in ranges.iter() {
            if let Some(last) = regions.last() {
                if last
                    .guest_base
                    .checked_add(last.mapping.len() as GuestAddressValue)
                    .map_or(true, |a| a > range.0)
                {
                    return Err(MmapError::MemoryRegionOverlap);
                }
            }

            let mapping = MmapRegion::new(range.1).map_err(|e| MmapError::SystemCallFailed(e))?;
            regions.push(GuestRegionMmap {
                mapping,
                guest_base: range.0,
            });
        }

        Ok(Self {
            regions: Arc::new(regions),
        })
    }
}

impl GuestMemory for GuestMemoryMmap {
    type R = GuestRegionMmap;

    fn num_regions(&self) -> usize {
        self.regions.len()
    }

    fn find_region(&self, addr: GuestAddress) -> Option<&GuestRegionMmap> {
        for region in self.regions.iter() {
            if addr >= region.min_addr() && addr < region.max_addr() {
                return Some(region);
            }
        }
        None
    }

    fn with_regions<F>(&self, cb: F) -> Result<()>
    where
        F: Fn(usize, &GuestRegionMmap) -> Result<()>,
    {
        for (index, region) in self.regions.iter().enumerate() {
            cb(index, region)?;
        }
        Ok(())
    }

    fn with_regions_mut<F>(&self, mut cb: F) -> Result<()>
    where
        F: FnMut(usize, &GuestRegionMmap) -> Result<()>,
    {
        for (index, region) in self.regions.iter().enumerate() {
            cb(index, region)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    extern crate tempfile;

    use self::tempfile::tempfile;
    use super::*;
    use std::fs::File;
    use std::mem;
    use std::os::unix::io::FromRawFd;
    use std::path::Path;

    use Bytes;

    #[test]
    fn basic_map() {
        let m = MmapRegion::new(1024).unwrap();
        assert_eq!(1024, m.len());
    }

    #[test]
    fn map_invalid_size() {
        let e = MmapRegion::new(0).unwrap_err();
        assert_eq!(e.raw_os_error(), Some(libc::EINVAL));
    }

    #[test]
    fn map_invalid_fd() {
        let fd = unsafe { std::fs::File::from_raw_fd(-1) };
        let e = MmapRegion::from_fd(&fd, 1024, 0).unwrap_err();
        assert_eq!(e.raw_os_error(), Some(libc::EBADF));
    }

    #[test]
    fn slice_addr() {
        let m = MmapRegion::new(5).unwrap();
        let s = m.get_slice(2, 3).unwrap();
        assert_eq!(s.as_ptr(), unsafe { m.as_ptr().offset(2) });
    }

    #[test]
    fn mapped_file_read() {
        let mut f = tempfile().unwrap();
        let sample_buf = &[1, 2, 3, 4, 5];
        assert!(f.write_all(sample_buf).is_ok());

        let mem_map = MmapRegion::from_fd(&f, sample_buf.len(), 0).unwrap();
        let buf = &mut [0u8; 16];
        assert_eq!(
            mem_map.as_volatile_slice().read(buf, 0).unwrap(),
            sample_buf.len()
        );
        assert_eq!(buf[0..sample_buf.len()], sample_buf[..]);
    }

    #[test]
    fn test_regions() {
        // No regions provided should return error.
        assert_eq!(
            format!("{:?}", GuestMemoryMmap::new(&vec![]).err().unwrap()),
            format!("{:?}", MmapError::NoMemoryRegion)
        );

        let start_addr1 = GuestAddress(0x0);
        let start_addr2 = GuestAddress(0x800);
        let guest_mem =
            GuestMemoryMmap::new(&vec![(start_addr1, 0x400), (start_addr2, 0x400)]).unwrap();
        assert_eq!(guest_mem.num_regions(), 2);
        assert!(guest_mem.find_region(GuestAddress(0x200)).is_some());
        assert!(guest_mem.find_region(GuestAddress(0x600)).is_none());
        assert!(guest_mem.find_region(GuestAddress(0xa00)).is_some());
        assert!(guest_mem.find_region(GuestAddress(0xc00)).is_none());
    }

    #[test]
    fn overlap_memory() {
        let start_addr1 = GuestAddress(0x0);
        let start_addr2 = GuestAddress(0x1000);
        let res = GuestMemoryMmap::new(&vec![(start_addr1, 0x2000), (start_addr2, 0x2000)]);
        assert_eq!(
            format!("{:?}", res.err().unwrap()),
            format!("{:?}", MmapError::MemoryRegionOverlap)
        );
    }

    #[test]
    fn test_read_u64() {
        let start_addr1 = GuestAddress(0x0);
        let start_addr2 = GuestAddress(0x1000);
        let bad_addr = GuestAddress(0x2001);
        let bad_addr2 = GuestAddress(0x1ffc);
        let max_addr = GuestAddress(0x2000);

        let gm = GuestMemoryMmap::new(&vec![(start_addr1, 0x1000), (start_addr2, 0x1000)]).unwrap();

        let val1: u64 = 0xaa55aa55aa55aa55;
        let val2: u64 = 0x55aa55aa55aa55aa;
        assert_eq!(
            format!("{:?}", gm.write_obj(val1, bad_addr).err().unwrap()),
            format!("InvalidGuestAddress({:?})", bad_addr)
        );
        assert_eq!(
            format!("{:?}", gm.write_obj(val1, bad_addr2).err().unwrap()),
            format!(
                "PartialBuffer {{ expected: {:?}, completed: {:?} }}",
                mem::size_of::<u64>(),
                max_addr.checked_offset_from(bad_addr2).unwrap()
            )
        );

        gm.write_obj(val1, GuestAddress(0x500)).unwrap();
        gm.write_obj(val2, GuestAddress(0x1000 + 32)).unwrap();
        let num1: u64 = gm.read_obj(GuestAddress(0x500)).unwrap();
        let num2: u64 = gm.read_obj(GuestAddress(0x1000 + 32)).unwrap();
        assert_eq!(val1, num1);
        assert_eq!(val2, num2);
    }

    #[test]
    fn write_and_read() {
        let mut start_addr = GuestAddress(0x1000);
        let gm = GuestMemoryMmap::new(&vec![(start_addr, 0x400)]).unwrap();
        let sample_buf = &[1, 2, 3, 4, 5];

        assert_eq!(gm.write(sample_buf, start_addr).unwrap(), 5);

        let buf = &mut [0u8; 5];
        assert_eq!(gm.read(buf, start_addr).unwrap(), 5);
        assert_eq!(buf, sample_buf);

        start_addr = GuestAddress(0x13ff);
        assert_eq!(gm.write(sample_buf, start_addr).unwrap(), 1);
        assert_eq!(gm.read(buf, start_addr).unwrap(), 1);
        assert_eq!(buf[0], sample_buf[0]);
    }

    #[test]
    fn read_to_and_write_from_mem() {
        let gm = GuestMemoryMmap::new(&vec![(GuestAddress(0x1000), 0x400)]).unwrap();
        let addr = GuestAddress(0x1010);
        gm.write_obj(!0u32, addr).unwrap();
        gm.write_from_stream(
            addr,
            &mut File::open(Path::new("/dev/zero")).unwrap(),
            mem::size_of::<u32>(),
        )
        .unwrap();
        let value: u32 = gm.read_obj(addr).unwrap();
        assert_eq!(value, 0);

        let mut sink = Vec::new();
        gm.read_into_stream(addr, &mut sink, mem::size_of::<u32>())
            .unwrap();
        assert_eq!(sink, vec![0; mem::size_of::<u32>()]);
    }

    #[test]
    fn create_vec_with_regions() {
        let region_size = 0x400;
        let regions = vec![
            (GuestAddress(0x0), region_size),
            (GuestAddress(0x1000), region_size),
        ];
        let mut iterated_regions = Vec::new();
        let gm = GuestMemoryMmap::new(&regions).unwrap();

        let res: Result<()> = gm.with_regions(|_, region| {
            assert_eq!(region.len(), region_size as GuestAddressValue);
            Ok(())
        });
        assert!(res.is_ok());

        let res: Result<()> = gm.with_regions_mut(|_, region| {
            iterated_regions.push((region.min_addr(), region.len() as usize));
            Ok(())
        });
        assert!(res.is_ok());
        assert_eq!(regions, iterated_regions);
        assert_eq!(gm.clone().regions[0].guest_base, regions[0].0);
        assert_eq!(gm.clone().regions[1].guest_base, regions[1].0);
    }

    #[test]
    fn test_access_cross_boundary() {
        let start_addr1 = GuestAddress(0x0);
        let start_addr2 = GuestAddress(0x1000);
        let gm = GuestMemoryMmap::new(&vec![(start_addr1, 0x1000), (start_addr2, 0x1000)]).unwrap();
        let sample_buf = &[1, 2, 3, 4, 5];
        assert_eq!(gm.write(sample_buf, GuestAddress(0xffc)).unwrap(), 5);
        let buf = &mut [0u8; 5];
        assert_eq!(gm.read(buf, GuestAddress(0xffc)).unwrap(), 5);
        assert_eq!(buf, sample_buf);
    }
}
